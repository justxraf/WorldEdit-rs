# Brush Plan

Comprehensive, phased plan for bringing [`src/commands/brush.rs`](src/commands/brush.rs) closer to FAWE brush parity within Pumpkin's current block-only API limits.

## Scope and constraints

- [x] Audit current brush surface in `src/commands/brush.rs`, `src/pattern.rs`, and `src/commands/mod.rs`.
- [x] Confirm hard constraints: block-only edits through `World::set_block_states`, raycast targeting, and history entries.
- [x] Keep the "recognized but unsupported" pattern for brushes that need entities, biomes, images, expressions, CFI, or generation APIs.
- [ ] Audit FAWE command syntax and defaults brush-by-brush before implementing advanced parity work.

## Current baseline

- [x] `BrushKind` already covers `Sphere`, `Cylinder`, `Cuboid`, `Clipboard`, `Smooth`, `Gravity`, `Extinguish`, `Splatter`, `Raise`, `Morph`, and `Snow`.
- [x] `BrushKind::summary()`, `permission()`, `set_radius()`, and `set_material()` already exist for current brush families.
- [x] Parsing already supports `sphere`, `cylinder`, `set`, `clipboard`, `smooth`, `gravity`, `extinguish`, `splatter`, `raise`, `lower`, `erode`, `dilate`, `morph`, `snow`, `none`, `list`, `size`, `material`, `mask`, `range`, `tracemask`, and `vis`.
- [x] Current flags already handled: `-h` for hollow sphere/cylinder, `-a` and `-o` for clipboard, `-h <height>` for gravity, `-s` for snow.
- [x] Brush bindings already preserve mask and range across rebinds.
- [x] Current permission nodes already exist for implemented brushes and brush option commands.
- [x] Current brush apply paths already push history entries.
- [x] Brush bindings still need transform state, trace/target mask state, target mode, scroll action state, and visualization state.
- [ ] Parser still needs broader FAWE flag coverage and exact FAWE argument parity.

## Phase 1: Foundations

Priority: P0. Finish the brush framework before adding more brush families.

- [x] Keep the expanded `BrushKind`/summary/permission/setter surface for currently implemented brushes.
- [x] Add remaining `BrushKind` variants needed for planned block-capable brushes: `Flatten`, `Height`, `Heightmap`, `Overlay`, `Surface`, `BlendBall`, `Scatter`, `ScatterOverlay`, `ScatterCommand`, `Spline`, `SurfaceSpline`, `Sweep`, `Catenary`, `Shatter`, `Command`, and `PopulateSchematic`.
- [x] Keep existing parser coverage for current brush commands and settings.
- [x] Extend parsing for additional FAWE flags where Pumpkin can support them cleanly.
- [x] Add explicit parsing and storage for target modes `0..=3`.
- [x] Add `/br vis` state storage as a stub, even if rendering remains unavailable.
- [x] Add scroll action parsing/storage as a stub for size, range, and pattern switching.
- [x] Keep `BrushBinding` support for mask and range.
- [x] Extend `BrushBinding` with target mode, trace mask, target mask, transform settings, visualization mode, and scroll action.
- [x] Keep the generic `apply_pattern_positions` helper for shape-based block application.
- [x] Generalize position generation so new brushes can reuse shared scatter, noise, overlay, and surface-following helpers.
- [x] Keep current `worldedit.brush.*` permissions wired through `src/commands/mod.rs`.
- [x] Expand permission registration for every new brush and option node.
- [x] Keep history integration mandatory for all current brush implementations.
- [x] Add regression tests covering parser behavior, binding persistence, and history for new brush families.

## Phase 2: Core brushes

Priority: P1. Focus on brushes that fit Pumpkin's existing world/block model.

- [x] Sphere / Cylinder / Set / Cuboid: solid baseline exists.
- [x] Add missing variants and FAWE-style argument parity for the shape brushes.
- [x] Clipboard / Copypaste: basic clipboard paste brush with `-a` and `-o` exists.
- [x] Expand clipboard brush support with any feasible additional flags and clearer unsupported messaging for `-r` / full FAWE-only behavior.
- [x] Add scatter-style clipboard placement and `populate schematic` support if it can be backed by the existing clipboard/schematic code.
- [x] Smooth: base smoothing implementation exists.
- [x] Add dedicated `Flatten` behavior and refine smoothing options toward FAWE defaults.
- [x] Gravity / Extinguish: baseline implementations exist.
- [x] Splatter / Blob: splatter-style probabilistic placement exists.
- [x] Improve splatter/blob behavior with better seeded noise and density controls.
- [x] Raise / Lower / Erode / Dilate / Morph: baseline terrain sculpting exists.
- [x] Improve terrain tools to behave more like FAWE presets and document any intentional deviations.
- [x] Snow: baseline implementation exists.
- [x] Extend snow behavior for better layering parity and mask interactions.
- [x] Height / Heightmap: implement top-column terrain shaping using `top_solid_in_column` plus pattern support.

