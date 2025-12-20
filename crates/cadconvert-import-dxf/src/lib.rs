use anyhow::{Context, Result};
use cadconvert_core::geom::Vec2;
use cadconvert_core::model::{
    Arc2D, Circle2D, DimensionEntity, Drawing2D, Entity2D, EntityKind, LineSeg2D, Polyline2D,
    PolylineVertex2D, Primitive2D, Style, TextEntity, Units,
};
use dxf::entities::EntityType;
use std::collections::HashMap;
use std::path::Path;

pub fn import_dxf(path: &Path) -> Result<Drawing2D> {
    let drawing = dxf::Drawing::load_file(path).with_context(|| format!("load DXF: {path:?}"))?;

    let mut importer = DxfImporter::new(&drawing);
    importer.import_all();

    Ok(Drawing2D {
        units: Units::Unknown,
        entities: importer.entities,
        dims: importer.dims,
        texts: importer.texts,
    })
}

struct DxfImporter<'a> {
    drawing: &'a dxf::Drawing,
    blocks: HashMap<String, &'a dxf::Block>,
    next_id: u64,
    entities: Vec<Entity2D>,
    dims: Vec<DimensionEntity>,
    texts: Vec<TextEntity>,
}

impl<'a> DxfImporter<'a> {
    fn new(drawing: &'a dxf::Drawing) -> Self {
        let mut blocks = HashMap::new();
        for block in drawing.blocks() {
            blocks.insert(block.name.to_ascii_lowercase(), block);
        }
        Self {
            drawing,
            blocks,
            next_id: 1,
            entities: Vec::new(),
            dims: Vec::new(),
            texts: Vec::new(),
        }
    }

    fn import_all(&mut self) {
        let tx = Transform2D::identity();
        let mut stack = Vec::new();
        for ent in self.drawing.entities() {
            self.import_entity(ent, &tx, None, &mut stack, 0);
        }
    }

