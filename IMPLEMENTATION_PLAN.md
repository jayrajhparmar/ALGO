# Implementation Plan (Deterministic, Robust, Fast)

This doc describes the **target architecture**, **detailed data flow**, and a practical **implementation sequence**.

The key design principle is: **never guess silently**.
If the drawing is ambiguous or inconsistent, the system must either:

1. produce an explicit **question** that resolves the ambiguity with minimal user input, or
2. produce an explicit **contradiction report** listing the smallest set of constraints that cannot be satisfied together.

## 0) Definitions (what “robust” means here)

Robustness is not “always output something”.
Robustness is:

- Accurate when dimensions are trusted: the 3D result satisfies every dimension (within tolerance).
- Deterministic: same input → same output + same questions.
- Safe failure: underdefined/inconsistent drawings produce a report of **what’s missing / conflicting**, not a wrong 3D.

## 1) Target product shape

### Inputs (initial focus)

- DXF (ACAD 2000+ ASCII/Binary)
- DWG (later via ODA/Teigha adapter)
- SVG (common export path; dims may be real or “exploded”)

### Outputs

- STEP (B-Rep solid) with a validation report.
- Optional: 2D projection previews (SVG/PNG) generated from the 3D for QA.

### UI

- Phase 1: CLI + JSON reports.
- Phase 2: Thin “review wizard” UI that only:
  - confirms detected views and projection scheme (1st/3rd angle),
  - asks a small number of questions when ambiguous,
  - exports STEP + report.

## 2) Core pipeline (high-level)

1. **Import** → convert source file into a canonical 2D model.
2. **Normalize** → snap/merge/fit so downstream logic sees clean topology.
3. **Semantics** → classify entities (object/hidden/center/dimension/text/hatch).
4. **View segmentation** → group entities into per-view clusters.
5. **View registration** → solve scale/origin/axes per view; detect broken projection.
6. **Constraint extraction** → dimensions/GD&T → constraints with tolerance.
7. **Reconstruction** → build 3D model that satisfies constraints.
8. **Validation** → reproject 3D into each view and verify geometry + dimensions.
9. **Export** → STEP + a full traceable report.

The engine should support incremental iteration:

- Add one constraint → update model → revalidate quickly.
- Ask one question → apply answer → update model → revalidate.

## 3) Canonical data model (critical for determinism)

### 3.1 Canonical2D

All importers (DXF/DWG/SVG/PDF/raster) must produce the same internal model:

- Geometry primitives (2D, Z=0 for view sketches):
  - line segment, circular arc, circle, polyline (with bulge), spline/bezier, ellipse.
- Styling/metadata:
  - layer name, linetype, color, stroke dash pattern, lineweight.
- Semantics:
  - `EntityKind`: object / hidden / center / dimension / text / hatch / unknown.
- Annotations:
  - Dimension entities (when real DIM objects exist) and text blocks (for exploded dims).
- Provenance:
  - original entity id/handle + source file path to enable traceability.

### 3.2 Constraints

Each dimension becomes a constraint with:

- type: distance / angle / radius / diameter / concentricity / symmetry / etc.
- target: references to canonical geometry (edges/points/axes) with confidence.
- value + tolerance.
- source: exact DIM object or extracted text + leader geometry.

Constraints must be able to be:

- satisfied by a solver (parameter update),
- evaluated (residual + pass/fail),
- explained (why it failed, what it references).

## 4) Deterministic view segmentation + registration

### 4.1 View segmentation (no ML)

Goal: group entities into view candidates.

Deterministic approach:

1. Remove obvious non-geometry (dims/text/hatch) for clustering.
2. Compute each entity’s bounding box.
3. Cluster with a spatial hash + union-find:
   - entities whose bboxes intersect or are within a gap threshold belong together.
4. Produce cluster bounding boxes → view candidates.

Output:

- list of view clusters with bounding boxes + entity ids.
- warnings if clusters overlap heavily or counts are unexpected.

### 4.2 View registration

Goal: assign each cluster a role (front/top/right/aux/section) and compute transforms.

Deterministic cues:

- relative placement of clusters (layout rules for 1st/3rd angle),
- shared centerlines/axes,
- shared bounding dimensions,
- explicit labels when present (e.g. “FRONT VIEW”).

If dimensions are trusted:

- solve a scale factor per view from dimension measurements,
- if inconsistent scale appears: report “broken drawing” + ask which is correct.

