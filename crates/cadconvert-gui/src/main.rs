use anyhow::{bail, Context, Result};
use cadconvert_core::analysis::{AnalysisConfig, Analyzer};
use cadconvert_core::geom::{BBox2, Vec2 as CadVec2};
use cadconvert_core::model::{Drawing2D, Primitive2D};
use cadconvert_core::normalize::{normalize_in_place, NormalizeConfig};
use cadconvert_core::report::AnalysisReport;
use cadconvert_core::view::{ProjectionScheme, ViewRole};
use eframe::egui;
use std::path::{Path, PathBuf};

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "cadconvert",
        native_options,
        Box::new(|cc| Ok(Box::new(CadConvertApp::new(cc)))),
    )
}

struct CadConvertApp {
    input_path: Option<PathBuf>,
    input_format: Option<String>,
    drawing: Option<Drawing2D>,
    drawing_extents: Option<BBox2>,

    report: Option<AnalysisReport>,

    view_gap_factor: f64,
    min_cluster_entities: usize,

    output_dir: Option<PathBuf>,
    out_report_path: Option<PathBuf>,
    out_drawing_path: Option<PathBuf>,
    out_step_path: Option<PathBuf>,

    zoom: f32,
    pan: egui::Vec2,

    status: String,
}

impl CadConvertApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            input_path: None,
            input_format: None,
            drawing: None,
            drawing_extents: None,
            report: None,
            view_gap_factor: 0.02,
            min_cluster_entities: 10,
            output_dir: None,
            out_report_path: None,
            out_drawing_path: None,
            out_step_path: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            status: "Open a DXF/SVG to begin.".to_string(),
        }
    }

    fn pick_input(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter("CAD drawings", &["dxf", "svg"])
            .pick_file();
        if let Some(path) = file {
            self.load_input(&path);
        }
    }

    fn pick_output_dir(&mut self) {
        let folder = rfd::FileDialog::new().pick_folder();
        if let Some(path) = folder {
            self.output_dir = Some(path);
        }
    }

    fn load_input(&mut self, path: &Path) {
        match self.import_any(path) {
            Ok((format, mut drawing)) => {
                let _ = normalize_in_place(&mut drawing, &NormalizeConfig::default());
                self.drawing_extents = drawing.extents();
                self.drawing = Some(drawing);
                self.input_path = Some(path.to_path_buf());
                self.input_format = Some(format.to_string());
                self.report = None;
                self.out_report_path = None;
                self.out_drawing_path = None;
                self.out_step_path = None;
                self.zoom = 1.0;
                self.pan = egui::Vec2::ZERO;

                if self.output_dir.is_none() {
                    let out = path
                        .parent()
                        .map(|p| p.join("cadconvert-out"))
                        .unwrap_or_else(|| PathBuf::from("out"));
                    self.output_dir = Some(out);
                }

                self.status = format!("Loaded {}", path.display());
            }
            Err(e) => {
                self.status = format!("Failed to load {}: {e}", path.display());
                self.drawing = None;
                self.drawing_extents = None;
                self.report = None;
            }
        }
    }

    fn import_any(&self, path: &Path) -> Result<(&'static str, Drawing2D)> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "dxf" => Ok(("dxf", cadconvert_import_dxf::import_dxf(path)?)),
            "svg" => Ok(("svg", cadconvert_import_svg::import_svg(path)?)),
            "dwg" => bail!("DWG import not implemented yet."),
            _ => bail!("Unsupported input extension: .{ext}"),
        }
    }

    fn run_analyze(&mut self) {
        let Some(drawing) = self.drawing.clone() else {
            self.status = "No drawing loaded.".to_string();
            return;
        };
        let Some(format) = self.input_format.clone() else {
            self.status = "No input format available.".to_string();
            return;
        };
        let Some(input_path) = self.input_path.clone() else {
            self.status = "No input path available.".to_string();
            return;
        };

        let out_dir = self
            .output_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("out"));

        let stem = input_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("drawing");

        let report_path = out_dir.join(format!("{stem}.report.json"));
        let drawing_path = out_dir.join(format!("{stem}.drawing.json"));
        let step_path = out_dir.join(format!("{stem}.step"));

        let cfg = AnalysisConfig {
            view_gap_factor: self.view_gap_factor,
            min_cluster_entities: self.min_cluster_entities,
            normalize: NormalizeConfig::default(),
        };
        let analyzer = Analyzer::new(cfg.clone());
        let report = analyzer.analyze(&format, &drawing);

        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            self.status = format!("Failed to create output dir {}: {e}", out_dir.display());
            return;
        }

        if let Err(e) = write_json(&report_path, &report) {
            self.status = format!("Failed to write report: {e}");
            return;
        }

        let mut normalized = drawing;
        let _ = normalize_in_place(&mut normalized, &cfg.normalize);
        if let Err(e) = write_json(&drawing_path, &normalized) {
            self.status = format!("Failed to write drawing dump: {e}");
            return;
        }

        self.report = Some(report);
        self.out_report_path = Some(report_path.clone());
        self.out_drawing_path = Some(drawing_path.clone());
        self.out_step_path = Some(step_path.clone());
        let step_data = cadconvert_core::step::wireframe_step(&normalized, stem);
        if let Err(e) = std::fs::write(&step_path, &step_data) {
            self.status = format!(
                "Wrote report: {} (drawing: {}, failed to write STEP: {e})",
                report_path.display(),
                drawing_path.display()
            );
            return;
        }

        self.status = format!(
            "Wrote report: {} (drawing: {}, step: {})",
            report_path.display(),
            drawing_path.display(),
            step_path.display()
        );
    }

    fn handle_file_drop(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        let Some(file) = dropped.into_iter().find(|f| f.path.is_some()) else {
            return;
        };
        if let Some(path) = file.path {
            self.load_input(&path);
        }
    }
}

