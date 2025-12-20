    how # cadconvert (WIP)

Deterministic pipeline to convert 2D multi-view engineering drawings into validated 3D CAD solids.

Current state: import + analysis + view segmentation + JSON reporting (no 3D reconstruction yet).

## Goals

- Deterministic (no ML guessing): dimensions and projections drive the model.
- Robust: either produce a model that satisfies all constraints, or produce a precise ambiguity/inconsistency report.
- Fast: headless core + cached, incremental validation.

## Build

Rust toolchain required.

```bash
cargo build -p cadconvert
```

## Run

Analyze a DXF/SVG and emit a report:

```bash
cargo run -p cadconvert -- analyze fixtures/three_views.svg --report out/report.json
```

Dump the normalized canonical model:

```bash
cargo run -p cadconvert -- analyze fixtures/three_views.svg --dump-drawing out/drawing.json
```

## GUI

Minimal desktop UI for non-technical use (input preview + output paths + report view):

```bash
cargo run -p cadconvert-gui
```

## Docs

- `IMPLEMENTATION_PLAN.md`
