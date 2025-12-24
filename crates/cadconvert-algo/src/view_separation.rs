use crate::structs::{View2D, ViewPlane};
use anyhow::{bail, Result};
use cadconvert_core::model::{Drawing2D, EntityKind};

pub fn separate_views(drawing: &Drawing2D) -> Result<(View2D, View2D, View2D)> {
    let mut v_xy = View2D::new(ViewPlane::XY);
    let mut v_xz = View2D::new(ViewPlane::XZ);
    let mut v_yz = View2D::new(ViewPlane::YZ);

    for entity in &drawing.entities {
        // Skip non-geometric entities (dims, text) for now if they are not critical
        // But for visual hull we need the lines.
        let layer_raw = entity.style.layer.as_deref().unwrap_or("0");
        let layer = layer_raw.to_ascii_uppercase();

        // println!("Entity {} on layer '{}'", entity.id, layer_raw);

        if entity.kind == EntityKind::Dimension || entity.kind == EntityKind::Text {
            continue;
        }

        // Simple heuristic: check if layer name contains plane name
        // Or default mapping:
        // "TOP", "XY" -> XY
        // "FRONT", "XZ" -> XZ
        // "RIGHT", "SIDE", "YZ" -> YZ

        if layer.contains("XY") || layer.contains("TOP") {
            v_xy.raw_entities.push(entity.clone());
        } else if layer.contains("XZ") || layer.contains("FRONT") {
            v_xz.raw_entities.push(entity.clone());
        } else if layer.contains("YZ") || layer.contains("RIGHT") || layer.contains("SIDE") {
            v_yz.raw_entities.push(entity.clone());
            // Optional: what to do with unassigned layers?
            // println!("Warning: Unassigned entity {} on layer '{}'", entity.id, layer_raw);
        }
    }

    println!(
        "View Separation Stats: XY={}, XZ={}, YZ={}",
        v_xy.raw_entities.len(),
        v_xz.raw_entities.len(),
        v_yz.raw_entities.len()
    );

    // If logical layers failed, try spatial separation
    if v_xy.raw_entities.is_empty() && v_xz.raw_entities.is_empty() && v_yz.raw_entities.is_empty()
    {
        println!("Layer separation failed. Attempting spatial clustering...");
        return separate_spatially(drawing);
    }

    Ok((v_xy, v_xz, v_yz))
}

