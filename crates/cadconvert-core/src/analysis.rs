use crate::geom::BBox2;
use crate::model::{Drawing2D, EntityKind};
use crate::normalize::{normalize_in_place, NormalizeConfig};
use crate::report::{AnalysisReport, StatsReport, ViewClusterReport, Warning};
use crate::view::assign_three_view_roles;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub view_gap_factor: f64,
    pub min_cluster_entities: usize,
    pub normalize: NormalizeConfig,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            view_gap_factor: 0.02,
            min_cluster_entities: 10,
            normalize: NormalizeConfig::default(),
        }
    }
}

pub struct Analyzer {
    cfg: AnalysisConfig,
}

impl Analyzer {
    pub fn new(cfg: AnalysisConfig) -> Self {
        Self { cfg }
    }

    pub fn analyze(&self, input_format: &str, drawing: &Drawing2D) -> AnalysisReport {
        let mut normalized = drawing.clone();
        let normalize_stats = normalize_in_place(&mut normalized, &self.cfg.normalize);

        let extents = normalized.extents();

        let mut warnings = Vec::new();
        if normalized.entities.is_empty() {
            warnings.push(Warning {
                code: "no_entities".to_string(),
                message: "No drawable entities found.".to_string(),
            });
        }

        let clusters = self.cluster_views(&normalized, extents);
        let view_assignment = assign_three_view_roles(&clusters);

        if clusters.len() < 2 {
            warnings.push(Warning {
                code: "views_not_detected".to_string(),
                message: "Detected fewer than 2 view clusters; check layers/scale or clustering config."
                    .to_string(),
            });
        }
        if clusters.len() == 3 && view_assignment.is_none() {
            warnings.push(Warning {
                code: "view_assignment_ambiguous".to_string(),
                message: "Detected 3 clusters but could not confidently assign front/top/right; will require user confirmation."
                    .to_string(),
            });
        }

        AnalysisReport {
            input_format: input_format.to_string(),
            stats: StatsReport {
                entities_total: drawing.entities.len(),
                entities_normalized: normalized.entities.len(),
                removed_degenerate_entities: normalize_stats.removed_degenerate_entities,
                inferred_kinds: normalize_stats.inferred_kinds,
                dims_total: drawing.dims.len(),
                texts_total: drawing.texts.len(),
            },
            extents,
            view_clusters: clusters,
            view_assignment,
            warnings,
        }
    }

    fn cluster_views(&self, drawing: &Drawing2D, extents: Option<BBox2>) -> Vec<ViewClusterReport> {
        let diag = extents.map(|b| b.diag()).unwrap_or(1.0);
        let gap = (diag * self.cfg.view_gap_factor).max(1e-6);

        let mut drawable_indices = Vec::new();
        for (i, e) in drawing.entities.iter().enumerate() {
            match e.kind {
                EntityKind::Dimension | EntityKind::Text | EntityKind::Hatch => {}
                _ => drawable_indices.push(i),
            }
        }

        if drawable_indices.is_empty() {
            return Vec::new();
        }

        let bboxes: Vec<BBox2> = drawable_indices
            .iter()
            .map(|&i| drawing.entities[i].bbox())
            .collect();

        let mut dsu = DisjointSet::new(drawable_indices.len());

        // Spatial hash: assign each entity bbox to grid cells.
        let cell = gap;
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (local_idx, bbox) in bboxes.iter().enumerate() {
            let min_x = (bbox.min.x / cell).floor() as i64;
            let max_x = (bbox.max.x / cell).floor() as i64;
            let min_y = (bbox.min.y / cell).floor() as i64;
            let max_y = (bbox.max.y / cell).floor() as i64;
            for gx in min_x..=max_x {
                for gy in min_y..=max_y {
                    grid.entry((gx, gy)).or_default().push(local_idx);
                }
            }
        }

        for (local_i, bbox_i) in bboxes.iter().enumerate() {
            let min_x = (bbox_i.min.x / cell).floor() as i64;
            let max_x = (bbox_i.max.x / cell).floor() as i64;
            let min_y = (bbox_i.min.y / cell).floor() as i64;
            let max_y = (bbox_i.max.y / cell).floor() as i64;

            // Only compare within nearby cells.
            for gx in (min_x - 1)..=(max_x + 1) {
                for gy in (min_y - 1)..=(max_y + 1) {
                    if let Some(candidates) = grid.get(&(gx, gy)) {
                        for &local_j in candidates {
                            if local_j <= local_i {
                                continue;
                            }
                            let bbox_j = &bboxes[local_j];
                            if bbox_i.distance_to(bbox_j) <= gap {
                                dsu.union(local_i, local_j);
                            }
                        }
                    }
                }
            }
        }

        let mut clusters: HashMap<usize, ClusterAccum> = HashMap::new();
        for (local_idx, bbox) in bboxes.iter().enumerate() {
            let root = dsu.find(local_idx);
            let entry = clusters.entry(root).or_insert_with(ClusterAccum::new);
            entry.count += 1;
            entry.bbox = entry.bbox.union(bbox);
            if entry.entity_id_sample.len() < 20 {
                let orig_idx = drawable_indices[local_idx];
                entry.entity_id_sample.push(drawing.entities[orig_idx].id);
            }
        }

        let mut reports: Vec<_> = clusters
            .into_values()
            .filter(|c| c.count >= self.cfg.min_cluster_entities)
            .enumerate()
            .map(|(id, c)| ViewClusterReport {
                id,
                entity_count: c.count,
                bbox: c.bbox,
                entity_id_sample: c.entity_id_sample,
            })
            .collect();

        // Sort left-to-right, top-to-bottom for stable ordering.
        reports.sort_by(|a, b| {
            let ac = a.bbox.center();
            let bc = b.bbox.center();
            ac.y
                .partial_cmp(&bc.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .reverse()
                .then_with(|| ac.x.partial_cmp(&bc.x).unwrap_or(std::cmp::Ordering::Equal))
        });

        reports
    }
}

#[derive(Debug, Clone)]
struct ClusterAccum {
    count: usize,
    bbox: BBox2,
    entity_id_sample: Vec<u64>,
}

impl ClusterAccum {
    fn new() -> Self {
        Self {
            count: 0,
            bbox: BBox2::empty(),
            entity_id_sample: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl DisjointSet {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut ra = self.find(a);
        let mut rb = self.find(b);
        if ra == rb {
            return;
        }
        let rank_a = self.rank[ra];
        let rank_b = self.rank[rb];
        if rank_a < rank_b {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        if rank_a == rank_b {
            self.rank[ra] = rank_a.saturating_add(1);
        }
    }
}
