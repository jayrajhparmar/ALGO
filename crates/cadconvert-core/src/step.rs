use crate::geom::Vec2;
use crate::model::{Drawing2D, EntityKind, Primitive2D, Units};
use std::fmt::Write as _;

pub fn wireframe_step(drawing: &Drawing2D, name: &str) -> String {
    let safe_name = if name.trim().is_empty() {
        "cadconvert"
    } else {
        name.trim()
    };
    let safe_name = escape_step_string(safe_name);

    let mut writer = StepWriter::new();

    // Product + context boilerplate (AP214-ish; widely supported for simple wireframes).
    let app_ctx = writer.push(format!(
        "APPLICATION_CONTEXT('core data for automotive mechanical design processes')"
    ));
    writer.push(format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app_ctx})"
    ));
    let prod_def_ctx = writer.push(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{app_ctx},'design')"
    ));
    let prod_ctx = writer.push(format!("PRODUCT_CONTEXT('',#{app_ctx},'mechanical')"));

    let product = writer.push(format!(
        "PRODUCT('{safe_name}','{safe_name}','',(#{prod_ctx}))"
    ));
    let prod_def_form = writer.push(format!(
        "PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE('','',#{product},.MADE.)"
    ));
    let prod_def = writer.push(format!(
        "PRODUCT_DEFINITION('','',#{prod_def_form},#{prod_def_ctx})"
    ));
    let prod_def_shape = writer.push(format!("PRODUCT_DEFINITION_SHAPE('','',#{prod_def})"));

    let (len_unit, plane_unit, solid_unit) = units(writer.next_id(), drawing.units);
    let len_unit = writer.push(len_unit);
    let plane_unit = writer.push(plane_unit);
    let solid_unit = writer.push(solid_unit);
    let uncertainty = writer.push(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-6),#{len_unit},'distance_accuracy_value','')"
    ));
    let rep_ctx = writer.push(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty}))GLOBAL_UNIT_ASSIGNED_CONTEXT((#{len_unit},#{plane_unit},#{solid_unit}))REPRESENTATION_CONTEXT('',''))"
    ));

    // Geometry
    let mut curve_ids = Vec::new();
    for ent in &drawing.entities {
        match ent.kind {
            EntityKind::Dimension | EntityKind::Text | EntityKind::Hatch => continue,
            _ => {}
        }

        let points = primitive_to_polyline_points(&ent.primitive);
        if points.len() < 2 {
            continue;
        }
        if points.iter().all(|p| approx_eq(points[0], *p)) {
            continue;
        }

        let mut point_ids = Vec::with_capacity(points.len());
        for p in points {
            point_ids.push(writer.push(cartesian_point(p.x, p.y, 0.0)));
        }
        curve_ids.push(writer.push(polyline(&point_ids)));
    }

    // Representation
    let curve_set = writer.push(geometric_curve_set(&curve_ids));
    let shape_rep = writer.push(format!(
        "SHAPE_REPRESENTATION('wireframe',(#{curve_set}),#{rep_ctx})"
    ));
    writer.push(format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{prod_def_shape},#{shape_rep})"
    ));

    // File wrapper
    let mut out = String::new();
    let _ = writeln!(out, "ISO-10303-21;");
    let _ = writeln!(out, "HEADER;");
    let _ = writeln!(out, "FILE_DESCRIPTION(('cadconvert wireframe'),'2;1');");
    let _ = writeln!(
        out,
        "FILE_NAME('{safe_name}.step','1970-01-01T00:00:00',('cadconvert'),(''),'cadconvert','cadconvert','');"
    );
    let _ = writeln!(out, "FILE_SCHEMA(('AUTOMOTIVE_DESIGN_CC2'));");
    let _ = writeln!(out, "ENDSEC;");
    let _ = writeln!(out, "DATA;");
    for line in writer.lines {
        let _ = writeln!(out, "{line}");
    }
    let _ = writeln!(out, "ENDSEC;");
    let _ = writeln!(out, "END-ISO-10303-21;");
    out
}

struct StepWriter {
    next_id: u32,
    lines: Vec<String>,
}

impl StepWriter {
    fn new() -> Self {
        Self {
            next_id: 1,
            lines: Vec::new(),
        }
    }

    fn next_id(&self) -> u32 {
        self.next_id
    }

    fn push(&mut self, entity: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.lines.push(format!("#{id}={entity};"));
        id
    }
}

fn escape_step_string(s: &str) -> String {
    s.replace('\'', "''")
}

fn f64_step(v: f64) -> String {
    if !v.is_finite() {
        return "0.".to_string();
    }
    // Keep it readable but deterministic.
    let mut s = format!("{v:.6}");
    if s == "-0.000000" {
        s = "0.000000".to_string();
    }
    s
}

fn cartesian_point(x: f64, y: f64, z: f64) -> String {
    format!(
        "CARTESIAN_POINT('',({},{},{}))",
        f64_step(x),
        f64_step(y),
        f64_step(z)
    )
}

fn polyline(point_ids: &[u32]) -> String {
    let mut ids = String::new();
    for (i, id) in point_ids.iter().enumerate() {
        if i > 0 {
            ids.push(',');
        }
        let _ = write!(ids, "#{id}");
    }
    format!("POLYLINE('',({ids}))")
}

