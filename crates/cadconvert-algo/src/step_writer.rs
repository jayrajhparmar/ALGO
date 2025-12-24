use crate::structs::{LambdaRow, ThetaEdge};
use anyhow::{Context, Result};
use std::fmt::Write;

pub fn write_step(
    lambda: &[LambdaRow],
    theta: &std::collections::HashSet<ThetaEdge>,
) -> Result<String> {
    let mut out = String::new();
    let timestamp = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");

    // Standard Header
    writeln!(out, "ISO-10303-21;")?;
    writeln!(out, "HEADER;")?;
    writeln!(
        out,
        "FILE_DESCRIPTION(('Reconstructed 3D Wireframe'), '2;1');"
    )?;
    writeln!(out, "FILE_NAME('reconstruction.stp', '{}', ('Aditya'), ('CadConvert'), 'Preprocessor v1', 'CadConvert Algo', '');", timestamp)?;
    writeln!(
        out,
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN {{1 0 10303 214 1 1 1 1}}'));"
    )?;
    writeln!(out, "ENDSEC;")?;
    writeln!(out, "DATA;")?;

    let mut id = 10;

    // Top-Level Infrastructure (AP214 boilerplate)
    // 1. Application Context
    writeln!(out, "#{}=APPLICATION_CONTEXT('automotive design');", id)?;
    let id_app_ctx = id;
    id += 1;
    writeln!(out, "#{}=APPLICATION_PROTOCOL_DEFINITION('international standard', 'automotive_design', 2000, #{});", id, id_app_ctx)?;
    id += 1;
    writeln!(
        out,
        "#{}=PRODUCT_DEFINITION_CONTEXT('part definition', #{}, 'design');",
        id, id_app_ctx
    )?;
    let id_prod_def_ctx = id;
    id += 1;

    // 2. Product
    writeln!(
        out,
        "#{}=PRODUCT('Product1', 'Part1', '', (#{}));",
        id, id_prod_def_ctx
    )?;
    let id_prod = id;
    id += 1;
    writeln!(
        out,
        "#{}=PRODUCT_DEFINITION_FORMATION('1', 'First Version', #{});",
        id, id_prod
    )?;
    let id_pdf = id;
    id += 1;
    writeln!(
        out,
        "#{}=PRODUCT_DEFINITION('design', '', #{}, #{});",
        id, id_pdf, id_prod_def_ctx
    )?;
    let id_pd = id;
    id += 1;

    // 3. Shape Definition
    writeln!(
        out,
        "#{}=PRODUCT_DEFINITION_SHAPE('Shape1', 'Shape', #{});",
        id, id_pd
    )?;
    let id_pds = id;
    id += 1;

    // 4. Shape Representation Relationship
    // We will define the Shape Representation later after collecting geometry items.
    let id_sdr = id;
    id += 1;
    let id_shape_rep = id;
    id += 1;

    writeln!(
        out,
        "#{}=SHAPE_DEFINITION_REPRESENTATION(#{}, #{});",
        id_sdr, id_pds, id_shape_rep
    )?;
    // Representation depends on context
    writeln!(
        out,
        "#{}=GEOMETRIC_REPRESENTATION_CONTEXT('3D Context', 'World', 3);",
        id
    )?;
    let id_geom_ctx = id;
    id += 1;
    writeln!(
        out,
        "#{}=GLOBAL_UNIT_ASSIGNED_CONTEXT((#{}, #{}, #{}), #{});",
        id,
        id + 1,
        id + 2,
        id + 3,
        id_geom_ctx
    )?;
    let id_guac = id;
    id += 1;

    writeln!(
        out,
        "#{}=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));",
        id
    )?;
    id += 1;
    writeln!(
        out,
        "#{}=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));",
        id
    )?;
    id += 1;
    writeln!(out, "#{}=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.));", id)?;
    id += 1;

    // Write Geometry

    // Write Vertices (CARTESIAN_POINT)
    let mut point_ids = vec![0; lambda.len()];

    for (i, row) in lambda.iter().enumerate() {
        let pid = id;
        id += 1;
        point_ids[i] = pid;
        writeln!(
            out,
            "#{}=CARTESIAN_POINT('',({:.6},{:.6},{:.6}));",
            pid, row.p3.x, row.p3.y, row.p3.z
        )?;

        let vid = id;
        id += 1;
        writeln!(out, "#{}=VERTEX_POINT('',#{});", vid, pid)?;
    }

    // Write Edges (EDGE_CURVE)
    let mut edge_ids = Vec::new();
    for edge in theta {
        let p1_id = point_ids[edge.start_lambda_idx];
        let p2_id = point_ids[edge.end_lambda_idx];

        // Ensure we refer to proper VERTEX_POINTs (pid + 1)
        let v1_id = p1_id + 1;
        let v2_id = p2_id + 1;

        let p1_coords = lambda[edge.start_lambda_idx].p3;
        let p2_coords = lambda[edge.end_lambda_idx].p3;

        // Normalize Direction
        let mut dx = p2_coords.x - p1_coords.x;
        let mut dy = p2_coords.y - p1_coords.y;
        let mut dz = p2_coords.z - p1_coords.z;
        let mag = (dx * dx + dy * dy + dz * dz).sqrt();
        if mag > 1e-9 {
            dx /= mag;
            dy /= mag;
            dz /= mag;
        } else {
            dx = 1.0;
            dy = 0.0;
            dz = 0.0; // Degenerate?
        }

        let dir_id = id;
        id += 1;
        writeln!(
            out,
            "#{}=DIRECTION('',({:.6},{:.6},{:.6}));",
            dir_id, dx, dy, dz
        )?;

        let vector_id = id;
        id += 1;
        writeln!(out, "#{}=VECTOR('',#{},{:.6});", vector_id, dir_id, mag)?;

        let line_id = id;
        id += 1;
        writeln!(out, "#{}=LINE('',#{},#{});", line_id, p1_id, vector_id)?;

        let edge_id = id;
        id += 1;
        writeln!(
            out,
            "#{}=EDGE_CURVE('',#{},#{},#{},.T.);",
            edge_id, v1_id, v2_id, line_id
        )?;
        edge_ids.push(edge_id);
    }

    // Wrap in GEOMETRIC_CURVE_SET
    let set_id = id;
    id += 1;
    let edges_str = edge_ids
        .iter()
        .map(|id| format!("#{}", id))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(
        out,
        "#{}=GEOMETRIC_CURVE_SET('Wireframe',({}));",
        set_id, edges_str
    )?;

    // Definition of SHAPE_REPRESENTATION
    // It contains the GEOMETRIC_CURVE_SET. Context is id_guac.
    writeln!(
        out,
        "#{}=SHAPE_REPRESENTATION('Simple Shape', (#{}), #{});",
        id_shape_rep, set_id, id_guac
    )?;

    writeln!(out, "ENDSEC;")?;
    writeln!(out, "END-ISO-10303-21;")?;

    Ok(out)
}
