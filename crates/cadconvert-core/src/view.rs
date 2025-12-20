use crate::geom::Vec2;
use crate::report::ViewClusterReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionScheme {
    ThirdAngle,
    FirstAngle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewRole {
    Front,
    Top,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewRoleAssignment {
    pub cluster_id: usize,
    pub role: ViewRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewAssignmentReport {
    pub scheme: ProjectionScheme,
    pub confidence: f64,
    pub roles: Vec<ViewRoleAssignment>,
}

pub fn assign_three_view_roles(clusters: &[ViewClusterReport]) -> Option<ViewAssignmentReport> {
    if clusters.len() != 3 {
        return None;
    }

    // Brute-force the 3! permutations deterministically.
    let perms = [
        [0usize, 1usize, 2usize],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];

    let mut best: Option<(f64, ProjectionScheme, [usize; 3])> = None; // score, scheme, [front, top, right] indices

    for perm in perms {
        let f = &clusters[perm[0]];
        let t = &clusters[perm[1]];
        let r = &clusters[perm[2]];
        for scheme in [ProjectionScheme::ThirdAngle, ProjectionScheme::FirstAngle] {
            let score = score_assignment(scheme, f.bbox.center(), t.bbox.center(), r.bbox.center());
            match best {
                Some((best_score, _, _)) if score <= best_score => {}
                _ => best = Some((score, scheme, [perm[0], perm[1], perm[2]])),
            }
        }
    }

    let (best_score, scheme, best_perm) = best?;
    if best_score < 2.0 {
        // Too weak to trust.
        return None;
    }

    // Map back to cluster ids.
    let front_id = clusters[best_perm[0]].id;
    let top_id = clusters[best_perm[1]].id;
    let right_id = clusters[best_perm[2]].id;

    // Confidence is heuristic; keep it in [0,1].
    let confidence = (best_score / 4.0).clamp(0.0, 1.0);

    Some(ViewAssignmentReport {
        scheme,
        confidence,
        roles: vec![
            ViewRoleAssignment {
                cluster_id: front_id,
                role: ViewRole::Front,
            },
            ViewRoleAssignment {
                cluster_id: top_id,
                role: ViewRole::Top,
            },
            ViewRoleAssignment {
                cluster_id: right_id,
                role: ViewRole::Right,
            },
        ],
    })
}

fn score_assignment(scheme: ProjectionScheme, front: Vec2, top: Vec2, right: Vec2) -> f64 {
    let mut score = 0.0;

    // Relative position rules.
    match scheme {
        ProjectionScheme::ThirdAngle => {
            if top.y > front.y {
                score += 1.0;
            } else {
                score -= 1.0;
            }
            if right.x > front.x {
                score += 1.0;
            } else {
                score -= 1.0;
            }
        }
        ProjectionScheme::FirstAngle => {
            if top.y < front.y {
                score += 1.0;
            } else {
                score -= 1.0;
            }
            if right.x < front.x {
                score += 1.0;
            } else {
                score -= 1.0;
            }
        }
    }

    // Alignment rules (soft): top roughly above/below front; right roughly left/right of front.
    let dx_tf = (top.x - front.x).abs();
    let dy_rf = (right.y - front.y).abs();
    let span_x = dx_tf + (right.x - front.x).abs() + 1e-9;
    let span_y = dy_rf + (top.y - front.y).abs() + 1e-9;

    if dx_tf / span_x < 0.35 {
        score += 1.0;
    }
    if dy_rf / span_y < 0.35 {
        score += 1.0;
    }

    score
}
