use cadconvert_core::geom::Vec2;
use cadconvert_core::model::{Drawing2D, Entity2D, EntityKind, LineSeg2D, Primitive2D, Style, Units};
use cadconvert_core::normalize::{normalize_in_place, NormalizeConfig};

#[test]
fn drops_degenerate_entities_and_inferrs_kind() {
    let mut drawing = Drawing2D {
        units: Units::Unknown,
        entities: vec![
            Entity2D {
                id: 1,
                kind: EntityKind::Unknown,
                primitive: Primitive2D::Line(LineSeg2D {
                    a: Vec2::new(0.0, 0.0),
                    b: Vec2::new(0.0, 0.0),
                }),
                style: Style {
                    layer: None,
                    linetype: Some("HIDDEN".to_string()),
                    color_index: None,
                },
            },
            Entity2D {
                id: 2,
                kind: EntityKind::Unknown,
                primitive: Primitive2D::Line(LineSeg2D {
                    a: Vec2::new(0.0, 0.0),
                    b: Vec2::new(1.0, 0.0),
                }),
                style: Style {
                    layer: None,
                    linetype: Some("HIDDEN".to_string()),
                    color_index: None,
                },
            },
        ],
        dims: Vec::new(),
        texts: Vec::new(),
    };

    let stats = normalize_in_place(&mut drawing, &NormalizeConfig::default());
    assert_eq!(1, drawing.entities.len());
    assert_eq!(1, stats.removed_degenerate_entities);
    assert_eq!(1, stats.inferred_kinds);
    assert_eq!(EntityKind::Hidden, drawing.entities[0].kind);
}

