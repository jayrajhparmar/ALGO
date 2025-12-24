use crate::structs::{LambdaRow, ThetaEdge, Vertex2D, View2D};
use anyhow::Result;
use nalgebra::{Point2, Point3, Vector2};
use std::cmp::Ordering;
use std::collections::HashSet;

const EPSILON: f64 = 1e-4;
const MATCH_TOLERANCE: f64 = 1.0;

pub fn build_reconstruction(
    v_xy: &View2D,
    v_xz: &View2D,
    v_yz: &View2D,
) -> Result<(Vec<LambdaRow>, HashSet<ThetaEdge>)> {
    // 1. Align Views (Centroid heuristic)
    let center_xy = get_centroid(v_xy);
    let center_xz = get_centroid(v_xz);
    let center_yz = get_centroid(v_yz);

    // Calculate offsets based on centroid alignment
    // Shift Top (XY) so its X aligns with Front (XZ)
    let offset_xy_x = center_xz.x - center_xy.x;

    // Shift Side (YZ) so its Z (y) aligns with Front (XZ) Z (y)
    let offset_yz_z = center_xz.y - center_yz.y;

    // Shift Side (YZ) so its Y (x) aligns with Top (XY) Y (y)
    // Note: We use the ORIGINAL Top Y for this, as we only shifted Top's X.
    let offset_yz_y = center_xy.y - center_yz.x;

    let shift_xy = Vector2::new(offset_xy_x, 0.0);
    // YZ.x -> Y, YZ.y -> Z
    let shift_yz = Vector2::new(offset_yz_y, offset_yz_z);

    println!(
        "Auto-Aligning Views: Shift Top X by {:.2}, Shift Side Z by {:.2}, Shift Side Y by {:.2}",
        offset_xy_x, offset_yz_z, offset_yz_y
    );

    // 2. Build Lambda (Candidate 3D Vertices) - Optimized with sorting
    let lambda = build_lambda_optimized(v_xy, v_xz, v_yz, shift_xy, shift_yz);
    println!("Built {} Lambda candidates.", lambda.len());

    // 3. Build Theta (Candidate 3D Edges) - Optimized with hashing
    let theta = build_theta_optimized(&lambda, v_xy, v_xz, v_yz);
    println!("Built {} Theta edges.", theta.len());

    Ok((lambda, theta))
}

fn get_centroid(view: &View2D) -> Point2<f64> {
    if view.vertices.is_empty() {
        return Point2::origin();
    }
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for v in &view.vertices {
        sum_x += v.point.x;
        sum_y += v.point.y;
    }
    let n = view.vertices.len() as f64;
    Point2::new(sum_x / n, sum_y / n)
}

