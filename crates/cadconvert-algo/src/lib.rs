use anyhow::{Result, bail};
use cadconvert_core::model::Drawing2D;

pub mod structs;
pub mod view_separation;
pub mod topology;
pub mod reconstruction;
pub mod solid_builder;
pub mod step_writer;

pub struct StepModel {
    pub content: String,
}

impl StepModel {
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, &self.content)?;
        Ok(())
    }
}

pub fn reconstruct_solid(drawing: &Drawing2D) -> Result<StepModel> {
    // 1. Separate views
    let (mut v_xy, mut v_xz, mut v_yz) = view_separation::separate_views(drawing)?;

    // 2. Build 2D Topology
    topology::build_topology(&mut v_xy)?;
    topology::build_topology(&mut v_xz)?;
    topology::build_topology(&mut v_yz)?;
    
    // 3. Build 3D Lambda/Theta
    let (lambda, theta) = reconstruction::build_reconstruction(&v_xy, &v_xz, &v_yz)?;
    
    // 4. Generate Solid (TODO)
    
    // Generate STEP content
    let step_content = step_writer::write_step(&lambda, &theta)?;

    Ok(StepModel { content: step_content })
}
