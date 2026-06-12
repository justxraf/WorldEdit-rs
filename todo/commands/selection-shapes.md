# TODO: Non-Cuboid Selections

## Missing Selection Types

- [ ] `//sel sphere` (radius set by a second click, like FAWE's `ExtendingCuboidRegionSelector`-style sphere)
- [ ] `//sel cyl` (cylinder, radius + height set by clicks)
- [ ] `//sel poly` (2D polygon extruded through a Y range)
- [ ] `//sel convex` (convex polyhedron hull from clicked points)
- [ ] `//sel ellipsoid`

## Current State

`src/commands/sel.rs` only accepts `cuboid`/`cube` and rejects every other
selector name with: "Selection type '{other}' is not supported yet; only
cuboid is available." `src/selection.rs`'s `Selection` is `{pos1, pos2}` and
`Region` is an axis-aligned box (`min`, `max`) with `volume`, `expanded`,
`contracted`, `shifted`, and `for_each_batch` all assuming a solid cuboid.

## Why It Matters

Every region command (`//set`, `//replace`, `//count`, `//walls`/`//faces`,
`//expand`/`//contract`/etc.) is written against `require_selection` ->
`Region`. Without non-cuboid selections, none of WorldEdit's
sphere/cylinder/polygon selection workflows are possible, and `//curve`
([shape-generation.md](shape-generation.md)) has no point-list selection to
draw through.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
This is a pure data/iteration problem - no Pumpkin API is needed.
`world.set-block-states` already takes an arbitrary `list<block-change>`, so
any shape can be expressed as a filtered set of positions within a bounding
box.

## Implementation Notes

- [ ] Turn `Selection`/`Region` into an enum (`Cuboid`, `Sphere`, `Cylinder`,
      `Polygon2d`, `Convex`, `Ellipsoid`), each exposing:
  - A bounding-box `Region` (cuboid `min`/`max`) for `for_each_batch`'s
    batching/memory behavior, unchanged from today.
  - A `contains(BlockPos) -> bool` test that every command applies inside the
    batch loop (skip positions where `contains` is `false`). For `Cuboid`,
    `contains` is always `true`, so existing behavior is preserved exactly.
- [ ] `//sel sphere`/`//sel cyl`: center on `pos1` (first click), radius (and
      height for cylinder) from the distance to `pos2` (second click) -
      matches FAWE's two-click sphere/cylinder selectors.
- [ ] `//sel poly [points]`: 2D polygon defined by repeated clicks (each
      `//pos1`-like click after the first appends a point instead of
      replacing `pos1`). **Y-extent (confirmed against
      `Polygonal2DRegionSelector`)**: NOT `MIN_BUILD_Y`/`MAX_BUILD_Y` - every
      clicked point calls `region.expandY(point.y)`, so the region's `min.y`/
      `max.y` track the lowest/highest Y among all clicked points so far and
      grow dynamically as more points are added. `contains` is a 2D
      point-in-polygon test on `(x, z)` AND'd with `min.y <= y <= max.y`.
- [ ] `//sel convex`: convex polyhedron hull from clicked points; `contains`
      needs a half-space test per hull face. This is the most complex shape -
      implement it last, and reuse the point list for `//curve`
      ([shape-generation.md](shape-generation.md)).
- [ ] `//sel ellipsoid`: like sphere but with independent per-axis radii (set
      via a second click defining a corner of the bounding box, similar to
      FAWE's ellipsoid selector).
- [ ] Update `//size`, `//count`, and `//expand`/`//contract`/`//shift`/
      `//outset`/`//inset` (`src/commands/transform.rs`) to either:
  - Report the shape's actual volume (sum of `contains` hits, not bounding-box
    volume) for `//size`/`//count`, or
  - Restrict `//expand`/`//contract`/etc. to cuboid selections with a clear
    error for other shapes (these commands resize a *box*, which doesn't
    translate cleanly to spheres/polygons).
- [ ] Add tests per shape: volume calculation against a known small case,
      `contains` boundary cases (on-sphere-surface, cylinder cap edge, polygon
      vertex/edge), and bounding-box correctness.
