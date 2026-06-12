# TODO: FAWE Color Patterns

## Patterns

- [x] `#color <r> <g> <b>`
- [x] `#saturate <r> <g> <b> <a>`
- [x] `#darken`
- [ ] `#anglecolor <distance>`
- [x] `#desaturate <percent>`
- [x] `#averagecolor <r> <g> <b> <a>`
- [x] `#lighten`

## Why It Is Missing

The non-angle variants now use a bundled palette derived from Pumpkin's
`blocks.json` `map_color` metadata, with FAWE-style color transforms applied to
the existing block. `#anglecolor` is still blocked on terrain-angle sampling.

This is intentionally an approximation of FAWE's texture-based matcher: it is
deterministic and fast, but it does not yet read client textures or biome-tinted
grass colors.

## Implementation Notes

- [x] Choose a block color source: bundled palette, generated assets, or Pumpkin
      registry data.
- [x] Implement nearest-color block lookup.
- [x] Decide which block states are eligible for color replacement.
- [x] Implement color transforms against the existing block color.
- [ ] Add terrain-angle sampling for `#anglecolor`.
- [x] Add tests for deterministic color matching and unsupported/transparent
      blocks.
