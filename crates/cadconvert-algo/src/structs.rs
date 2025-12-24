use cadconvert_core::model::Entity2D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewPlane {
    XY, // Top
    XZ, // Front
    YZ, // Side (Right)
}

pub struct View2D {
    pub plane: ViewPlane,
    pub raw_entities: Vec<Entity2D>,
    
    pub vertices: Vec<Vertex2D>,
    pub edges: Vec<Edge2D>,
}

#[derive(Debug, Clone)]
pub struct Vertex2D {
    pub id: usize,
    pub point: nalgebra::Point2<f64>,
}

#[derive(Debug, Clone)]
pub struct Edge2D {
    pub id: usize,
    pub start: usize, // Vertex ID
    pub end: usize,   // Vertex ID
    pub original_entity_id: Option<u64>,
}

impl View2D {
    pub fn new(plane: ViewPlane) -> Self {
        Self {
            plane,
            raw_entities: Vec::new(),
            vertices: Vec::new(),
            edges: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LambdaRow {
    pub p3: nalgebra::Point3<f64>,
    pub v_xy_id: usize,
    pub v_xz_id: usize,
    pub v_yz_id: usize,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ThetaEdge {
    pub start_lambda_idx: usize,
    pub end_lambda_idx: usize,
}