## 5) Reconstruction strategies (multi-strategy, deterministic)

There is no single universal method; use a set of strategies with explicit applicability rules.

### 5.1 Strategy A: Axisymmetric (revolve)

Detectable by:

- a dominant centerline/axis,
- diameter/radius dimensions,
- profile in one view.

Steps:

1. Extract a clean 2D profile loop.
2. Solve profile parameters from dims.
3. OCC revolve → solid.
4. Add features (holes, cuts) from other views/sections.
5. Validate projections + dims.

### 5.2 Strategy B: Prismatic (extrusion + cuts)

Detectable by:

- mostly rectilinear silhouettes,
- thickness/height dimensions,
- consistent orthographic views.

Steps:

1. Build a fast “envelope” (visual hull) from silhouettes (extrude + boolean common).
2. Identify internal loops/hidden-line cues for pockets/cuts.
3. Apply boolean cuts driven by dimensions.
4. Validate by reprojection + dims.

### 5.3 Strategy C: General surfaces (loft/sweep/fillet-heavy)

Deterministic but only when fully defined:

- explicit section profiles,
- guide curves,
- radii/blend annotations.

Approach:

- build a constrained surface network from profiles/sections,
- sew into a shell, thicken if needed,
- validate all given dimensions and silhouette projections.

### 5.4 Fillets/chamfers

Always last.

Reason:

- booleans + blends are fragile; doing blends early creates downstream failures.

Deterministic rule:

- only apply a fillet if the edge is uniquely identifiable and its radius is specified.

## 6) Validation (the “robustness engine”)

Validation is mandatory; reconstruction without validation is not acceptable.

### 6.1 Projection validation

From 3D model:

- compute hidden/visible edges per view (OCC HLR),
- compare with expected 2D geometry:
  - silhouettes must match (within tolerance),
  - key edges must exist where specified,
  - hidden edges should appear where hidden-line types indicate.

### 6.2 Dimension validation

For each trusted dimension:

- measure on the 3D model (distance/angle/radius/diameter),
- check residual within tolerance,
- report any failing constraint with:
  - measured value,
  - expected value,
  - the referenced geometry.

### 6.3 Minimal contradiction set (when failing)

When constraints cannot be satisfied:

- run a conflict-minimization routine to return a small set of conflicting dims.
- this is what enables actionable “people messed up” reporting.

## 7) Question engine (minimal human input)

When underdefined:

- generate targeted questions with limited choices:
  - “Is this hole THROUGH or BLIND?”
  - “Which view is FRONT?”
  - “Which circle does ⌀10 refer to?” (highlight candidates)

Answers become constraints and re-run the solve/validate loop.

## 8) Implementation sequence (pragmatic)

### Phase 1: Canonical2D + Analysis (DONE FIRST)

- DXF import with real DIM entities.
- SVG import of basic primitives (line/polyline/polygon/circle/path).
- Normalization + entity bbox computation.
- View segmentation (clusters) + report.

Deliverable:

- CLI that produces `report.json` and warns about ambiguous view detection.

### Phase 2: Constraints + Registration

- Dimension parsing to constraint objects (DXF dimension types first).
- Associate dimensions to geometry (direct DIM refs when available; exploded text heuristics).
- Solve view scale/origin; detect broken projections.

Deliverable:

- “dimension health report”: which dims are resolved, unresolved, or contradictory.

### Phase 3: First reconstruction strategy (Prismatic OR Revolve)

Pick one based on your most common drawings.

- Build base solid in OCC.
- Add holes (cylinders) and simple cuts.
- Validate via reprojection + dimensions.

Deliverable:

- STEP export for a narrow but real class of parts with full validation.

### Phase 4: Expand feature coverage

- Slots, counterbores/countersinks, pockets.
- Patterns (linear/circular).
- Fillets/chamfers last.

### Phase 5: Input expansion + UI

- DWG adapter (ODA).
- Better exploded-dimension extraction in SVG.
- Minimal review UI for non-technical users.

## 9) Performance strategy (keep it fast)

- Make everything incremental: small changes → quick revalidation.
- Cache expensive results:
  - normalized topology,
  - correspondence matches,
  - projection results,
  - boolean operands.
- Parallelize:
  - per-view analysis,
  - per-candidate validation.
- Early pruning:
  - reject candidates via cheap 2D tests before OCC booleans.