fn geometric_curve_set(curve_ids: &[u32]) -> String {
    let mut ids = String::new();
    for (i, id) in curve_ids.iter().enumerate() {
        if i > 0 {
            ids.push(',');
        }
        let _ = write!(ids, "#{id}");
    }
    format!("GEOMETRIC_CURVE_SET('',({ids}))")
}

fn units(next_id_hint: u32, units: Units) -> (String, String, String) {
    // Note: `next_id_hint` is unused today but kept to make it easy to debug ID ordering later.
    let _ = next_id_hint;
    let len = match units {
        Units::Meters => "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT($,.METRE.))",
        Units::Centimeters => "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.CENTI.,.METRE.))",
        Units::Millimeters | Units::Unknown | Units::Inches => {
            "(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))"
        }
    };
    (
        len.to_string(),
        "(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))".to_string(),
        "(NAMED_UNIT(*)SOLID_ANGLE_UNIT()SI_UNIT($,.STERADIAN.))".to_string(),
    )
}

fn primitive_to_polyline_points(prim: &Primitive2D) -> Vec<Vec2> {
    match prim {
        Primitive2D::Line(l) => vec![l.a, l.b],
        Primitive2D::Circle(c) => circle_points(c.center, c.radius, 64),
        Primitive2D::Arc(a) => {
            arc_points(a.center, a.radius, a.start_angle_deg, a.end_angle_deg, 48)
        }
        Primitive2D::Polyline(pl) => polyline_points(pl),
        Primitive2D::CubicBezier(b) => bezier_points(b, 32),
    }
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

fn bezier_points(b: &crate::model::Bezier2D, segments: usize) -> Vec<Vec2> {
    if segments < 2 {
        return Vec::new();
    }
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        pts.push(bezier_eval(b, t));
    }
    pts
}

fn bezier_eval(b: &crate::model::Bezier2D, t: f64) -> Vec2 {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * b.p0.x + 3.0 * uu * t * b.p1.x + 3.0 * u * tt * b.p2.x + ttt * b.p3.x;
    let y = uuu * b.p0.y + 3.0 * uu * t * b.p1.y + 3.0 * u * tt * b.p2.y + ttt * b.p3.y;
    Vec2::new(x, y)
}

fn polyline_points(pl: &crate::model::Polyline2D) -> Vec<Vec2> {
    let n = pl.vertices.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![pl.vertices[0].pos];
    }

    let mut out = Vec::new();
    out.push(pl.vertices[0].pos);

    for i in 0..(n - 1) {
        let p0 = pl.vertices[i].pos;
        let p1 = pl.vertices[i + 1].pos;
        let bulge = pl.vertices[i].bulge;
        append_segment(&mut out, p0, p1, bulge);
    }

    if pl.closed {
        let p0 = pl.vertices[n - 1].pos;
        let p1 = pl.vertices[0].pos;
        let bulge = pl.vertices[n - 1].bulge;
        append_segment(&mut out, p0, p1, bulge);
    }

    out
}

fn append_segment(out: &mut Vec<Vec2>, p0: Vec2, p1: Vec2, bulge: f64) {
    if approx_eq(p0, p1) {
        return;
    }
    if bulge.abs() < 1e-10 {
        out.push(p1);
        return;
    }

    let Some(arc) = bulge_arc_points(p0, p1, bulge) else {
        out.push(p1);
        return;
    };
    // Skip the first point (it duplicates the current end).
    out.extend(arc.into_iter().skip(1));
}

fn bulge_arc_points(p0: Vec2, p1: Vec2, bulge: f64) -> Option<Vec<Vec2>> {
    let chord = Vec2::new(p1.x - p0.x, p1.y - p0.y);
    let c = (chord.x * chord.x + chord.y * chord.y).sqrt();
    if !c.is_finite() || c < 1e-12 {
        return None;
    }
    if !bulge.is_finite() || bulge.abs() < 1e-12 {
        return Some(vec![p0, p1]);
    }

    let theta = 4.0 * bulge.atan(); // signed sweep angle
    let r = c * (1.0 + bulge * bulge) / (4.0 * bulge.abs());

    let mid = Vec2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
    let perp = norm(rot90(chord));
    let d = r * (theta * 0.5).cos() * bulge.signum();
    let center = Vec2::new(mid.x + perp.x * d, mid.y + perp.y * d);

    let a0 = (p0.y - center.y).atan2(p0.x - center.x);
    let segments = ((theta.abs() / (std::f64::consts::PI / 16.0)).ceil() as usize).clamp(2, 256);

    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = a0 + theta * t;
        pts.push(Vec2::new(center.x + r * a.cos(), center.y + r * a.sin()));
    }
    Some(pts)
}

fn rot90(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

fn norm(v: Vec2) -> Vec2 {
    let len = (v.x * v.x + v.y * v.y).sqrt();
    if !len.is_finite() || len < 1e-12 {
        return Vec2::new(0.0, 0.0);
    }
    Vec2::new(v.x / len, v.y / len)
}

fn approx_eq(a: Vec2, b: Vec2) -> bool {
    (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
}