fn build_lambda_optimized(
    v_xy: &View2D,
    v_xz: &View2D,
    v_yz: &View2D,
    shift_xy: Vector2<f64>,
    shift_yz: Vector2<f64>,
) -> Vec<LambdaRow> {
    let mut lambda = Vec::new();

    // V_xz: Sort by X
    let mut v_xz_sorted: Vec<&Vertex2D> = v_xz
        .vertices
        .iter()
        .filter(|v| v.point.x.is_finite() && v.point.y.is_finite())
        .collect();
    v_xz_sorted.sort_by(|a, b| a.point.x.partial_cmp(&b.point.x).unwrap());

    // V_yz: Sort by X (which maps to global Y)
    let mut v_yz_sorted: Vec<&Vertex2D> = v_yz
        .vertices
        .iter()
        .filter(|v| v.point.x.is_finite() && v.point.y.is_finite())
        .collect();
    v_yz_sorted.sort_by(|a, b| a.point.x.partial_cmp(&b.point.x).unwrap());

    // Iterate Top View (XY)
    for v1 in &v_xy.vertices {
        let p_xy = v1.point + shift_xy;

        // 1. Find candidates in XZ matching X
        // Range [target - tol, target + tol]
        let target_x = p_xy.x;
        // The raw V_xz X matches Global X directly.

        let start_idx = v_xz_sorted.partition_point(|v| v.point.x < target_x - MATCH_TOLERANCE);
        // We iterate from start_idx until value > target + tol

        for i in start_idx..v_xz_sorted.len() {
            let v2 = v_xz_sorted[i];
            if v2.point.x > target_x + MATCH_TOLERANCE {
                break;
            }
            // Candidate v2 found (matches X)
            let p_xz = v2.point; // Global (x, z)

            // 2. Find candidates in YZ matching Y (from XY)
            // V_yz.x (plus shift) should match p_xy.y
            // So raw V_yz.x should match p_xy.y - shift_yz.x
            let target_yz_local_x = p_xy.y - shift_yz.x;

            let start_idy =
                v_yz_sorted.partition_point(|v| v.point.x < target_yz_local_x - MATCH_TOLERANCE);

            for j in start_idy..v_yz_sorted.len() {
                let v3 = v_yz_sorted[j];
                if v3.point.x > target_yz_local_x + MATCH_TOLERANCE {
                    break;
                }

                // Candidate v3 found (matches Y)
                // Check Z match: V_yz.y (plus shift) should match p_xz.y (Global Z)
                let p_yz = v3.point + shift_yz;
                if (p_xz.y - p_yz.y).abs() <= MATCH_TOLERANCE {
                    // All coordinates match!
                    lambda.push(LambdaRow {
                        p3: Point3::new(p_xy.x, p_xy.y, p_xz.y),
                        v_xy_id: v1.id,
                        v_xz_id: v2.id,
                        v_yz_id: v3.id,
                    });
                }
            }
        }
    }
    lambda
}

fn build_theta_optimized(
    lambda: &[LambdaRow],
    v_xy: &View2D,
    v_xz: &View2D,
    v_yz: &View2D,
) -> HashSet<ThetaEdge> {
    let mut theta = HashSet::new();

    // Pre-hash edges for true O(1) lookup
    // Store as sorted pairs (min, max) to handle undirectedness
    let edges_xy: HashSet<(usize, usize)> = v_xy
        .edges
        .iter()
        .map(|e| {
            if e.start < e.end {
                (e.start, e.end)
            } else {
                (e.end, e.start)
            }
        })
        .collect();

    let edges_xz: HashSet<(usize, usize)> = v_xz
        .edges
        .iter()
        .map(|e| {
            if e.start < e.end {
                (e.start, e.end)
            } else {
                (e.end, e.start)
            }
        })
        .collect();

    let edges_yz: HashSet<(usize, usize)> = v_yz
        .edges
        .iter()
        .map(|e| {
            if e.start < e.end {
                (e.start, e.end)
            } else {
                (e.end, e.start)
            }
        })
        .collect();

    let edge_exists = |edges: &HashSet<(usize, usize)>, id1: usize, id2: usize| -> bool {
        if id1 == id2 {
            return true;
        } // Projecting an edge to a point is valid
        let key = if id1 < id2 { (id1, id2) } else { (id2, id1) };
        edges.contains(&key)
    };

    // Iterate all pairs of lambda
    // O(Lambda^2) - can be optimized further with adjacency lists if needed,
    // but this avoids the O(E) inner loop.
    for i in 0..lambda.len() {
        for j in (i + 1)..lambda.len() {
            let L1 = &lambda[i];
            let L2 = &lambda[j];

            let ok_xy = edge_exists(&edges_xy, L1.v_xy_id, L2.v_xy_id);
            if !ok_xy {
                continue;
            } // Fail fast

            let ok_xz = edge_exists(&edges_xz, L1.v_xz_id, L2.v_xz_id);
            if !ok_xz {
                continue;
            }

            let ok_yz = edge_exists(&edges_yz, L1.v_yz_id, L2.v_yz_id);
            if !ok_yz {
                continue;
            }

            theta.insert(ThetaEdge {
                start_lambda_idx: i,
                end_lambda_idx: j,
            });
        }
    }

    theta
}
