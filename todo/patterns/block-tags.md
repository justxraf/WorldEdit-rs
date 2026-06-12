# TODO: Real Block Tag Patterns

## Patterns

- [ ] `##<tag>`
- [ ] `##*<tag>`

## Current State

The engine has best-effort support based on generated registry names. For
example, `##slabs` can match block names ending in `_slab` or `_slabs`.

## Why It Is Incomplete

WorldEdit and FAWE use Minecraft block tags, including data-pack-provided tags.
Name suffix matching cannot faithfully represent those tag sets.

## Implementation Notes

- [ ] Find or expose Pumpkin's block tag registry to plugins.
- [ ] Preserve the distinction between default states and all states.
- [ ] Support namespaced tags such as `minecraft:slabs`.
- [ ] Handle custom data-pack tags.
- [ ] Add tests for vanilla tags, custom tags, empty tags, and `##*tag` state
      expansion.

