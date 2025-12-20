use anyhow::{bail, Context, Result};
use cadconvert_core::analysis::{AnalysisConfig, Analyzer};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(name = "cadconvert")]
#[command(about = "Deterministic 2D drawing analysis + 3D reconstruction (WIP).")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Analyze {
        input: PathBuf,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long)]
        dump_drawing: Option<PathBuf>,
        #[arg(long)]
        step: Option<PathBuf>,
        #[arg(long, default_value_t = 0.02)]
        view_gap_factor: f64,
        #[arg(long, default_value_t = 10)]
        min_cluster_entities: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Analyze {
            input,
            report,
            dump_drawing,
            step,
            view_gap_factor,
            min_cluster_entities,
        } => analyze(
            &input,
            report.as_deref(),
            dump_drawing.as_deref(),
            step.as_deref(),
            view_gap_factor,
            min_cluster_entities,
        ),
    }
}

fn analyze(
    input: &Path,
    report: Option<&Path>,
    dump_drawing: Option<&Path>,
    step: Option<&Path>,
    view_gap_factor: f64,
    min_cluster_entities: usize,
) -> Result<()> {
    ensure_input_file(input)?;

    let ext = input
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let (format, drawing) = match ext.as_str() {
        "dxf" => ("dxf", cadconvert_import_dxf::import_dxf(input)?),
        "svg" => ("svg", cadconvert_import_svg::import_svg(input)?),
        "dwg" => bail!("DWG import not implemented yet (planned via ODA/Teigha adapter)."),
        _ => bail!("Unsupported input extension: .{ext}"),
    };

    let cfg = AnalysisConfig {
        view_gap_factor,
        min_cluster_entities,
        ..AnalysisConfig::default()
    };

    let mut normalized = drawing.clone();
    let _ = cadconvert_core::normalize::normalize_in_place(&mut normalized, &cfg.normalize);

    if let Some(path) = dump_drawing {
        let json = serde_json::to_string_pretty(&normalized).context("serialize drawing")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &json).with_context(|| format!("write drawing: {path:?}"))?;
    }

    if let Some(path) = step {
        let name = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("drawing");
        let step_data = cadconvert_core::step::wireframe_step(&normalized, name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &step_data).with_context(|| format!("write step: {path:?}"))?;
    }

    let analyzer = Analyzer::new(cfg);

    let report_data = analyzer.analyze(format, &drawing);
    let json = serde_json::to_string_pretty(&report_data).context("serialize report")?;

    if let Some(path) = report {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &json).with_context(|| format!("write report: {path:?}"))?;
    } else {
        println!("{json}");
    }

    Ok(())
}

fn ensure_input_file(input: &Path) -> Result<()> {
    match std::fs::metadata(input) {
        Ok(meta) => {
            if meta.is_file() {
                Ok(())
            } else {
                bail!("input is not a file: {input:?}");
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let hint_root = find_workspace_root(&cwd);
            if let Some(root) = hint_root {
                bail!(
                    "input not found: {input:?} (cwd: {cwd:?}).\nHint: run from the workspace root {root:?} or pass an absolute path."
                );
            }
            bail!("input not found: {input:?} (cwd: {cwd:?}).");
        }
        Err(err) => Err(err).with_context(|| format!("stat input: {input:?}")),
    }
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .map(|dir| dir.to_path_buf())
}
