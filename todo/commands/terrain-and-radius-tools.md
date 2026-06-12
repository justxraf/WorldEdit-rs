# TODO: Terrain And Radius-Based Utility Commands

## Missing Commands

- [ ] `//smooth [iterations] [mask]`
- [ ] `//naturalize`
- [ ] `//green [radius] [-f]`
- [ ] `//snow [radius]`
- [ ] `//thaw [radius]`
- [ ] `//drain <radius>`
- [ ] `//fixwater <radius>` / `//fixlava <radius>`
- [ ] `//removeabove [size] [height]` / `//removebelow [size] [depth]`
- [ ] `//removenear <mask> [radius]`
- [ ] `//replacenear <radius> <from> <to>`

## Current State

None of these exist. The closest analog is the existing `Smooth`/`Splatter`
*brushes* in `src/commands/brush.rs`, which operate on a small radius around a
clicked point - the region-level (`//smooth`) and player-radius (`//drain`,
`//green`, etc.) versions don't share code with those yet.

## Why It Matters

These are FAWE's `UtilityCommands`/`RegionCommands` terrain-cleanup tools -
extremely common for quickly fixing or naturalizing generated/edited terrain
around the player or within a selection, without a full region edit.

## FAWE Reference Behavior

(from `UtilityCommands.java`/`RegionCommands.java`, for parity)

- **`//butcher`/`//remove`**: covered in
  [entity-and-biome-commands.md](entity-and-biome-commands.md), not here.
- **`//green [radius] [-f]`**: converts dirt to grass in a *cylindrical* area
  of the given radius centered on the player. Default radius is `10`. `-f`
  additionally converts coarse dirt.
- **`//fixwater <radius>` / `//fixlava <radius>`**: makes the targeted liquid
  *stationary* within `radius` - flattens flowing/irregular fluid surfaces to
  a calm, source-block state rather than draining it.
- **`//removeabove [size] [height]` / `//removebelow [size] [depth]`**: clears
  a `size`x`size` column (default size `1`) centered on the player, from the
  player's position up/down by `height`/`depth` blocks (default: the rest of
  the world's height span in that direction).
- **`//removenear <mask> [radius]`**: removes blocks matching `mask` within a
  square radius (default `50`) of the player; FAWE narrows the working region
  to the mask's bounds first for performance.
- **`//replacenear <radius> <from> <to>`**: `//replace`-equivalent restricted
  to a cuboid of the given radius around the player. If `from` is omitted,
  defaults to matching whatever block is already present at each position
  (i.e. behaves like `//set` within the radius) - confirm this default
  precisely before implementing, since it differs from `//replace`'s "all
  non-air" default.
- **`//naturalize`**: re-layers each column - the surface becomes grass, the
  next several layers (FAWE uses 3) become dirt, and everything below becomes
  stone.
- **`//smooth [iterations] [mask]`**: a **2D heightmap** Gaussian-kernel
  convolution over `iterations` passes (default `1`) - it smooths the terrain
  *surface height* per column, not a full 3D block-by-block average. This is
  notably simpler than a 3D smoothing brush.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
All fully implementable with `world.get-block-state-id`/`set-block-states`.
Two host functions help significantly:

- `world.get-top-block-y(x, z)` / `get-motion-blocking-height(x, z)` - highest
  non-air / motion-blocking column height, useful for `//naturalize`,
  `//green`, `//snow`, `//thaw`, and `//smooth`'s heightmap to find/adjust the
  surface without scanning every Y.
- For player-radius commands (`//drain`, `//fixwater`, `//fixlava`,
  `//green`, `//snow`, `//thaw`, `//removenear`, `//replacenear`), iterate a
  bounding cube/cylinder of the given radius around the player (via
  `sender_block_pos`) - a flood-fill is more faithful to FAWE for
  `//drain`/`//fixwater`/`//fixlava` (they only affect *connected* liquid) but
  a bounded-region first pass is reasonable and avoids unbounded BFS cost in
  wasm.

## Implementation Notes

- [ ] `//smooth [iterations] [mask]`: build a per-column heightmap (via
      `get-top-block-y` or a scan), apply a Gaussian-kernel convolution over
      neighboring columns for `iterations` passes, then re-fill each column to
      match its smoothed height using the dominant block type from that
      column's original makeup. Needs a one-column halo outside the
      selection for the convolution - read but don't write those columns
      (`region.expanded(1)` for reads only). Optional `mask` restricts which
      blocks are eligible to be replaced during re-fill.
- [ ] `//naturalize`: for each column in the selection, replace the top layer
      with grass-like cover, the next 3 layers with dirt, and the rest with
      stone (match FAWE's exact layer count/blocks). Use `get-top-block-y` to
      find each column's surface.
- [ ] `//green [radius] [-f]`: within a cylindrical `radius` (default `10`) of
      the player, convert `minecraft:dirt` (and, with `-f`,
      `minecraft:coarse_dirt`) to `minecraft:grass_block` where the column
      above is open to the sky - check `world.get-sky-light(pos_above) > 0` as
      a cheap "open to sky" proxy, or confirm via `get-top-block-y`.
- [ ] `//snow [radius]`: within `radius`, for columns whose top block is solid
      and exposed, place `minecraft:snow` (layer) above it and set the top
      block's `snowy=true` property where applicable (reuse
      `apply_state_properties` from `src/mapping.rs`).
- [ ] `//thaw [radius]`: inverse of `//snow`, plus convert
      `minecraft:ice`/`minecraft:frosted_ice`/`minecraft:snow_block`/snow
      layers back to `water`/`air` as FAWE does.
- [ ] `//drain <radius>`: flood-fill (BFS, capped) from the player's position
      over connected water/lava/ice/seagrass-family blocks within `radius`,
      replacing them with air.
- [ ] `//fixwater`/`//fixlava <radius>`: flood-fill connected water/lava
      within `radius` and rewrite flowing-liquid states to the stationary
      source-block state (level `0`), flattening the surface to a single
      height.
- [ ] `//removeabove`/`//removebelow [size] [height|depth]`: clear a
      `size`x`size` column (default `1`) centered on the player from their
      position up/down `height`/`depth` blocks (default: rest of the world's
      Y range) to air.
- [ ] `//removenear <mask> [radius]` (default radius `50`): within `radius`,
      set any block matching `BlockMask` (see
      [mask-coverage-and-global-mask.md](mask-coverage-and-global-mask.md)) to
      air.
- [ ] `//replacenear <radius> <from> <to>`: `//replace`'s logic restricted to a
      player-centered cube of side `2*radius+1` - confirm the no-`from`
      default against real FAWE before shipping.
- [ ] All of these should push a single `EditEntry` to `history` like existing
      commands, and use `batch_size()`/`block_flags()` for consistency.
- [ ] Add tests for the pure math/classification helpers (fluid-state
      detection, snow/grass eligibility, naturalize layer assignment, sphere/
      cylinder/column membership) - world-dependent behavior (sky light,
      top-block) can't be unit tested but should be isolated behind small
      functions that take the relevant inputs as parameters.