fn separate_spatially(drawing: &Drawing2D) -> Result<(View2D, View2D, View2D)> {
    // 1. Collect all valid geometric entities
    let mut valid_ents = Vec::new();
    for ent in &drawing.entities {
        if ent.kind != EntityKind::Dimension && ent.kind != EntityKind::Text {
            valid_ents.push(ent.clone());
        }
    }

    if valid_ents.is_empty() {
        bail!("No geometry found in drawing");
    }

    // 2. Simple clustering: Group by connectivity or proximity
    // For a robust start, let's sort by centroids.
    // Assuming 3 distinct views separated by whitespace.

    // We can merge entities that are close to each other.
    let mut groups: Vec<Vec<cadconvert_core::model::Entity2D>> = Vec::new();

    // Naive O(N^2) merge loop (acceptable for N < 20000)
    // Or use a grid? Let's use bounding box expansion intersection.
    let mut definitions: Vec<(
        cadconvert_core::geom::BBox2,
        Vec<cadconvert_core::model::Entity2D>,
    )> = valid_ents
        .into_iter()
        .map(|e| (e.bbox().expand(5.0), vec![e]))
        .collect();

    // Iteratively merge intersecting boxes
    let mut changed = true;
    while changed {
        changed = false;
        let mut i = 0;
        while i < definitions.len() {
            let mut j = i + 1;
            while j < definitions.len() {
                if !definitions[i].0.union(&definitions[j].0).is_empty()
                    && definitions[i].0.distance_to(&definitions[j].0) < 1.0
                {
                    // Merge j into i
                    let other = definitions.remove(j);
                    definitions[i].0 = definitions[i].0.union(&other.0);
                    definitions[i].1.extend(other.1);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }

    // We hope for exactly 3 groups.
    println!("Found {} spatial clusters.", definitions.len());

    // If not 3, try K-Means fallback if we have just 1 giant cluster
    if definitions.len() == 1 {
        println!("Only 1 cluster found. Trying K-Means(k=3) force split...");
        definitions = run_kmeans_k3(&definitions[0].1);
    } else if definitions.len() == 2 {
        println!("Only 2 clusters found. Splitting the largest one...");
        // Find largest
        let (max_idx, _) = definitions
            .iter()
            .enumerate()
            .max_by_key(|(_, d)| d.1.len())
            .unwrap();
        let large_cluster = definitions.remove(max_idx);

        let split_clusters = run_kmeans_k2(&large_cluster.1);
        if split_clusters.len() == 2 {
            definitions.extend(split_clusters);
            println!("Split successful. Now have {} clusters.", definitions.len());
        } else {
            // Split failed? logic error?
            definitions.push(large_cluster);
            println!("Split failed to produce 2 distinct groups.");
        }
    }

    if definitions.len() != 3 {
        // Fallback: Just take the 3 largest groups?
        definitions.sort_by_key(|g| std::cmp::Reverse(g.1.len()));
        if definitions.len() > 3 {
            println!("Using top 3 largest clusters.");
            definitions.truncate(3);
        } else if definitions.len() < 3 {
            bail!(
                "Found only {} clusters. Need 3 views (XY, XZ, YZ).",
                definitions.len()
            );
        }
    }

    // 3. Assign views based on centroids
    // Calculate centers
    let centers: Vec<cadconvert_core::geom::Vec2> =
        definitions.iter().map(|d| d.0.center()).collect();

    // Identifying views by relative position.
    // Front view usually central.
    // Top is roughly same X, higher Y? (Or simply Higher Y)
    // Right is roughly same Y, higher X? (Or simply Higher X)

    // Sort by Y to find Top (Highest Y) vs Bottom Row (Front + Side)
    let mut indices: Vec<usize> = (0..3).collect();
    indices.sort_by(|&a, &b| centers[a].y.partial_cmp(&centers[b].y).unwrap());

    // indices[0] is Lowest Y
    // indices[2] is Highest Y (Top View)
    let top_idx = indices[2];

    // The other two (0 and 1) are Front and Side.
    // Side is usually Right of Front.
    // Sort remaining by X.
    let mut bottom_row = vec![indices[0], indices[1]];
    bottom_row.sort_by(|&a, &b| centers[a].x.partial_cmp(&centers[b].x).unwrap());

    let front_idx = bottom_row[0]; // Left-most of bottom row
    let side_idx = bottom_row[1]; // Right-most of bottom row

    // Wait, check alignment:
    // Top and Front should align in X.
    // Front and Side should align in Y.
    // Does Top align with Side? No.
    // Let's refine based on X alignment if possible.
    // But failing that, simple position is best guess.

    println!(
        "Assigned views: Top (Cluster {}), Front (Cluster {}), Side (Cluster {})",
        top_idx, front_idx, side_idx
    );

    let mut v_xy = View2D::new(ViewPlane::XY);
    v_xy.raw_entities = definitions[top_idx].1.clone();

    let mut v_xz = View2D::new(ViewPlane::XZ);
    v_xz.raw_entities = definitions[front_idx].1.clone();

    let mut v_yz = View2D::new(ViewPlane::YZ);
    v_yz.raw_entities = definitions[side_idx].1.clone();

    Ok((v_xy, v_xz, v_yz))
}

fn run_kmeans_k3(
    entities: &[cadconvert_core::model::Entity2D],
) -> Vec<(
    cadconvert_core::geom::BBox2,
    Vec<cadconvert_core::model::Entity2D>,
)> {
    if entities.is_empty() {
        return Vec::new();
    }

    // 1. Init Centroids (Heuristic: Top, Front, Side)
    // Global BBox
    let mut global_bbox = cadconvert_core::geom::BBox2::empty();
    for e in entities {
        global_bbox = global_bbox.union(&e.bbox());
    }

    let min = global_bbox.min;
    let max = global_bbox.max;
    let w = max.x - min.x;
    let h = max.y - min.y;

    // C1 (Top): Top-Leftish (aligned with Front in X) -> (min + w*0.25, max - h*0.25)
    // C2 (Front): Bottom-Left -> (min + w*0.25, min + h*0.25)
    // C3 (Side): Bottom-Right -> (max - w*0.25, min + h*0.25)

    let mut centers = vec![
        cadconvert_core::geom::Vec2::new(min.x + w * 0.25, max.y - h * 0.25), // Top
        cadconvert_core::geom::Vec2::new(min.x + w * 0.25, min.y + h * 0.25), // Front
        cadconvert_core::geom::Vec2::new(max.x - w * 0.25, min.y + h * 0.25), // Side
    ];

    // 2. Iterate
    let mut assignments = vec![0; entities.len()];
    for _iter in 0..10 {
        // Assign
        let mut sums = vec![cadconvert_core::geom::Vec2::new(0.0, 0.0); 3];
        let mut counts = vec![0; 3];

        for (i, ent) in entities.iter().enumerate() {
            let c = ent.bbox().center();
            let mut best_dist = f64::INFINITY;
            let mut best_k = 0;
            for k in 0..3 {
                let d = (c.x - centers[k].x).hypot(c.y - centers[k].y);
                if d < best_dist {
                    best_dist = d;
                    best_k = k;
                }
            }
            assignments[i] = best_k;
            sums[best_k].x += c.x;
            sums[best_k].y += c.y;
            counts[best_k] += 1;
        }

        // Update
        let mut moved = 0.0;
        for k in 0..3 {
            if counts[k] > 0 {
                let new_c = cadconvert_core::geom::Vec2::new(
                    sums[k].x / counts[k] as f64,
                    sums[k].y / counts[k] as f64,
                );
                moved += (new_c.x - centers[k].x).hypot(new_c.y - centers[k].y);
                centers[k] = new_c;
            }
        }
        if moved < 0.1 {
            break;
        }
    }

    // 3. Group
    let mut clusters = vec![Vec::new(); 3];
    for (i, &k) in assignments.iter().enumerate() {
        clusters[k].push(entities[i].clone());
    }

    // Remove empty clusters if any (bad init?)
    let mut result = Vec::new();
    for grp in clusters {
        if !grp.is_empty() {
            let mut box_ = cadconvert_core::geom::BBox2::empty();
            for e in &grp {
                box_ = box_.union(&e.bbox());
            }
            result.push((box_, grp));
        }
    }
    result
}

fn run_kmeans_k2(
    entities: &[cadconvert_core::model::Entity2D],
) -> Vec<(
    cadconvert_core::geom::BBox2,
    Vec<cadconvert_core::model::Entity2D>,
)> {
    if entities.is_empty() {
        return Vec::new();
    }

    let mut global_bbox = cadconvert_core::geom::BBox2::empty();
    for e in entities {
        global_bbox = global_bbox.union(&e.bbox());
    }

    let min = global_bbox.min;
    let max = global_bbox.max;
    let w = max.x - min.x;
    let h = max.y - min.y;

    // Heuristic: Split along major axis
    let mut centers = if w > h {
        // Horizontal split (Left / Right)
        vec![
            cadconvert_core::geom::Vec2::new(min.x + w * 0.25, min.y + h * 0.5),
            cadconvert_core::geom::Vec2::new(max.x - w * 0.25, min.y + h * 0.5),
        ]
    } else {
        // Vertical split (Top / Bottom)
        vec![
            cadconvert_core::geom::Vec2::new(min.x + w * 0.5, max.y - h * 0.25), // Top
            cadconvert_core::geom::Vec2::new(min.x + w * 0.5, min.y + h * 0.25), // Bottom
        ]
    };

    // Iterate
    let mut assignments = vec![0; entities.len()];
    for _iter in 0..10 {
        let mut sums = vec![cadconvert_core::geom::Vec2::new(0.0, 0.0); 2];
        let mut counts = vec![0; 2];

        for (i, ent) in entities.iter().enumerate() {
            let c = ent.bbox().center();
            let mut best_dist = f64::INFINITY;
            let mut best_k = 0;
            for k in 0..2 {
                let d = (c.x - centers[k].x).hypot(c.y - centers[k].y);
                if d < best_dist {
                    best_dist = d;
                    best_k = k;
                }
            }
            assignments[i] = best_k;
            sums[best_k].x += c.x;
            sums[best_k].y += c.y;
            counts[best_k] += 1;
        }

        let mut moved = 0.0;
        for k in 0..2 {
            if counts[k] > 0 {
                let new_c = cadconvert_core::geom::Vec2::new(
                    sums[k].x / counts[k] as f64,
                    sums[k].y / counts[k] as f64,
                );
                moved += (new_c.x - centers[k].x).hypot(new_c.y - centers[k].y);
                centers[k] = new_c;
            }
        }
        if moved < 0.1 {
            break;
        }
    }

    let mut clusters = vec![Vec::new(); 2];
    for (i, &k) in assignments.iter().enumerate() {
        clusters[k].push(entities[i].clone());
    }

    let mut result = Vec::new();
    for grp in clusters {
        if !grp.is_empty() {
            let mut box_ = cadconvert_core::geom::BBox2::empty();
            for e in &grp {
                box_ = box_.union(&e.bbox());
            }
            result.push((box_, grp));
        }
    }
    result
}