    fn import_entity(
        &mut self,
        ent: &dxf::entities::Entity,
        tx: &Transform2D,
        parent_style: Option<&Style>,
        stack: &mut Vec<String>,
        depth: usize,
    ) {
        let style = self.resolve_style(ent, parent_style);
        match &ent.specific {
            EntityType::Insert(insert) => {
                self.import_insert(insert, &style, tx, stack, depth + 1);
            }
            EntityType::Line(line) => {
                let kind = classify_linetype(&style.linetype);
                let a = tx.apply_point(Vec2::new(line.p1.x, line.p1.y));
                let b = tx.apply_point(Vec2::new(line.p2.x, line.p2.y));
                let id = self.next_id();
                self.entities.push(Entity2D {
                    id,
                    kind,
                    primitive: Primitive2D::Line(LineSeg2D { a, b }),
                    style,
                });
            }
            EntityType::Circle(circle) => {
                let kind = classify_linetype(&style.linetype);
                let center = Vec2::new(circle.center.x, circle.center.y);
                if let Some((scale, _)) = tx.uniform_scale_rotation() {
                    let center = tx.apply_point(center);
                    let radius = circle.radius * scale;
                    let id = self.next_id();
                    self.entities.push(Entity2D {
                        id,
                        kind,
                        primitive: Primitive2D::Circle(Circle2D { center, radius }),
                        style,
                    });
                } else {
                    let vertices = circle_points(center, circle.radius, 64)
                        .into_iter()
                        .map(|p| PolylineVertex2D {
                            pos: tx.apply_point(p),
                            bulge: 0.0,
                        })
                        .collect();
                    let id = self.next_id();
                    self.entities.push(Entity2D {
                        id,
                        kind,
                        primitive: Primitive2D::Polyline(Polyline2D {
                            vertices,
                            closed: true,
                        }),
                        style,
                    });
                }
            }
            EntityType::Arc(arc) => {
                let kind = classify_linetype(&style.linetype);
                let center = Vec2::new(arc.center.x, arc.center.y);
                if let Some((scale, rot)) = tx.uniform_scale_rotation() {
                    let center = tx.apply_point(center);
                    let radius = arc.radius * scale;
                    let rot_deg = rot.to_degrees();
                    let id = self.next_id();
                    self.entities.push(Entity2D {
                        id,
                        kind,
                        primitive: Primitive2D::Arc(Arc2D {
                            center,
                            radius,
                            start_angle_deg: arc.start_angle + rot_deg,
                            end_angle_deg: arc.end_angle + rot_deg,
                        }),
                        style,
                    });
                } else {
                    let vertices = arc_points(
                        center,
                        arc.radius,
                        arc.start_angle,
                        arc.end_angle,
                        48,
                    )
                    .into_iter()
                    .map(|p| PolylineVertex2D {
                        pos: tx.apply_point(p),
                        bulge: 0.0,
                    })
                    .collect();
                    let id = self.next_id();
                    self.entities.push(Entity2D {
                        id,
                        kind,
                        primitive: Primitive2D::Polyline(Polyline2D {
                            vertices,
                            closed: false,
                        }),
                        style,
                    });
                }
            }
            EntityType::LwPolyline(poly) => {
                let kind = classify_linetype(&style.linetype);
                let preserve_bulge = tx.uniform_scale_rotation().is_some();
                let closed = poly.is_closed();
                let vertices = poly
                    .vertices
                    .iter()
                    .map(|v| PolylineVertex2D {
                        pos: tx.apply_point(Vec2::new(v.x, v.y)),
                        bulge: if preserve_bulge { v.bulge } else { 0.0 },
                    })
                    .collect();
                let id = self.next_id();
                self.entities.push(Entity2D {
                    id,
                    kind,
                    primitive: Primitive2D::Polyline(Polyline2D { vertices, closed }),
                    style,
                });
            }
            EntityType::Polyline(poly) => {
                let kind = classify_linetype(&style.linetype);
                let preserve_bulge = tx.uniform_scale_rotation().is_some();
                let vertices = poly
                    .vertices()
                    .map(|v| PolylineVertex2D {
                        pos: tx.apply_point(Vec2::new(v.location.x, v.location.y)),
                        bulge: if preserve_bulge { v.bulge } else { 0.0 },
                    })
                    .collect();
                let id = self.next_id();
                self.entities.push(Entity2D {
                    id,
                    kind,
                    primitive: Primitive2D::Polyline(Polyline2D {
                        vertices,
                        closed: false,
                    }),
                    style,
                });
            }
            EntityType::Spline(spline) => {
                self.import_spline(spline, style, tx);
            }
            EntityType::Ellipse(ellipse) => {
                self.import_ellipse(ellipse, style, tx);
            }
            EntityType::Text(t) => {
                let at = tx.apply_point(Vec2::new(t.location.x, t.location.y));
                let height = Some(scale_text_height(tx, t.text_height));
                let id = self.next_id();
                self.texts.push(TextEntity {
                    id,
                    text: t.value.clone(),
                    at,
                    height,
                    style,
                });
            }
            EntityType::MText(t) => {
                let at = tx.apply_point(Vec2::new(
                    t.insertion_point.x,
                    t.insertion_point.y,
                ));
                let height = Some(scale_text_height(tx, t.initial_text_height));
                let id = self.next_id();
                self.texts.push(TextEntity {
                    id,
                    text: join_mtext(t),
                    at,
                    height,
                    style,
                });
            }
            EntityType::RotatedDimension(d) => {
                let id = self.next_id();
                self.dims.push(DimensionEntity {
                    id,
                    raw_type: None,
                    text: empty_to_none(&d.dimension_base.text),
                    measurement: Some(d.dimension_base.actual_measurement),
                    style,
                });
            }
            EntityType::RadialDimension(d) => {
                let id = self.next_id();
                self.dims.push(DimensionEntity {
                    id,
                    raw_type: None,
                    text: empty_to_none(&d.dimension_base.text),
                    measurement: Some(d.dimension_base.actual_measurement),
                    style,
                });
            }
            EntityType::DiameterDimension(d) => {
                let id = self.next_id();
                self.dims.push(DimensionEntity {
                    id,
                    raw_type: None,
                    text: empty_to_none(&d.dimension_base.text),
                    measurement: Some(d.dimension_base.actual_measurement),
                    style,
                });
            }
            EntityType::AngularThreePointDimension(d) => {
                let id = self.next_id();
                self.dims.push(DimensionEntity {
                    id,
                    raw_type: None,
                    text: empty_to_none(&d.dimension_base.text),
                    measurement: Some(d.dimension_base.actual_measurement),
                    style,
                });
            }
            EntityType::OrdinateDimension(d) => {
                let id = self.next_id();
                self.dims.push(DimensionEntity {
                    id,
                    raw_type: None,
                    text: empty_to_none(&d.dimension_base.text),
                    measurement: Some(d.dimension_base.actual_measurement),
                    style,
                });
            }
            _ => {
                // Keep deterministic: ignore unsupported entities for now, but don't fail import.
            }
        }
    }

