use anyhow::Result;
use crate::structs::{View2D, Vertex2D, Edge2D};
use cadconvert_core::model::{Primitive2D, Entity2D};
use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet};

const EPSILON: f64 = 1e-4;

#[derive(Clone, Debug)]
struct RawSegment {
    p1: Point2<f64>,
    p2: Point2<f64>,
    original_id: u64,
}

pub fn build_topology(view: &mut View2D) -> Result<()> {
    let mut segments = extract_segments(&view.raw_entities);

    // 1. Intersect segments (Naively O(N^2))
    let mut split_points_map: HashMap<usize, Vec<Point2<f64>>> = HashMap::new();
    
    for i in 0..segments.len() {
        for j in (i + 1)..segments.len() {
            if let Some(pt) = intersect_segment_segment(&segments[i], &segments[j]) {
                split_points_map.entry(i).or_default().push(pt);
                split_points_map.entry(j).or_default().push(pt);
            }
        }
    }

    // 2. Split segments
    let mut final_segments = Vec::new();

    for (i, seg) in segments.iter().enumerate() {
        if let Some(points) = split_points_map.get(&i) {
            let mut pts = points.clone();
            pts.push(seg.p1);
            pts.push(seg.p2);
            
            // Sort points along the segment vector
            let dir = seg.p2 - seg.p1;
            
            pts.sort_by(|a, b| {
                let da = (*a - seg.p1).norm();
                let db = (*b - seg.p1).norm();
                da.partial_cmp(&db).unwrap()
            });

            // Deduplicate points
            pts.dedup_by(|a, b| (*a - *b).norm() < EPSILON);

            // Create sub-segments
            for window in pts.windows(2) {
                let p_start = window[0];
                let p_end = window[1];
                if (p_start - p_end).norm() > EPSILON {
                    final_segments.push(RawSegment {
                        p1: p_start,
                        p2: p_end,
                        original_id: seg.original_id,
                    });
                }
            }

        } else {
            final_segments.push(seg.clone());
        }
    }
    
    // 3. Snap vertices and build graph
    let mut unique_points: Vec<Point2<f64>> = Vec::new();
    // Helper to find or add point
    let mut get_point_id = |p: Point2<f64>, points: &mut Vec<Point2<f64>>| -> usize {
        for (idx, existing) in points.iter().enumerate() {
            if (p - *existing).norm() < EPSILON {
                return idx;
            }
        }
        points.push(p);
        points.len() - 1
    };

    let mut edges = Vec::new();
    
    for (seg_idx, seg) in final_segments.iter().enumerate() {
        let id1 = get_point_id(seg.p1, &mut unique_points);
        let id2 = get_point_id(seg.p2, &mut unique_points);
        
        if id1 != id2 {
            edges.push(Edge2D {
                id: seg_idx, // This ID is temporary, will refine
                start: id1,
                end: id2,
                original_entity_id: Some(seg.original_id),
            });
        }
    }

    // Populate View
    view.vertices = unique_points
        .into_iter()
        .enumerate()
        .map(|(id, point)| Vertex2D { id, point })
        .collect();

    // Re-index edges
    for (i, edge) in edges.iter_mut().enumerate() {
        edge.id = i;
    }
    view.edges = edges;

    Ok(())
}

fn extract_segments(entities: &[Entity2D]) -> Vec<RawSegment> {
    let mut segs = Vec::new();
    for ent in entities {
        match &ent.primitive {
            Primitive2D::Line(line) => {
                segs.push(RawSegment {
                    p1: Point2::new(line.a.x, line.a.y),
                    p2: Point2::new(line.b.x, line.b.y),
                    original_id: ent.id,
                });
            }
            Primitive2D::Polyline(poly) => {
                for i in 0..poly.vertices.len() {
                    let v1 = poly.vertices[i].clone();
                    let v2 = if i + 1 < poly.vertices.len() {
                        poly.vertices[i+1].clone()
                    } else if poly.closed {
                        poly.vertices[0].clone()
                    } else {
                        continue;
                    };
                    segs.push(RawSegment {
                        p1: Point2::new(v1.pos.x, v1.pos.y),
                        p2: Point2::new(v2.pos.x, v2.pos.y),
                        original_id: ent.id,
                    });
                }
            }
            _ => {} // Ignore non-polygonal
        }
    }
    segs
}

fn intersect_segment_segment(s1: &RawSegment, s2: &RawSegment) -> Option<Point2<f64>> {
    let p = s1.p1;
    let r = s1.p2 - s1.p1;
    let q = s2.p1;
    let s = s2.p2 - s2.p1;

    let r_cross_s = perp_dot(r, s);
    let q_minus_p = q - p;

    if r_cross_s.abs() < EPSILON {
        return None; 
    }

    let t = perp_dot(q_minus_p, s) / r_cross_s;
    let u = perp_dot(q_minus_p, r) / r_cross_s;

    if t >= -EPSILON && t <= 1.0 + EPSILON && u >= -EPSILON && u <= 1.0 + EPSILON {
        return Some(p + r * t);
    }

    None
}

fn perp_dot(v1: Vector2<f64>, v2: Vector2<f64>) -> f64 {
    v1.x * v2.y - v1.y * v2.x
}
