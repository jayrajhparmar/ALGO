use anyhow::{Context, Result};
use cadconvert_core::geom::Vec2;
use cadconvert_core::model::{
    Bezier2D, Circle2D, Drawing2D, Entity2D, EntityKind, LineSeg2D, Polyline2D, PolylineVertex2D,
    Primitive2D, Style, TextEntity, Units,
};
use roxmltree::{Document, Node};
use std::path::Path;

pub fn import_svg(path: &Path) -> Result<Drawing2D> {
    let xml = std::fs::read_to_string(path).with_context(|| format!("read SVG: {path:?}"))?;
    let doc = Document::parse(&xml).with_context(|| format!("parse SVG XML: {path:?}"))?;

    let svg = doc
        .descendants()
        .find(|n| n.has_tag_name("svg"))
        .context("no <svg> root element")?;

    let vb = parse_viewbox(svg.attribute("viewBox"));
    let height = vb.map(|v| v.3);

    let mut next_id: u64 = 1;
    let mut entities = Vec::new();
    let mut texts = Vec::new();

    walk(svg, Transform2D::identity(), height, &mut next_id, &mut entities, &mut texts);

    Ok(Drawing2D {
        units: Units::Unknown,
        entities,
        dims: Vec::new(),
        texts,
    })
}

fn walk(
    node: Node<'_, '_>,
    parent_tx: Transform2D,
    svg_height: Option<f64>,
    next_id: &mut u64,
    entities: &mut Vec<Entity2D>,
    texts: &mut Vec<TextEntity>,
) {
    let node_tx = parse_transform(node.attribute("transform"));
    let tx = parent_tx.mul(node_tx);

    if node.is_element() {
        let tag = node.tag_name().name();
        match tag {
            "line" => {
                if let Some(seg) = parse_line(node, tx, svg_height) {
                    entities.push(Entity2D {
                        id: alloc_id(next_id),
                        kind: EntityKind::Unknown,
                        primitive: Primitive2D::Line(seg),
                        style: parse_style(node),
                    });
                }
            }
            "circle" => {
                if let Some(circle) = parse_circle(node, tx, svg_height) {
                    entities.push(Entity2D {
                        id: alloc_id(next_id),
                        kind: EntityKind::Unknown,
                        primitive: Primitive2D::Circle(circle),
                        style: parse_style(node),
                    });
                }
            }
            "polyline" | "polygon" => {
                if let Some(poly) = parse_polyline(node, tx, svg_height) {
                    let closed = tag == "polygon";
                    entities.push(Entity2D {
                        id: alloc_id(next_id),
                        kind: EntityKind::Unknown,
                        primitive: Primitive2D::Polyline(Polyline2D {
                            vertices: poly,
                            closed,
                        }),
                        style: parse_style(node),
                    });
                }
            }
            "path" => {
                if let Some(d) = node.attribute("d") {
                    parse_path(d, tx, svg_height, next_id, entities, node);
                }
            }
            "text" => {
                let value = node.text().unwrap_or("").trim().to_string();
                if !value.is_empty() {
                    if let Some(at) = parse_text_pos(node, tx, svg_height) {
                        texts.push(TextEntity {
                            id: alloc_id(next_id),
                            text: value,
                            at,
                            height: None,
                            style: parse_style(node),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    for c in node.children() {
        walk(c, tx, svg_height, next_id, entities, texts);
    }
}

fn alloc_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

fn parse_viewbox(viewbox: Option<&str>) -> Option<(f64, f64, f64, f64)> {
    let vb = viewbox?;
    let parts: Vec<_> = vb.split_whitespace().collect();
    if parts.len() != 4 {
        return None;
    }
    let a = parts[0].parse().ok()?;
    let b = parts[1].parse().ok()?;
    let c = parts[2].parse().ok()?;
    let d = parts[3].parse().ok()?;
    Some((a, b, c, d))
}

fn parse_style(node: Node<'_, '_>) -> Style {
    // Minimal, deterministic: preserve layer-ish metadata when available.
    // Real classification happens later based on dash patterns / stroke etc.
    let layer = node.attribute("id").map(|s| s.to_string());
    let linetype = node
        .attribute("stroke-dasharray")
        .or_else(|| node.attribute("style").and_then(find_dasharray_in_style))
        .map(|s| s.to_string());
    Style {
        layer,
        linetype,
        color_index: None,
    }
}

fn find_dasharray_in_style(style: &str) -> Option<&str> {
    // style="...;stroke-dasharray: 5, 2;..."
    for part in style.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("stroke-dasharray:") {
            return Some(rest.trim());
        }
    }
    None
}

fn parse_line(node: Node<'_, '_>, tx: Transform2D, svg_height: Option<f64>) -> Option<LineSeg2D> {
    let x1 = parse_len(node.attribute("x1")?)?;
    let y1 = parse_len(node.attribute("y1")?)?;
    let x2 = parse_len(node.attribute("x2")?)?;
    let y2 = parse_len(node.attribute("y2")?)?;
    let a = flip_y(tx.apply_point(Vec2::new(x1, y1)), svg_height);
    let b = flip_y(tx.apply_point(Vec2::new(x2, y2)), svg_height);
    Some(LineSeg2D { a, b })
}

fn parse_circle(
    node: Node<'_, '_>,
    tx: Transform2D,
    svg_height: Option<f64>,
) -> Option<Circle2D> {
    let cx = parse_len(node.attribute("cx")?)?;
    let cy = parse_len(node.attribute("cy")?)?;
    let r = parse_len(node.attribute("r")?)?;
    let c = flip_y(tx.apply_point(Vec2::new(cx, cy)), svg_height);
    // Note: transform may include scaling; we ignore non-uniform scaling for now.
    Some(Circle2D { center: c, radius: r })
}

fn parse_polyline(
    node: Node<'_, '_>,
    tx: Transform2D,
    svg_height: Option<f64>,
) -> Option<Vec<PolylineVertex2D>> {
    let points = node.attribute("points")?;
    let mut out = Vec::new();
    for pair in points.split_whitespace() {
        let mut it = pair.split(',');
        let x = it.next().and_then(parse_len)?;
        let y = it.next().and_then(parse_len)?;
        let p = flip_y(tx.apply_point(Vec2::new(x, y)), svg_height);
        out.push(PolylineVertex2D {
            pos: p,
            bulge: 0.0,
        });
    }
    if out.len() >= 2 { Some(out) } else { None }
}

fn parse_text_pos(node: Node<'_, '_>, tx: Transform2D, svg_height: Option<f64>) -> Option<Vec2> {
    let x = parse_len(node.attribute("x")?)?;
    let y = parse_len(node.attribute("y")?)?;
    Some(flip_y(tx.apply_point(Vec2::new(x, y)), svg_height))
}

fn parse_path(
    d: &str,
    tx: Transform2D,
    svg_height: Option<f64>,
    next_id: &mut u64,
    entities: &mut Vec<Entity2D>,
    node: Node<'_, '_>,
) {
    let style = parse_style(node);
    let mut cur = Vec2::new(0.0, 0.0);
    let mut start = Vec2::new(0.0, 0.0);

    let mut parser = svgtypes::PathParser::from(d);
    while let Some(seg) = parser.next() {
        let seg = match seg {
            Ok(s) => s,
            Err(_) => return,
        };
        use svgtypes::PathSegment::*;
        match seg {
            MoveTo { abs, x, y } => {
                cur = if abs {
                    Vec2::new(x, y)
                } else {
                    Vec2::new(cur.x + x, cur.y + y)
                };
                start = cur;
            }
            LineTo { abs, x, y } => {
                let next = if abs {
                    Vec2::new(x, y)
                } else {
                    Vec2::new(cur.x + x, cur.y + y)
                };
                let a = flip_y(tx.apply_point(cur), svg_height);
                let b = flip_y(tx.apply_point(next), svg_height);
                entities.push(Entity2D {
                    id: alloc_id(next_id),
                    kind: EntityKind::Unknown,
                    primitive: Primitive2D::Line(LineSeg2D { a, b }),
                    style: style.clone(),
                });
                cur = next;
            }
            CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let p1 = if abs {
                    Vec2::new(x1, y1)
                } else {
                    Vec2::new(cur.x + x1, cur.y + y1)
                };
                let p2 = if abs {
                    Vec2::new(x2, y2)
                } else {
                    Vec2::new(cur.x + x2, cur.y + y2)
                };
                let p3 = if abs {
                    Vec2::new(x, y)
                } else {
                    Vec2::new(cur.x + x, cur.y + y)
                };
                let b = Bezier2D {
                    p0: flip_y(tx.apply_point(cur), svg_height),
                    p1: flip_y(tx.apply_point(p1), svg_height),
                    p2: flip_y(tx.apply_point(p2), svg_height),
                    p3: flip_y(tx.apply_point(p3), svg_height),
                };
                entities.push(Entity2D {
                    id: alloc_id(next_id),
                    kind: EntityKind::Unknown,
                    primitive: Primitive2D::CubicBezier(b),
                    style: style.clone(),
                });
                cur = p3;
            }
            ClosePath { .. } => {
                let a = flip_y(tx.apply_point(cur), svg_height);
                let b = flip_y(tx.apply_point(start), svg_height);
                entities.push(Entity2D {
                    id: alloc_id(next_id),
                    kind: EntityKind::Unknown,
                    primitive: Primitive2D::Line(LineSeg2D { a, b }),
                    style: style.clone(),
                });
                cur = start;
            }
            _ => {
                // Deterministic v0: ignore quadratic/arc commands.
            }
        }
    }
}

fn parse_len(s: &str) -> Option<f64> {
    // Parse numeric prefix, ignore units (px/mm/etc).
    let mut end = 0usize;
    for (i, ch) in s.char_indices() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
            end = i + ch.len_utf8();
        } else {
            break;
        }
    }
    s[..end].trim().parse().ok()
}

fn parse_transform(transform: Option<&str>) -> Transform2D {
    let Some(t) = transform else {
        return Transform2D::identity();
    };

    match t.parse::<svgtypes::Transform>() {
        Ok(m) => Transform2D {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            e: m.e,
            f: m.f,
        },
        Err(_) => Transform2D::identity(),
    }
}

fn flip_y(p: Vec2, svg_height: Option<f64>) -> Vec2 {
    // SVG Y axis points down; CAD Y points up.
    match svg_height {
        Some(h) => Vec2::new(p.x, h - p.y),
        None => p,
    }
}

#[derive(Debug, Clone, Copy)]
struct Transform2D {
    // SVG affine matrix: [a c e; b d f; 0 0 1]
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Transform2D {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn mul(self, rhs: Self) -> Self {
        Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        }
    }

    fn apply_point(self, p: Vec2) -> Vec2 {
        Vec2::new(
            self.a * p.x + self.c * p.y + self.e,
            self.b * p.x + self.d * p.y + self.f,
        )
    }
}