    fn import_insert(
        &mut self,
        insert: &dxf::entities::Insert,
        insert_style: &Style,
        parent_tx: &Transform2D,
        stack: &mut Vec<String>,
        depth: usize,
    ) {
        if depth > 8 {
            return;
        }
        let name = insert.name.to_ascii_lowercase();
        if stack.iter().any(|n| n == &name) {
            return;
        }
        let (base, entities) = match self.blocks.get(&name) {
            Some(block) => (
                Vec2::new(block.base_point.x, block.base_point.y),
                block.entities.clone(),
            ),
            None => return,
        };
        stack.push(name);
        let loc = Vec2::new(insert.location.x, insert.location.y);
        let scale = Vec2::new(insert.x_scale_factor, insert.y_scale_factor);
        let rot = insert.rotation;
        let col_count = insert.column_count.max(1) as i32;
        let row_count = insert.row_count.max(1) as i32;

        for row in 0..row_count {
            for col in 0..col_count {
                let offset = Vec2::new(
                    col as f64 * insert.column_spacing,
                    row as f64 * insert.row_spacing,
                );
                let local_tx = Transform2D::from_insert(base, loc, scale, rot, offset);
                let combined = parent_tx.compose(&local_tx);
                for ent in &entities {
                    self.import_entity(ent, &combined, Some(insert_style), stack, depth);
                }
            }
        }

        stack.pop();
    }

    fn import_spline(
        &mut self,
        spline: &dxf::entities::Spline,
        style: Style,
        tx: &Transform2D,
    ) {
        let points = if !spline.fit_points.is_empty() {
            spline.fit_points.iter().collect::<Vec<_>>()
        } else {
            spline.control_points.iter().collect::<Vec<_>>()
        };
        if points.len() < 2 {
            return;
        }
        let vertices = points
            .into_iter()
            .map(|p| PolylineVertex2D {
                pos: tx.apply_point(Vec2::new(p.x, p.y)),
                bulge: 0.0,
            })
            .collect();
        let id = self.next_id();
        self.entities.push(Entity2D {
            id,
            kind: classify_linetype(&style.linetype),
            primitive: Primitive2D::Polyline(Polyline2D {
                vertices,
                closed: false,
            }),
            style,
        });
    }

    fn import_ellipse(
        &mut self,
        ellipse: &dxf::entities::Ellipse,
        style: Style,
        tx: &Transform2D,
    ) {
        let center = Vec2::new(ellipse.center.x, ellipse.center.y);
        let major = Vec2::new(ellipse.major_axis.x, ellipse.major_axis.y);
        let major_len = (major.x * major.x + major.y * major.y).sqrt();
        if !major_len.is_finite() || major_len <= 0.0 {
            return;
        }
        let minor_len = major_len * ellipse.minor_axis_ratio;
        let minor_dir = norm(Vec2::new(-major.y, major.x));
        let minor = Vec2::new(minor_dir.x * minor_len, minor_dir.y * minor_len);

        let vertices = ellipse_points(
            center,
            major,
            minor,
            ellipse.start_parameter,
            ellipse.end_parameter,
            64,
        )
        .into_iter()
        .map(|p| PolylineVertex2D {
            pos: tx.apply_point(p),
            bulge: 0.0,
        })
        .collect::<Vec<_>>();

        if vertices.len() < 2 {
            return;
        }

        let closed = is_full_ellipse(ellipse.start_parameter, ellipse.end_parameter);
        let id = self.next_id();
        self.entities.push(Entity2D {
            id,
            kind: classify_linetype(&style.linetype),
            primitive: Primitive2D::Polyline(Polyline2D { vertices, closed }),
            style,
        });
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn resolve_style(&self, ent: &dxf::entities::Entity, parent_style: Option<&Style>) -> Style {
        let mut style = Style {
            layer: Some(ent.common.layer.clone()),
            linetype: Some(ent.common.line_type_name.clone()),
            color_index: ent.common.color.index().map(|v| v as i16),
        };
        if let Some(parent) = parent_style {
            if is_layer_zero(&style.layer) {
                style.layer = parent.layer.clone();
            }
            if is_byblock_linetype(&style.linetype) {
                style.linetype = parent.linetype.clone();
            }
            if ent.common.color.is_by_block() {
                style.color_index = parent.color_index;
            }
        }
        style
    }
}

#[derive(Debug, Clone, Copy)]
struct Transform2D {
    m11: f64,
    m12: f64,
    m21: f64,
    m22: f64,
    tx: f64,
    ty: f64,
}

impl Transform2D {
    fn identity() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m21: 0.0,
            m22: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn from_insert(base: Vec2, location: Vec2, scale: Vec2, rotation_deg: f64, offset: Vec2) -> Self {
        let r = rotation_deg.to_radians();
        let cos = r.cos();
        let sin = r.sin();
        let m11 = cos * scale.x;
        let m12 = -sin * scale.y;
        let m21 = sin * scale.x;
        let m22 = cos * scale.y;

        let off_x = m11 * offset.x + m12 * offset.y;
        let off_y = m21 * offset.x + m22 * offset.y;
        let tx = location.x + off_x - (m11 * base.x + m12 * base.y);
        let ty = location.y + off_y - (m21 * base.x + m22 * base.y);

        Self {
            m11,
            m12,
            m21,
            m22,
            tx,
            ty,
        }
    }

