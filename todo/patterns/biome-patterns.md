# TODO: Biome Pattern

## Pattern

- [ ] `#biome <biome>`

## Why It Is Missing

Biome edits are not block-state edits. The current pattern output type is a
single block-state id, and history currently records block changes.

Pumpkin also currently exposes biome **reads** but not biome **writes**:
`pumpkin-plugin-wit/v0.1/world.wit` has `get-biome(pos) -> biome`, but there is
no `set-biome` host call. That makes this an upstream API blocker today, even
though FAWE supports `#biome` via a dedicated biome-pattern path
(`BiomePatternParser` -> `BiomeApplyingPattern` -> `Extent#setBiome(...)`) and
records biome undo/redo separately from block changes.

## Implementation Notes

- [x] Audit Pumpkin/FAWE biome support surface.
      Pumpkin: `world.get-biome` exists, `world.set-biome` does not.
      FAWE: `#biome` is parsed separately and applies with `setBiome(...)`,
      while history records biome changes independently of block changes.
- [ ] Expose biome writes from Pumpkin once a `world.set-biome` (or equivalent
      region biome write) exists upstream.
- [ ] Extend command/edit history to record biome changes.
- [x] Decide how biome resolution should handle namespaces and aliases.
      Use canonical Minecraft biome ids for user input. Accept
      `minecraft:plains` and bare `plains` as equivalent; do not expose
      Pumpkin's WIT enum spelling to users. If Pumpkin's generated enum names
      require kebab-case internally, translate that inside the resolver rather
      than making command users type it.
- [x] Decide which commands should accept biome patterns.
      Any command that currently consumes a pattern *to write into the world*
      should eventually accept biome patterns via a widened material/pattern
      enum: `//set`, `//replace`, `//shell`, `//cut`'s leave pattern, and the
      material-bearing `//brush` modes. Clipboard-only commands do not need
      `#biome`, and none of the above can safely enable it until biome writes
      and biome-aware undo/redo exist.
- [ ] Add tests for biome parsing, undo/redo, and mixed block/biome operations.

## Current Outcome

This cannot be implemented correctly in `WorldEdit-rs` yet because Pumpkin does
not expose a biome setter. The next real code step is upstream: add
`world.set-biome` (or equivalent batched biome write support), then widen this
plugin's pattern/history model from "block state only" to "block or biome
edit".