impl eframe::App for CadConvertApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_file_drop(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open DXF/SVG…").clicked() {
                    self.pick_input();
                }
                if ui.button("Output folder…").clicked() {
                    self.pick_output_dir();
                }
                ui.separator();
                ui.add(
                    egui::DragValue::new(&mut self.view_gap_factor)
                        .speed(0.001)
                        .range(0.001..=0.5)
                        .prefix("view_gap_factor="),
                );
                ui.add(
                    egui::DragValue::new(&mut self.min_cluster_entities)
                        .range(1..=10_000)
                        .prefix("min_cluster_entities="),
                );
                ui.separator();
                let can_analyze = self.drawing.is_some();
                if ui
                    .add_enabled(can_analyze, egui::Button::new("Analyze → report.json"))
                    .clicked()
                {
                    self.run_analyze();
                }
            });

            if let Some(p) = &self.input_path {
                ui.label(format!("Input: {}", p.display()));
            }
            if let Some(p) = &self.output_dir {
                ui.label(format!("Output: {}", p.display()));
            }
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if let Some(p) = &self.out_report_path {
                    if ui.button("Copy report path").clicked() {
                        ui.ctx().copy_text(p.display().to_string());
                    }
                    if ui.button("Open report").clicked() {
                        let _ = open::that(p);
                    }
                    if let Some(dir) = p.parent() {
                        if ui.button("Open folder").clicked() {
                            let _ = open::that(dir);
                        }
                    }
                }
                if let Some(p) = &self.out_step_path {
                    if ui.button("Copy STEP path").clicked() {
                        ui.ctx().copy_text(p.display().to_string());
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                cols[0].heading("Input preview");
                cols[0].separator();
                draw_preview(&mut cols[0], self);

                cols[1].heading("Output");
                cols[1].separator();
                draw_output(&mut cols[1], self);
            });
        });
    }
}

