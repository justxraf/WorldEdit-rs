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
- [ ] Add missing variants and FAWE-style argument parity for the shape brushes.
- [x] Clipboard / Copypaste: basic clipboard paste brush with `-a` and `-o` exists.
- [x] Expand clipboard brush support with any feasible additional flags and clearer unsupported messaging for `-r` / full FAWE-only behavior.
- [ ] Add scatter-style clipboard placement and `populate schematic` support if it can be backed by the existing clipboard/schematic code.
- [x] Smooth: base smoothing implementation exists.
- [ ] Add dedicated `Flatten` behavior and refine smoothing options toward FAWE defaults.
- [x] Gravity / Extinguish: baseline implementations exist.
- [x] Splatter / Blob: splatter-style probabilistic placement exists.
- [x] Improve splatter/blob behavior with better seeded noise and density controls.
- [x] Raise / Lower / Erode / Dilate / Morph: baseline terrain sculpting exists.
- [ ] Improve terrain tools to behave more like FAWE presets and document any intentional deviations.
- [x] Snow: baseline implementation exists.
- [ ] Extend snow behavior for better layering parity and mask interactions.
- [x] Height / Heightmap: implement top-column terrain shaping using `top_solid_in_column` plus pattern support.

## Phase 3: Advanced brushes

Priority: P2. Implement only where the block API is sufficient; otherwise recognize and fail clearly.

- [ ] Scatter: random pattern placement within a brush volume.
- [ ] ScatterOverlay: scatter constrained to surface hits.
- [ ] ScatterCommand: limited command execution at brush targets, gated behind explicit permission and safety checks.
- [ ] SurfaceSpline / Spline / Sweep / Catenary: multi-click curve brushes with per-player temporary control point state.
- [ ] Shatter: fracture terrain using seeded partitioning/noise.
- [ ] Command brush: targeted command execution with strict allowlist or server permission gating.
- [ ] PopulateSchematic: scatter schematic or clipboard placements across valid surfaces.
- [ ] Image brush: recognize syntax and return an unsupported message until image loading exists.
- [ ] BlendBall / Overlay / Surface: implement surface-following block brushes where a solid-top-column model is enough.
- [ ] Add recognized-but-unsupported handling for entity, biome, feature, and CFI brush families with precise reasons.

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
- [ ] 2. Audit FAWE source/docs for exact brush names, aliases, defaults, and flags.
- [x] 3. Extend parser, literals, and binding state before adding new apply functions.
- [x] 4. Add shared helpers for targeting, surfaces, scatter, and noise-driven positions.
- [ ] 5. Implement P1 block-capable brushes first: shape parity, clipboard expansion, flatten, terrain tools, height/heightmap.
- [ ] 6. Implement P2 advanced block-capable brushes: scatter, overlay/surface, spline family, shatter, populate schematic.
- [x] 7. Add or expand tests for parsing, binding persistence, and apply behavior.
- [x] 8. Fill out permissions, usage text, and unsupported error messages.
- [ ] 9. Add docstrings and command help text that match the supported FAWE subset accurately.

## Notes for future passes

- [ ] Keep unsupported brushes recognized instead of silently ignored.
- [ ] Prefer deterministic seeded behavior for splatter, shatter, scatter, and noise-backed brushes so undo/tests stay predictable.
- [ ] Avoid implementing misleading partial support for entity, biome, image, expression, or CFI brushes without explicit user-facing warnings.