## Phase 3: Advanced brushes

Priority: P2. Implement only where the block API is sufficient; otherwise recognize and fail clearly.

- [x] Scatter: random pattern placement within a brush volume.
- [x] ScatterOverlay: scatter constrained to surface hits (one block above the surface).
- [ ] ScatterCommand: limited command execution at brush targets, gated behind explicit permission and safety checks. (Kept recognized-but-unsupported: Pumpkin's plugin API exposes no command-dispatch hook.)
- [x] SurfaceSpline / Spline / Sweep / Catenary: multi-click curve brushes with per-player temporary control point state.
- [x] Shatter: fracture terrain using seeded partitioning/noise (Voronoi cell boundaries over scattered surface seeds).
- [ ] Command brush: targeted command execution with strict allowlist or server permission gating. (Kept recognized-but-unsupported: no plugin command-dispatch hook.)
- [x] PopulateSchematic: scatter schematic or clipboard placements across valid surfaces (implemented during Phase 2).
- [x] Image brush: recognize syntax and return an unsupported message until image loading exists (`image` / `stencil`).
- [x] BlendBall / Overlay / Surface: implement surface-following block brushes where a solid-top-column model is enough.
- [x] Add recognized-but-unsupported handling for entity, biome, feature, and CFI brush families with precise reasons.

## Phase 4: Features and polish

Priority: P3. Improve parity, usability, and safety after the block-capable brush set is stable.

- [x] Reuse existing `BlockMask` and `BlockPattern` integration where possible.
- [ ] Expand brush-side use of FAWE-style patterns, including better `#simplex`-driven scatter and surface workflows.
- [ ] Add basic transform support that is realistic for Pumpkin's clipboard/block model.
- [ ] Add visualization stub behavior for `/br vis` and optional raycast preview if the client/server API allows it.
- [ ] Add scroll-action plumbing for wheel-based size/range/material changes.
- [ ] Update raycast logic to honor target modes consistently.
- [ ] Add persistent per-player brush save/load if plugin storage is available.
- [ ] Add performance guards: batch reuse, radius caps, shape-specific limits, and early exits for no-op brushes.
- [ ] Standardize unsupported messages so users know when a feature needs full FAWE-only subsystems.

## Implementation order

- [x] 1. Audit current implementation and constraints.
- [x] 2. Audit FAWE source/docs for exact brush names, aliases, defaults, and flags.
- [x] 3. Extend parser, literals, and binding state before adding new apply functions.
- [x] 4. Add shared helpers for targeting, surfaces, scatter, and noise-driven positions.
- [x] 5. Implement P1 block-capable brushes first: shape parity, clipboard expansion, flatten, terrain tools, height/heightmap.
- [x] 6. Implement P2 advanced block-capable brushes: scatter, overlay/surface, spline family, shatter, populate schematic.
- [x] 7. Add or expand tests for parsing, binding persistence, and apply behavior.
- [x] 8. Fill out permissions, usage text, and unsupported error messages.
- [x] 9. Add docstrings and command help text that match the supported FAWE subset accurately.

## Phase 1 audit notes

- [x] Audited the local `FastAsyncWorldEdit` `worldedit-core` implementation instead of external docs for faster parity checks.
- [x] Confirmed FAWE aliases/defaults currently mirrored in WorldEdit-rs Phase 1: `blendball`/`bb`/`blend`, `surface`/`surf`, `scattercommand`/`scattercmd`/`scmd`/`scommand`, `surfacespline`/`sspline`/`sspl`, `catenary`/`cat`/`gravityline`/`saggedline`, `populateschematic`/`populateschem`/`popschem`/`pschem`/`ps`, and `flatten`/`flat`/`flatmap`.
- [x] Confirmed FAWE terrain-family naming: `height` also covers `heightmap`, while `cliff` / `flatcylinder` are separate FAWE-facing terrain aliases. WorldEdit-rs now recognizes the `cliff` naming on the flatten path.
- [x] Confirmed FAWE tool-option naming differences that matter for Phase 1 help text: `target` exposes the same four target enum modes, `scroll` supports `none|clipboard|mask|pattern|target_offset|range|size|target`, and FAWE's `tracemask` aliases differ from the extra stub state WorldEdit-rs stores today.
- [x] Documented intentional Phase 1 deviations in command help: Pumpkin keeps `clipboard` as the supported paste brush while FAWE's `copypaste`, image-backed `stencil` / `image`, and other richer brush families stay recognized-but-unsupported until later phases.

## Phase 2 notes (completed 2026-06-12)

All behavior was audited against the local `FastAsyncWorldEdit` checkout (`worldedit-core/src/main/java/com/sk89q/worldedit/command/BrushCommands.java` plus the brush classes under `com/fastasyncworldedit/core/command/tool/brush/` and `com/sk89q/worldedit/command/tool/brush/`).

What was implemented:

- [x] Sphere: FAWE default radius 2, `-f` falling-sphere variant (`FallingSphere` column-settling port in `falling_sphere_positions`), flags accepted in any argument position via the shared `split_flags` helper.
- [x] Cylinder: FAWE default radius 2, fourth positional `thickness` argument for hollow cylinders, and hollow cylinders are now open-ended tubes (the old top/bottom caps did not match FAWE's `HollowCylinderBrush`).
- [x] Erode family: `erode` = FAWE `ErodeBrush(2, 1, 5, 1)` with the four optional face/iteration arguments, new `pull` alias = FAWE `RaiseBrush(6, 0, 1, 1)`, `dilate` = FAWE `MorphBrush(5, 1, 2, 1)` preset (radius-only), all mapped onto the existing `BrushKind::Morph` apply path.
- [x] Gravity: `-h` is a switch (scan from world bottom, FAWE `fullHeight`) instead of the old `-h <height>` argument; footprint is FAWE's square column area; column compaction extracted into testable `compact_column_states`.
- [x] Snow: real layer parity via the block-state registry — stacking increments `snow[layers=N]`, the 8th layer converts to `snow_block`, partial layer stacks refuse a new layer in non-stack mode, and `snowy=true` is applied to grass-like blocks beneath new snow. Degrades to no-op stacking when the embedded registry lacks per-state variants.
- [x] Smooth: FAWE default radius 2 (was 5).
- [x] PopulateSchematic: implemented. `#clipboard` (or `#copy`) uses the player clipboard including pending transform; any other source loads `<data folder>/schematics/<name>.schem`. Placement mirrors FAWE `Extent#addSchems`/`SchemGen`: one density%-gated attempt per chunk in the brush cuboid, pasted air-skipped with origin anchored one block above the masked surface hit, optional deterministic rotation with `-r`. The plugin data folder is captured at registration into the `DATA_FOLDER` thread-local.
- [x] `snowsmooth` got its own precise unsupported message (block-only smoothing exists; snow-layer-aware heightmap smoothing does not).
- [x] Tests: parser coverage for all of the above plus pure-helper tests for `compact_column_states`, `snow_layer_count`, `populate_chunk_attempt`, and capless hollow cylinders. Native-target run: 188 passed; the 5 remaining failures (mapping/pattern/transform state-variant tests) pre-exist on a clean tree because the Pumpkin-style `blocks.json` carries no per-state property data.

Intentional deviations from FAWE (also documented in the `brush.rs` module docs):

- Erode/pull/dilate use the 6-neighbor morph pass instead of FAWE's 4-face cardinal erosion, and erosion carves to air instead of the most common neighboring fluid.
- Gravity compacts columns fully (upstream WorldEdit behavior) rather than reproducing FAWE's gap-preserving `freeSpot = y + 1` quirk.
- Populate schematic, splatter, and clipboard random rotation derive randomness from position hashes, not `ThreadLocalRandom`, so repeated clicks are reproducible for undo/tests.
- Sphere does not auto-switch sand/gravel patterns to falling mode (FAWE prints a hint and forces `-f`); users pass `-f` explicitly.
- `pull` reuses the `worldedit.brush.morph` permission node instead of FAWE's `worldedit.brush.pull`.
- Populate schematic loads the schematic file on every brush use rather than caching at bind time.

## Phase 3 hand-off notes (for the next session)

Read these before starting Phase 3; the conversation that produced Phase 2 was cleared.

- FAWE reference source lives at `..\FastAsyncWorldEdit\worldedit-core\src\main\java\` (note: `Glob` may fail on that OneDrive tree; use PowerShell `Get-ChildItem -Recurse -Filter` instead).
- Existing shared helpers in `brush.rs` to reuse: `surface_hits_for_shape` / `top_solid_in_column` (masked surface scans), `select_spaced_positions` + `scatter_surface_hits` (deterministic spaced sampling — already exactly what Scatter needs), `position_hash` (seeded determinism), `apply_pattern_positions` / `push_change` / `commit_entry` (mask + gmask + history plumbing), `split_flags` (FAWE-style switches anywhere), and `crate::simplex_noise` for noise-backed brushes.
- Scatter / ScatterOverlay (`BrushKind::Scatter`/`ScatterOverlay`, parsed and stored already): FAWE `ScatterBrush` picks `points` surface positions at least `distance` apart inside the radius and applies the pattern at the surface block; the overlay variant places one block above the surface instead. Wire `scatter_surface_hits` to the apply path.
- BlendBall (`BrushKind::BlendBall`): FAWE `BlendBall` replaces each block in the sphere with the most common state among its 26 neighbors when the frequency difference is at least `min_frequency_diff`; `-a` only swaps air vs non-air. `most_common_state` exists for morph; extend to a 26-neighbor sample.
- Surface (`BrushKind::Surface`): FAWE `SurfaceSphereBrush` applies the pattern to existing surface blocks (blocks with air exposure) inside the sphere. Overlay (`BrushKind::Overlay`): place the pattern one block above masked top-column hits in the disc.
- Shatter (`BrushKind::Shatter`): FAWE `ShatterBrush` picks `count` seeded points on the surface and draws Voronoi cell boundaries (blocks whose nearest-seed differs from a neighbor's nearest-seed get the pattern). Keep seeds deterministic via `position_hash`.
- Spline family (`Spline`, `SurfaceSpline`, `Catenary`, `Sweep`): need per-player accumulated control points (add a `Vec<BlockPos>` to the per-player state next to `BRUSHES`; FAWE ends point collection when the same block is clicked twice). Catenary hangs a rope curve between two clicks; Sweep pastes the clipboard along the curve. Start with `Spline` and `Catenary`; `//line`-style block tracing helpers exist in `src/commands/generation.rs` (curve/line code) and may be reusable.
- Command / ScatterCommand brushes: first check whether `pumpkin_plugin_api` exposes any command-dispatch API for plugins (none was found during Phase 2 — if still absent, keep them recognized-but-unsupported with a precise reason instead of partially implementing).
- Image brush: keep recognized-but-unsupported (no image loading); `image` and `stencil` are not currently in the literal registration list — add them as recognized names with precise unsupported reasons.
- Block-state caveat: per-state property variants (e.g. `snow[layers=N]`, stair facings) resolve to default states with the current Pumpkin-style `blocks.json`; write apply paths so they degrade to no-ops rather than wrong blocks, and gate tests like `snow_layer_states_round_trip` does.
- Run tests with `cargo test --target x86_64-pc-windows-msvc` (the default target is `wasm32-wasip2`, whose test binary cannot execute on Windows). Expect exactly 5 pre-existing failures unrelated to brushes.

## Phase 3 notes (completed 2026-06-12)

Audited against the same local `FastAsyncWorldEdit` checkout as Phase 2. All new
apply paths reuse the shared helpers called out in the Phase 3 hand-off.

What was implemented (all in `src/commands/brush.rs`):

- [x] Scatter / ScatterOverlay: `apply_scatter` wires `scatter_surface_hits`
  (deterministic spaced sampling) to the surface scan. Scatter replaces the
  surface block; the `-o` overlay variant places the pattern one block above
  it. Honors brush mask + gmask and pushes history.
- [x] BlendBall (`apply_blendball`): samples the 26 neighbors, replaces the
  center with the most common neighbor state only when its frequency beats the
  center's by at least `min_frequency_diff`. `-a` collapses the vote to
  air-vs-solid. New helpers: `blendball_neighbor_states`, `most_common_with_count`.
- [x] Surface (`apply_surface`): applies the pattern to every air-exposed block
  in the sphere (`is_air_exposed` 6-face check). Overlay (`apply_overlay`):
  places the pattern one block above the top solid block of each column in the disc.
- [x] Shatter (`apply_shatter`): scatters `count` seeds on the surface, assigns
  each surface column to its nearest seed (Voronoi), and applies the pattern on
  columns that border a different cell (4-neighbor boundary test). Seeds are
  deterministic via `scatter_surface_hits` → `position_hash`.
- [x] Curve family (spline / surfacespline / catenary / sweep): multi-click
  control points live in `PlayerBrushes::control_points` keyed by the bound
  item. `trigger_curve_brush` intercepts these before `apply_brush`: each click
  adds a point; splines finalize when the same block is clicked twice (FAWE
  rule), catenary/sweep finalize on the second distinct click. Rebinding or
  unbinding clears the points. Spline uses a Catmull-Rom curve (`spline_curve`);
  surface spline projects to XZ and re-snaps Y to `top_solid_in_column`;
  catenary hangs a parabolic-approximation sag between two points
  (`catenary_curve`); sweep pastes the clipboard along the two-point line
  (`apply_sweep`). Local `line_block_samples` (3D Bresenham) keeps the module
  decoupled from `generation.rs`.
- [x] Command / ScatterCommand: confirmed `pumpkin-plugin-api` exposes no
  command-dispatch hook for plugins (only handler registration via
  `CommandNode::execute`). Kept recognized-but-unsupported with a precise
  reason naming the missing hook, in both the parser and the apply path.
- [x] Image / stencil: already recognized-but-unsupported (no image loading).
- [x] Tests: pure-helper coverage for `most_common_with_count`,
  `blendball_neighbor_states`, `line_block_samples`, `spline_curve` (incl. flat
  surface-spline mode and 2-point fallback), `catenary_curve` (sag + taut),
  `is_curve_brush` / `curve_required_points`, shatter Voronoi nearest-seed, and
  command-brush parsing. Native-target run (`cargo test --target
  x86_64-pc-windows-msvc`): 197 passed; the same 5 pre-existing failures the
  Phase 2 notes document (mapping/pattern/transform per-state-variant tests that
  need a mojang-style `blocks.json`) remain and are unrelated to brushes.

Intentional deviations from FAWE:

- Spline uses a uniform Catmull-Rom interpolation rather than FAWE's
  Kochanek–Bartels spline with tension/bias/continuity; the surface-spline TBC
  and `quality` arguments are still parsed and stored but not yet fed into the
  curve sampler.
- Catenary uses a parabolic sag approximation (`4·sag·t·(1−t)`) scaled by
  `length_factor` instead of solving the true hyperbolic-cosine catenary; the
  `-h` shell, `-s` select, and `-d` facing-direction flags are parsed/stored but
  not yet applied.
- Sweep spaces `copies` clipboard pastes evenly along the straight two-click
  line (no curved sweep path yet); `copies < 1` pastes once per block on the line.
- Surface/overlay/scatter/shatter derive any randomness from position hashes
  (not `ThreadLocalRandom`) so repeated clicks reproduce for undo/tests, matching
  the Phase 2 determinism convention.

## Phase 4 hand-off notes (for the next session)

- Remaining Phase 4 work is parity/usability polish, not new block brushes:
  feed surface-spline TBC + `quality` into a Kochanek–Bartels sampler, apply the
  catenary `-h/-s/-d` flags, add real visualization for `/br vis`, wire scroll
  actions to actual wheel events (if the event API exposes them — none was used
  yet), honor target modes 1–3 in the raycast (currently only mode 0 / block
  range is used by `entity.raycast`), and add persistent per-player brush
  save/load if plugin storage appears.
- The curve control-point store (`PlayerBrushes::control_points`) is per
  `(player name, item slot+id)`; it is cleared on rebind/unbind but NOT on
  player disconnect — if a leak matters later, clear it from a quit event.
- Command-dispatch is still the one hard blocker for command/scattercommand
  brushes; re-check `pumpkin-plugin-api` for a dispatch/run-command API before
  attempting them again.

## Notes for future passes

- [x] Keep unsupported brushes recognized instead of silently ignored.
- [x] Prefer deterministic seeded behavior for splatter, shatter, scatter, and noise-backed brushes so undo/tests stay predictable.
- [x] Avoid implementing misleading partial support for entity, biome, image, expression, or CFI brushes without explicit user-facing warnings.