fn draw_preview(ui: &mut egui::Ui, app: &mut CadConvertApp) {
    let Some(drawing) = &app.drawing else {
        ui.label("No input loaded.");
        return;
    };
    let Some(extents) = app.drawing_extents else {
        ui.label("No extents.");
        return;
    };

    let (rect, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
    let painter = ui.painter_at(rect);

    if response.dragged() {
        app.pan += response.drag_delta();
    }
    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let factor = (scroll / 200.0).exp();
            app.zoom = (app.zoom * factor).clamp(0.1, 30.0);
        }
    }

    let transform = WorldToScreen::new(rect, extents, app.pan, app.zoom);

    let stroke_obj = egui::Stroke::new(1.0, egui::Color32::BLACK);
    let stroke_hidden = egui::Stroke::new(1.0, egui::Color32::from_gray(140));
    let stroke_center = egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 120, 200));

    for ent in &drawing.entities {
        let stroke = match ent.kind {
            cadconvert_core::model::EntityKind::Hidden => stroke_hidden,
            cadconvert_core::model::EntityKind::Center => stroke_center,
            _ => stroke_obj,
        };
        draw_primitive(&painter, &transform, &ent.primitive, stroke);
    }

    // Cluster overlay
    if let Some(report) = &app.report {
        for (idx, c) in report.view_clusters.iter().enumerate() {
            let color = cluster_color(idx);
            let stroke = egui::Stroke::new(2.0, color);
            let r = transform.bbox_to_rect(c.bbox);
            painter.rect_stroke(
                r,
                egui::CornerRadius::same(0),
                stroke,
                egui::StrokeKind::Outside,
            );

            let mut label = format!("V{}", c.id);
            if let Some(assign) = &report.view_assignment {
                if let Some(role) = assign
                    .roles
                    .iter()
                    .find(|r| r.cluster_id == c.id)
                    .map(|r| r.role)
                {
                    let role = match role {
                        ViewRole::Front => "F",
                        ViewRole::Top => "T",
                        ViewRole::Right => "R",
                    };
                    label.push_str(&format!(" ({role})"));
                }
            }
            painter.text(
                r.min + egui::vec2(4.0, 4.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::monospace(12.0),
                color,
            );
        }
    }
}

