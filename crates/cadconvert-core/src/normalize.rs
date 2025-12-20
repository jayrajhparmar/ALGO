use crate::model::{Drawing2D, EntityKind, Primitive2D, Style};

#[derive(Debug, Clone)]
pub struct NormalizeConfig {
    pub min_entity_length: f64,
    pub infer_kinds_from_style: bool,
    pub drop_degenerate_entities: bool,
}

impl Default for NormalizeConfig {
    fn default() -> Self {
        Self {
            min_entity_length: 1e-6,
            infer_kinds_from_style: true,
            drop_degenerate_entities: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NormalizeStats {
    pub removed_degenerate_entities: usize,
    pub inferred_kinds: usize,
}

pub fn normalize_in_place(drawing: &mut Drawing2D, cfg: &NormalizeConfig) -> NormalizeStats {
    let mut stats = NormalizeStats::default();

    if cfg.infer_kinds_from_style {
        for ent in &mut drawing.entities {
            if ent.kind == EntityKind::Unknown {
                ent.kind = infer_kind_from_style(&ent.style);
                if ent.kind != EntityKind::Unknown {
                    stats.inferred_kinds += 1;
                }
            }
        }
    }

    if cfg.drop_degenerate_entities {
        let min_len2 = cfg.min_entity_length * cfg.min_entity_length;
        let before = drawing.entities.len();
        drawing.entities.retain(|e| !is_degenerate(&e.primitive, min_len2));
        stats.removed_degenerate_entities = before.saturating_sub(drawing.entities.len());
    }

    stats
}

fn infer_kind_from_style(style: &Style) -> EntityKind {
    let mut s = String::new();
    if let Some(layer) = &style.layer {
        s.push_str(layer);
        s.push(' ');
    }
    if let Some(lt) = &style.linetype {
        s.push_str(lt);
    }
    let s = s.to_ascii_lowercase();

    if s.contains("center") || s.contains("centre") {
        return EntityKind::Center;
    }
    if s.contains("hidden") || s.contains("hid") {
        return EntityKind::Hidden;
    }
    if s.contains("object") || s.contains("cont") {
        return EntityKind::Object;
    }

    EntityKind::Unknown
}

fn is_degenerate(p: &Primitive2D, min_len2: f64) -> bool {
    match p {
        Primitive2D::Line(l) => {
            let dx = l.a.x - l.b.x;
            let dy = l.a.y - l.b.y;
            (dx * dx + dy * dy) <= min_len2
        }
        Primitive2D::Circle(c) => c.radius * c.radius <= min_len2,
        Primitive2D::Arc(a) => a.radius * a.radius <= min_len2,
        Primitive2D::Polyline(pl) => pl.vertices.len() < 2,
        Primitive2D::CubicBezier(b) => {
            let mut max_d2 = 0.0f64;
            for (p0, p1) in [
                (b.p0, b.p1),
                (b.p0, b.p2),
                (b.p0, b.p3),
                (b.p1, b.p2),
                (b.p1, b.p3),
                (b.p2, b.p3),
            ] {
                let dx = p0.x - p1.x;
                let dy = p0.y - p1.y;
                max_d2 = max_d2.max(dx * dx + dy * dy);
            }
            max_d2 <= min_len2
        }
    }
}

