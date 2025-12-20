use cadconvert_core::view::{assign_three_view_roles, ProjectionScheme, ViewRole};
use cadconvert_core::{geom::BBox2, geom::Vec2, report::ViewClusterReport};

#[test]
fn assigns_third_angle_three_view_layout() {
    let clusters = vec![
        ViewClusterReport {
            id: 0,
            entity_count: 10,
            bbox: BBox2::new(Vec2::new(0.0, 100.0), Vec2::new(100.0, 200.0)), // top
            entity_id_sample: Vec::new(),
        },
        ViewClusterReport {
            id: 1,
            entity_count: 10,
            bbox: BBox2::new(Vec2::new(0.0, 0.0), Vec2::new(100.0, 100.0)), // front
            entity_id_sample: Vec::new(),
        },
        ViewClusterReport {
            id: 2,
            entity_count: 10,
            bbox: BBox2::new(Vec2::new(100.0, 0.0), Vec2::new(200.0, 100.0)), // right
            entity_id_sample: Vec::new(),
        },
    ];

    let assignment = assign_three_view_roles(&clusters).expect("expected an assignment");
    assert_eq!(ProjectionScheme::ThirdAngle, assignment.scheme);
    assert!(assignment.confidence > 0.9);

    let mut front = None;
    let mut top = None;
    let mut right = None;
    for r in assignment.roles {
        match r.role {
            ViewRole::Front => front = Some(r.cluster_id),
            ViewRole::Top => top = Some(r.cluster_id),
            ViewRole::Right => right = Some(r.cluster_id),
        }
    }

    assert_eq!(Some(1), front);
    assert_eq!(Some(0), top);
    assert_eq!(Some(2), right);
}