    fn compose(&self, other: &Transform2D) -> Self {
        Self {
            m11: self.m11 * other.m11 + self.m12 * other.m21,
            m12: self.m11 * other.m12 + self.m12 * other.m22,
            m21: self.m21 * other.m11 + self.m22 * other.m21,
            m22: self.m21 * other.m12 + self.m22 * other.m22,
            tx: self.m11 * other.tx + self.m12 * other.ty + self.tx,
            ty: self.m21 * other.tx + self.m22 * other.ty + self.ty,
        }
    }

    fn apply_point(&self, p: Vec2) -> Vec2 {
        Vec2::new(
            self.m11 * p.x + self.m12 * p.y + self.tx,
            self.m21 * p.x + self.m22 * p.y + self.ty,
        )
    }

    fn uniform_scale_rotation(&self) -> Option<(f64, f64)> {
        const EPS: f64 = 1e-6;
        let sx = (self.m11 * self.m11 + self.m21 * self.m21).sqrt();
        let sy = (self.m12 * self.m12 + self.m22 * self.m22).sqrt();
        if !sx.is_finite() || !sy.is_finite() {
            return None;
        }
        if (sx - sy).abs() > EPS {
            return None;
        }
        let dot = self.m11 * self.m12 + self.m21 * self.m22;
        if dot.abs() > EPS {
            return None;
        }
        let det = self.m11 * self.m22 - self.m12 * self.m21;
        if det < 0.0 {
            return None;
        }
        let rot = self.m21.atan2(self.m11);
        Some((sx, rot))
    }
}

fn scale_text_height(tx: &Transform2D, height: f64) -> f64 {
    if let Some((scale, _)) = tx.uniform_scale_rotation() {
        height * scale
    } else {
        height
    }
}

fn is_layer_zero(layer: &Option<String>) -> bool {
    matches!(layer.as_deref(), Some(v) if v.eq_ignore_ascii_case("0"))
}

fn is_byblock_linetype(linetype: &Option<String>) -> bool {
    matches!(linetype.as_deref(), Some(v) if v.eq_ignore_ascii_case("BYBLOCK"))
}

fn classify_linetype(linetype: &Option<String>) -> EntityKind {
    let lt = match linetype {
        Some(v) => v.to_ascii_lowercase(),
        None => return EntityKind::Unknown,
    };
    if lt.contains("center") || lt.contains("centre") {
        return EntityKind::Center;
    }
    if lt.contains("hidden") || lt.contains("hid") {
        return EntityKind::Hidden;
    }
    EntityKind::Object
}

fn circle_points(center: Vec2, radius: f64, segments: usize) -> Vec<Vec2> {
    if !radius.is_finite() || radius <= 0.0 || segments < 3 {
        return Vec::new();
    }
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = t * std::f64::consts::TAU;
        pts.push(Vec2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    pts
}

fn arc_points(
    center: Vec2,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    segments: usize,
) -> Vec<Vec2> {
    if !radius.is_finite() || radius <= 0.0 || segments < 2 {
        return Vec::new();
    }
    let a0 = start_deg.to_radians();
    let mut a1 = end_deg.to_radians();
    if a1 < a0 {
        a1 += std::f64::consts::TAU;
    }
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = a0 + (a1 - a0) * t;
        pts.push(Vec2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    pts
}

fn ellipse_points(
    center: Vec2,
    major: Vec2,
    minor: Vec2,
    start: f64,
    end: f64,
    segments: usize,
) -> Vec<Vec2> {
    if segments < 2 || !start.is_finite() || !end.is_finite() {
        return Vec::new();
    }
    let a0 = start;
    let mut a1 = end;
    if a1 < a0 {
        a1 += std::f64::consts::TAU;
    }
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = a0 + (a1 - a0) * t;
        pts.push(Vec2::new(
            center.x + major.x * a.cos() + minor.x * a.sin(),
            center.y + major.y * a.cos() + minor.y * a.sin(),
        ));
    }
    pts
}

fn is_full_ellipse(start: f64, end: f64) -> bool {
    if !start.is_finite() || !end.is_finite() {
        return false;
    }
    let mut delta = (end - start).abs();
    if delta > std::f64::consts::TAU {
        delta = delta % std::f64::consts::TAU;
    }
    (delta - std::f64::consts::TAU).abs() < 1e-6 || delta < 1e-6
}

fn norm(v: Vec2) -> Vec2 {
    let len = (v.x * v.x + v.y * v.y).sqrt();
    if !len.is_finite() || len < 1e-12 {
        return Vec2::new(0.0, 0.0);
    }
    Vec2::new(v.x / len, v.y / len)
}

fn join_mtext(t: &dxf::entities::MText) -> String {
    if t.extended_text.is_empty() {
        return t.text.clone();
    }
    let mut s = String::new();
    s.push_str(&t.text);
    for part in &t.extended_text {
        s.push_str(part);
    }
    s
}

fn empty_to_none(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}
