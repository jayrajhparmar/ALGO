use cadconvert_core::geom::Vec2;
use cadconvert_core::model::{
    Circle2D, Drawing2D, Entity2D, EntityKind, LineSeg2D, Primitive2D, Style, Units,
};

#[test]
fn writes_basic_step_wireframe() {
    let drawing = Drawing2D {
        units: Units::Millimeters,
        entities: vec![
            Entity2D {
                id: 1,
                kind: EntityKind::Object,
                primitive: Primitive2D::Line(LineSeg2D {
                    a: Vec2::new(0.0, 0.0),
                    b: Vec2::new(10.0, 0.0),
                }),
                style: Style {
                    layer: None,
                    linetype: None,
                    color_index: None,
                },
            },
            Entity2D {
                id: 2,
                kind: EntityKind::Object,
                primitive: Primitive2D::Circle(Circle2D {
                    center: Vec2::new(5.0, 5.0),
                    radius: 2.5,
                }),
                style: Style {
                    layer: None,
                    linetype: None,
                    color_index: None,
                },
            },
        ],
        dims: Vec::new(),
        texts: Vec::new(),
    };

    let step = cadconvert_core::step::wireframe_step(&drawing, "part");
    assert!(step.contains("ISO-10303-21;"));
    assert!(step.contains("FILE_SCHEMA(('AUTOMOTIVE_DESIGN_CC2'));"));
    assert!(step.contains("GEOMETRIC_CURVE_SET"));
    assert!(step.contains("POLYLINE"));
    assert!(step.contains("CARTESIAN_POINT"));
}
