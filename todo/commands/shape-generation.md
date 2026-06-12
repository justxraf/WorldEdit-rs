# TODO: Shape Generation Commands

## Missing Commands

- [ ] `//sphere <pattern> <radius>[,<radius-y>,<radius-z>] [-r] [-h]`
- [ ] `//hsphere <pattern> <radius>[,...] [-r]` (hollow sphere)
- [ ] `//cyl <pattern> <radius>[,<radius-z>] [height] [-h]`
- [ ] `//hcyl <pattern> <radius>[,...] [height] [thickness]` (hollow cylinder)
- [ ] `//pyramid <pattern> <size> [-h]` (alias `//hpyramid` for hollow)
- [ ] `//cone <pattern> <radii>[,<radius-z>] [height] [-h] [thickness]`
- [ ] `//line <pattern> [thickness]` (draws a line between `pos1` and `pos2`)
- [ ] `//curve <pattern> [thickness]` (draws a curve through a `convex`
      selection's points)

## Current State

None of these exist. `src/commands/mod.rs` has no `generation` module.
`//set` is the only "fill" command, and it always fills the full selection
bounding box.

## Why It Matters

These are among the most-used WorldEdit commands (quick terrain features,
domes, pillars, hills). They're also a good first addition because, unlike
most other gaps, they don't require a pre-existing selection - WorldEdit
centers them on the placement position (the player's position by default, or
`pos1` after `//toggleplace` - see
[navigation-and-tools.md](navigation-and-tools.md)), or `pos1`/`pos2` for
`//line`/`//curve`, and *creates* a region afterward.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).cla
Fully implementable today. All of these reduce to "for each `BlockPos` in a
computed shape, evaluate a `BlockPattern` and call `world.set-block-states`" -
exactly the machinery `src/commands/set.rs` and `src/commands/shell.rs`
already use.

## FAWE Reference Behavior

(from `GenerationCommands.java`, for parity)

- **Radii**: accept either one value (used for all axes) or three
  comma-separated values for `//sphere`/`//hsphere` (N/S, U/D, E/W radii -
  i.e. an ellipsoid), or one/two for `//cyl`/`//hcyl` (N/S radius, E/W
  radius). Minimum enforced radius is `0` for spheres and `1` for cylinders.
- **`-r`**: raise the sphere/cylinder so its bottom sits at the placement
  position instead of being centered on it.
- **`-h`**: hollow variant (shared switch with the dedicated `//hsphere`/
  `//hcyl`/`//hpyramid` commands - implement the switch first, then alias the
  `h`-prefixed command names to "same command + `-h`").
- **`//hcyl`**: takes an additional `thickness` argument (default `0`) for
  wall thickness.
- **`//pyramid`**: `<pattern> <size>`, with `-h` for hollow.
- **`//cone <pattern> <radii> [height] [-h] [thickness]`** (from
  `GenerationCommands.cone`/`EditSession.makeCone`): `radii` uses the same
  one-or-two-comma-separated-values syntax as `//cyl` (N/S radius, E/W radius,
  each clamped to a minimum of `1`). `height` defaults to `1`; a negative
  height builds the cone downward from the placement position instead of
  upward. `-h` makes it hollow with wall `thickness` (default `1`).
  Membership at layer `y` (0 = base): the horizontal ellipse radii shrink
  linearly from `(radiusX, radiusZ)` at the base to `0` at the apex (layer
  `height - 1`) - the same linear taper as `//pyramid`, but with `//cyl`'s
  elliptical cross-section instead of a square one.
- All generation commands enforce a maximum radius/size (server-configurable
  in FAWE) and can trigger the player's "unstuck" placement if they'd
  otherwise be generated inside the player.

## Implementation Notes

- [ ] New `src/commands/generation.rs` module registering all of the above,
      following `set.rs`'s pattern-parse -> validate -> batch -> history flow.
- [ ] Shape membership math:
  - Sphere/ellipsoid: `(dx/rx)^2 + (dy/ry)^2 + (dz/rz)^2 <= 1` over the
    bounding box `center +/- radius`.
  - Cylinder: `(dx/rx)^2 + (dz/rz)^2 <= 1` and `0 <= dy < height`.
  - Pyramid: at height `dy`, the horizontal extent shrinks linearly from
    `size` at the base to `0` at the top.
  - Match FAWE's rounding/edge-inclusion behavior so a given radius produces
    the same block count as real WorldEdit (write tests against known FAWE
    outputs for small radii, e.g. radius 1, 2, 3).
- [ ] Hollow variants (`//hsphere`, `//hcyl` with `thickness`, `//hpyramid`):
      a position is part of the shell if it's inside the shape at the given
      radius/size but *not* inside the shape at `radius - thickness` (default
      `thickness = 1` for sphere, configurable for cylinder).
- [ ] `[raised]`/`-r`: shift the shape's vertical center so its bottom is at
      the target Y instead of the center being at the target Y.
- [ ] `//cone`: reuse the cylinder's elliptical cross-section test
      `(dx/rx)^2 + (dz/rz)^2 <= 1`, but scale `rx`/`rz` by `(1 - y/height)` at
      each layer `y` (the same linear-taper formula as `//pyramid`, applied to
      an ellipse instead of a square). Negative `height` builds downward from
      the placement position. Hollow (`-h`) follows the same "in shape at the
      full radius but not at `radius - thickness`" rule as `//hsphere`/
      `//hcyl`, default `thickness = 1`.
- [ ] `//line`/`//curve`: rasterize a 3D line (DDA/Bresenham-style walk)
      between `pos1`/`pos2` (line) or through each consecutive pair of
      `convex` selection points (curve, from
      [selection-shapes.md](selection-shapes.md)), with `thickness` expanding
      each sample point into a small sphere of that radius.
- [ ] All shapes must respect `MIN_BUILD_Y`/`MAX_BUILD_Y` (currently defined
      in `src/commands/transform.rs` - consider moving these to a shared
      `src/commands/mod.rs` constant since multiple modules will need them).
- [ ] After generation, set the player's selection to the generated shape's
      bounding box (matches FAWE and lets `//undo`/`//count`/`//size` work
      naturally on the result).
- [ ] Add tests for shape membership functions (sphere/cylinder/pyramid) at
      small radii with known expected block counts, and for hollow-shell
      membership at `thickness = 1` and `> 1`.