fn draw_output(ui: &mut egui::Ui, app: &mut CadConvertApp) {
    if let Some(report) = &app.report {
        ui.label(format!(
            "Entities: {} → {} (removed {} degenerate, inferred {} kinds)",
            report.stats.entities_total,
            report.stats.entities_normalized,
            report.stats.removed_degenerate_entities,
            report.stats.inferred_kinds
        ));
        ui.label(format!("Dimensions: {}", report.stats.dims_total));
        ui.label(format!("Texts: {}", report.stats.texts_total));
        ui.label(format!("View clusters: {}", report.view_clusters.len()));

        if let Some(va) = &report.view_assignment {
            let scheme = match va.scheme {
                ProjectionScheme::ThirdAngle => "third-angle",
                ProjectionScheme::FirstAngle => "first-angle",
            };
            ui.label(format!(
                "3-view assignment: {scheme} (conf {:.0}%)",
                va.confidence * 100.0
            ));
        } else {
            ui.label("3-view assignment: (none / ambiguous)");
        }

        ui.separator();
        if let Some(p) = &app.out_report_path {
            ui.horizontal(|ui| {
                ui.label("Report:");
                ui.monospace(p.display().to_string());
            });
        }
        if let Some(p) = &app.out_drawing_path {
            ui.horizontal(|ui| {
                ui.label("Drawing dump:");
                ui.monospace(p.display().to_string());
            });
        }
        if let Some(p) = &app.out_step_path {
            ui.horizontal(|ui| {
                ui.label("CAD (STEP):");
                ui.monospace(p.display().to_string());
                let exists = p.exists();
                if !exists {
                    ui.label("(not generated yet)");
                }
                if ui.add_enabled(exists, egui::Button::new("Open")).clicked() {
                    let _ = open::that(p);
                }
            });
        }

        ui.separator();
        ui.collapsing("Raw report.json", |ui| {
            if let Ok(json) = serde_json::to_string_pretty(report) {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(json);
                });
            }
        });
    } else {
        ui.label("No output yet. Click “Analyze → report.json”.");
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value).context("serialize json")?;
    std::fs::write(path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn cluster_color(idx: usize) -> egui::Color32 {
    const PALETTE: [egui::Color32; 6] = [
        egui::Color32::from_rgb(0xE3, 0x4A, 0x33),
        egui::Color32::from_rgb(0x31, 0x8B, 0xBD),
        egui::Color32::from_rgb(0x31, 0xA3, 0x54),
        egui::Color32::from_rgb(0x75, 0x6B, 0xB1),
        egui::Color32::from_rgb(0xFD, 0x8D, 0x3C),
        egui::Color32::from_rgb(0x63, 0x63, 0x63),
    ];
    PALETTE[idx % PALETTE.len()]
}

#[derive(Debug, Clone, Copy)]
struct WorldToScreen {
    rect: egui::Rect,
    center: CadVec2,
    scale: f32,
    pan: egui::Vec2,
}

impl WorldToScreen {
    fn new(rect: egui::Rect, world: BBox2, pan: egui::Vec2, zoom: f32) -> Self {
        let center = world.center();
        let world_w = world.width().max(1e-6) as f32;
        let world_h = world.height().max(1e-6) as f32;
        let sx = rect.width() / world_w;
        let sy = rect.height() / world_h;
        let scale = (sx.min(sy) * 0.9).max(1e-3) * zoom;
        Self {
            rect,
            center,
            scale,
            pan,
        }
    }

    fn point(&self, p: CadVec2) -> egui::Pos2 {
        let dx = (p.x - self.center.x) as f32;
        let dy = (p.y - self.center.y) as f32;
        let x = self.rect.center().x + self.pan.x + dx * self.scale;
        let y = self.rect.center().y + self.pan.y - dy * self.scale;
        egui::pos2(x, y)
    }

    fn bbox_to_rect(&self, b: BBox2) -> egui::Rect {
        let p0 = self.point(b.min);
        let p1 = self.point(b.max);
        egui::Rect::from_min_max(
            egui::pos2(p0.x.min(p1.x), p0.y.min(p1.y)),
            egui::pos2(p0.x.max(p1.x), p0.y.max(p1.y)),
        )
    }
}

fn draw_primitive(
    painter: &egui::Painter,
    tx: &WorldToScreen,
    prim: &Primitive2D,
    stroke: egui::Stroke,
) {
    match prim {
        Primitive2D::Line(l) => {
            painter.line_segment([tx.point(l.a), tx.point(l.b)], stroke);
        }
        Primitive2D::Circle(c) => {
            let pts = circle_points(c.center, c.radius, 64)
                .into_iter()
                .map(|p| tx.point(p))
                .collect::<Vec<_>>();
            painter.add(egui::Shape::line(pts, stroke));
        }
        Primitive2D::Arc(a) => {
            let pts = arc_points(a.center, a.radius, a.start_angle_deg, a.end_angle_deg, 48)
                .into_iter()
                .map(|p| tx.point(p))
                .collect::<Vec<_>>();
            painter.add(egui::Shape::line(pts, stroke));
        }
        Primitive2D::Polyline(pl) => {
            if pl.vertices.len() < 2 {
                return;
            }
            for w in pl.vertices.windows(2) {
                painter.line_segment([tx.point(w[0].pos), tx.point(w[1].pos)], stroke);
            }
            if pl.closed {
                let a = pl.vertices.last().unwrap().pos;
                let b = pl.vertices.first().unwrap().pos;
                painter.line_segment([tx.point(a), tx.point(b)], stroke);
            }
        }
        Primitive2D::CubicBezier(b) => {
            let pts = bezier_points(b.clone(), 32)
                .into_iter()
                .map(|p| tx.point(p))
                .collect::<Vec<_>>();
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
}

fn circle_points(center: CadVec2, radius: f64, segments: usize) -> Vec<CadVec2> {
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = t * std::f64::consts::TAU;
        pts.push(CadVec2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    pts
}

fn arc_points(
    center: CadVec2,
    radius: f64,
    start_deg: f64,
    end_deg: f64,
    segments: usize,
) -> Vec<CadVec2> {
    let a0 = start_deg.to_radians();
    let mut a1 = end_deg.to_radians();
    if a1 < a0 {
        a1 += std::f64::consts::TAU;
    }
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        let a = a0 + (a1 - a0) * t;
        pts.push(CadVec2::new(
            center.x + radius * a.cos(),
            center.y + radius * a.sin(),
        ));
    }
    pts
}

fn bezier_points(b: cadconvert_core::model::Bezier2D, segments: usize) -> Vec<CadVec2> {
    let mut pts = Vec::with_capacity(segments + 1);
    for i in 0..=segments {
        let t = i as f64 / segments as f64;
        pts.push(bezier_eval(b.clone(), t));
    }
    pts
}

fn bezier_eval(b: cadconvert_core::model::Bezier2D, t: f64) -> CadVec2 {
    let u = 1.0 - t;
    let tt = t * t;
    let uu = u * u;
    let uuu = uu * u;
    let ttt = tt * t;

    let x = uuu * b.p0.x + 3.0 * uu * t * b.p1.x + 3.0 * u * tt * b.p2.x + ttt * b.p3.x;
    let y = uuu * b.p0.y + 3.0 * uu * t * b.p1.y + 3.0 * u * tt * b.p2.y + ttt * b.p3.y;
    CadVec2::new(x, y)
}
