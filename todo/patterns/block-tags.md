# TODO: Real Block Tag Patterns

## Patterns

- [x] `##<tag>`
- [x] `##*<tag>`

## Current State

The engine now resolves real generated block-tag membership from Pumpkin's tag
dump instead of guessing by block-name suffix. `##tag` uses each tag member's
default state, while `##*tag` expands those members to every known block state
from the embedded block registry.

## Why It Is Incomplete

WorldEdit and FAWE use the server's live Minecraft block tags, including
data-pack-provided tags. This pass embeds Pumpkin's generated tag dump at build
time, but it still does not read a runtime/custom tag registry from the server.

## Implementation Notes

- [x] Replace suffix matching with real generated tag membership.
- [ ] Find or expose Pumpkin's runtime block tag registry to plugins.
- [x] Preserve the distinction between default states and all states.
- [x] Support namespaced tags such as `minecraft:slabs`.
- [ ] Handle custom data-pack tags.
- [x] Add tests for vanilla tags.
- [ ] Add tests for custom tags.
- [x] Add tests for empty tags.
- [x] Add tests for `##*tag` state expansion.
