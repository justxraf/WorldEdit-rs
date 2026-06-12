# TODO: FAWE Color Patterns

## Patterns

- [ ] `#color <r> <g> <b>`
- [ ] `#saturate <r> <g> <b> <a>`
- [ ] `#darken`
- [ ] `#anglecolor <distance>`
- [ ] `#desaturate <percent>`
- [ ] `#averagecolor <r> <g> <b> <a>`
- [ ] `#lighten`

## Why It Is Missing

These patterns need a palette that maps blocks to representative colors. Some
also depend on the existing block, opacity/material behavior, or terrain angle.

## Implementation Notes

- [ ] Choose a block color source: bundled palette, generated assets, or Pumpkin
      registry data.
- [ ] Implement nearest-color block lookup.
- [ ] Decide which block states are eligible for color replacement.
- [ ] Implement color transforms against the existing block color.
- [ ] Add terrain-angle sampling for `#anglecolor`.
- [ ] Add tests for deterministic color matching and unsupported/transparent
      blocks.

