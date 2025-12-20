use crate::geom::{BBox2, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Units {
    Unknown,
    Inches,
    Millimeters,
    Centimeters,
    Meters,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Unknown,
    Object,
    Hidden,
    Center,
    Dimension,
    Text,
    Hatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Style {
    pub layer: Option<String>,
    pub linetype: Option<String>,
    pub color_index: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineSeg2D {
    pub a: Vec2,
    pub b: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Circle2D {
    pub center: Vec2,
    pub radius: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arc2D {
    pub center: Vec2,
    pub radius: f64,
    pub start_angle_deg: f64,
    pub end_angle_deg: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolylineVertex2D {
    pub pos: Vec2,
    pub bulge: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline2D {
    pub vertices: Vec<PolylineVertex2D>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bezier2D {
    pub p0: Vec2,
    pub p1: Vec2,
    pub p2: Vec2,
    pub p3: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Primitive2D {
    Line(LineSeg2D),
    Circle(Circle2D),
    Arc(Arc2D),
    Polyline(Polyline2D),
    CubicBezier(Bezier2D),
}

impl Primitive2D {
    pub fn bbox(&self) -> BBox2 {
        match self {
            Primitive2D::Line(line) => {
                let mut bbox = BBox2::empty();
                bbox.include_point(line.a);
                bbox.include_point(line.b);
                bbox
            }
            Primitive2D::Circle(circle) => BBox2::new(
                Vec2::new(circle.center.x - circle.radius, circle.center.y - circle.radius),
                Vec2::new(circle.center.x + circle.radius, circle.center.y + circle.radius),
            ),
            Primitive2D::Arc(arc) => {
                let mut bbox = BBox2::empty();
                let r = arc.radius;
                bbox.include_point(Vec2::new(arc.center.x - r, arc.center.y - r));
                bbox.include_point(Vec2::new(arc.center.x + r, arc.center.y + r));
                bbox
            }
            Primitive2D::Polyline(poly) => {
                let mut bbox = BBox2::empty();
                for v in &poly.vertices {
                    bbox.include_point(v.pos);
                }
                bbox
            }
            Primitive2D::CubicBezier(b) => {
                let mut bbox = BBox2::empty();
                bbox.include_point(b.p0);
                bbox.include_point(b.p1);
                bbox.include_point(b.p2);
                bbox.include_point(b.p3);
                bbox
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity2D {
    pub id: u64,
    pub kind: EntityKind,
    pub primitive: Primitive2D,
    pub style: Style,
}

impl Entity2D {
    pub fn bbox(&self) -> BBox2 {
        self.primitive.bbox()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextEntity {
    pub id: u64,
    pub text: String,
    pub at: Vec2,
    pub height: Option<f64>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionEntity {
    pub id: u64,
    pub raw_type: Option<i16>,
    pub text: Option<String>,
    pub measurement: Option<f64>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawing2D {
    pub units: Units,
    pub entities: Vec<Entity2D>,
    pub dims: Vec<DimensionEntity>,
    pub texts: Vec<TextEntity>,
}

impl Drawing2D {
    pub fn extents(&self) -> Option<BBox2> {
        let mut bbox = BBox2::empty();
        let mut any = false;
        for e in &self.entities {
            bbox = bbox.union(&e.bbox());
            any = true;
        }
        if any { Some(bbox) } else { None }
    }
}

