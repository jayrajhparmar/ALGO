use crate::geom::BBox2;
use crate::view::ViewAssignmentReport;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewClusterReport {
    pub id: usize,
    pub entity_count: usize,
    pub bbox: BBox2,
    pub entity_id_sample: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsReport {
    pub entities_total: usize,
    pub entities_normalized: usize,
    pub removed_degenerate_entities: usize,
    pub inferred_kinds: usize,
    pub dims_total: usize,
    pub texts_total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisReport {
    pub input_format: String,
    pub stats: StatsReport,
    pub extents: Option<BBox2>,
    pub view_clusters: Vec<ViewClusterReport>,
    pub view_assignment: Option<ViewAssignmentReport>,
    pub warnings: Vec<Warning>,
}
