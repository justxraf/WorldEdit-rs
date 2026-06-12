# TODO: Region Manipulation Commands

## Missing Commands

- [ ] `//move [count] [direction] [leave] [-s] [-a] [-m mask]`
- [ ] `//stack [count] [direction] [-s] [-a] [-r] [-m mask]`
- [ ] `//overlay <pattern>`
- [ ] `//hollow [thickness] [pattern] [-m mask]`
- [ ] `//deform <expression>` (blocked - see Expression Pattern)
- [ ] `//regen [seed] [-r] [-b [biome]]` (blocked - see below)

## Current State

None of these exist. `src/selection.rs::Region` has `shifted`, which `//move`
and `//stack` can reuse for moving the selection itself.
`src/selection.rs::Direction` already has `from_yaw_pitch` (player-facing
direction), `opposite`, and `vector` - exactly what `//move`/`//stack` need
for their default "direction = where the player is facing" behavior.

## Why It Matters

`//move` and `//stack` are everyday WorldEdit commands (move a build, repeat a
fence/wall/staircase along an axis). `//overlay` is the standard way to lay
flowers/snow/paths on top of terrain. `//hollow` carves out the interior of a
selection, leaving a shell - useful for buildings.

## FAWE Reference Behavior

(from `RegionCommands.java`, for parity)

- **`//move [count] [direction] [replace] [-s] [-a] [-e] [-b] [-m mask]`**:
  displaces the selection's contents by `count` blocks (default `1`) along
  `direction` (default: the player's facing direction - reuse
  `Direction::from_yaw_pitch`). `-s` shifts the player's selection to the new
  location. `-a` skips copying air blocks (so the destination's existing
  blocks show through where the source was air). `-e`/`-b` copy
  entities/biomes (treat as unimplemented, matching `//copy -e`/`-b`). `-m
  <mask>` restricts which source blocks are moved.
- **`//stack [count] [direction] [-s] [-a] [-r] [-m mask]`**: repeats the
  selection's contents `count` times (default `1`) along `direction` (default
  forward/player-facing), with each copy offset by the *region's size* along
  that axis by default. `-r` switches to raw block-unit offsets instead of
  region-size multiples. `-s`/`-a`/`-m` behave as in `//move`.
- **`//overlay <pattern>`**: for each `(x, z)` column in the selection, finds
  the highest non-air block and places `pattern` one block above it.
- **`//hollow [thickness] [pattern] [-m mask]`**: thickness defaults to `0`
  (manhattan-distance shell thickness); replacement pattern defaults to air.
  **Confirmed against `EditSession.hollowOutRegion`/`recurseHollow`**: the
  algorithm is a 6-connected BFS flood-fill inward from the selection
  bounding box's six outer faces. A position stops the flood (and counts as
  "shell") if `mask.test(position)` is `true`; everywhere the flood reaches is
  "outside" and gets `pattern` applied in the final pass unless one of its
  6-neighbors is shell. **The default mask (no `-m`) is `SolidBlockMask`** -
  i.e. by default only solid blocks count as shell, and the flood propagates
  freely through air/liquid. `thickness = 0` does **not** mean "hollow
  nothing" - it still produces the ~1-block solid shell found by the
  single-pass flood fill; `thickness > 1` repeats an extra "peel" pass that
  pulls additional region positions into `outside` before the final shell
  check, thickening the kept shell. An optional `-m mask` replaces
  `SolidBlockMask` as the "what counts as shell" test.
- **`//naturalize`** and **`//smooth`** are covered in
  [terrain-and-radius-tools.md](terrain-and-radius-tools.md), not here.
- **`//deform <expression>`**: applies a coordinate-transform expression to
  each block position, with three selectable origin modes (raw world
  coordinates, placement position, or selection center). This is purely a
  consumer of the FAWE expression engine tracked in
  [expression-pattern.md](../patterns/expression-pattern.md) - don't duplicate
  that tracking here.
- **`//regen [seed] [-r] [-b [biome]]`**: regenerates the selection using the
  world generator, optionally with an overridden `seed` (`-r` picks a random
  seed), and optionally regenerates biomes too (`-b`, with an optional
  specific `biome` to force).

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).

- `//move`, `//stack`, `//overlay`, `//hollow`: fully implementable - pure
  `get-block-state-id`/`set-block-states` plus the existing clipboard
  capture/paste machinery.
- `//deform`: blocked on the FAWE expression engine
  ([expression-pattern.md](../patterns/expression-pattern.md)).
- `//regen`: **blocked**. `world.wit` has no access to the chunk
  generator/seed (no `regenerate`, `get-seed`, or generator-handle function),
  and biome *writes* are also unavailable (see
  [README.md](README.md#quick-reference-pumpkin-api-capability-summary)), which
  `-b` would additionally need. Document `//regen` as blocked pending a future
  Pumpkin API addition; do not attempt a partial implementation - producing
  "almost but not quite what generation would place" would be worse than not
  having the command.

## Implementation Notes

- [ ] `//move [count] [direction] [leave] [-s] [-a] [-m mask]`: capture the
      selection (like `//cut`), paste it at `region.shifted(direction *
      count)`, fill the original region with `leave` (default air, skipping
      positions also covered by the destination if source and destination
      overlap), and (with `-s`) shift the player's selection to the new
      region.
- [ ] `//stack [count] [direction] [-s] [-a] [-r] [-m mask]`: repeat the
      *current* region's contents `count` times along `direction`, each copy
      offset by the region's size along that axis (or by 1 block with `-r`).
      Implement as `count` sequential copy+paste operations reusing
      `clipboard::capture`/paste internals against a temporary buffer (do not
      touch the player's actual clipboard). With `-s`, expand the player's
      selection to cover all copies afterward.
- [ ] `//overlay <pattern>`: for each `(x, z)` column in the selection, scan
      downward from `region.max.y` for the highest non-air block whose Y is
      within the selection's Y range, and set `pattern` one block above it
      (only if that position is also within the selection).
- [ ] `//hollow [thickness] [pattern] [-m mask]`: port FAWE's flood-fill
      algorithm rather than a bounding-box-distance heuristic (the geometric
      shortcut diverges for non-cuboid/irregular "objects" inside the
      selection and for custom masks, which is `//hollow`'s actual point - see
      its javadoc: "Hollows out the object contained in this selection").
      Implementation: BFS from the bounding box's six outer faces inward
      through `region`; a position joins an `outside` set if `!shell_mask(pos)`
      (default `shell_mask = is_solid(state)`, override via `-m`), and its
      6-neighbors are queued next - a position where `shell_mask(pos)` is true
      stops the flood there without joining `outside`. For `thickness > 1`,
      repeat `thickness - 1` extra passes that add any `region` position with a
      neighbor already in `outside` to `outside` too (thickening the shell).
      Final pass: for every position in `region`, apply `pattern` (default
      air) unless at least one of its 6-neighbors is in `outside` (i.e. it's
      shell - skip it).
- [ ] Add tests: `//move`/`//stack` offset math for each `Direction` variant
      (reuse `src/selection.rs`'s existing direction parsing/vector helpers),
      `//overlay` column scanning with gaps/overhangs, and `//hollow` interior
      detection at `thickness = 0` and `1` on a small cuboid.
