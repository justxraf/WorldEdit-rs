# TODO: Biome Pattern

## Pattern

- [ ] `#biome <biome>`

## Why It Is Missing

Biome edits are not block-state edits. The current pattern output type is a
single block-state id, and history currently records block changes.

## Implementation Notes

- [ ] Expose biome reads/writes from Pumpkin if available.
- [ ] Extend command/edit history to record biome changes.
- [ ] Decide how biome resolution should handle namespaces and aliases.
- [ ] Decide which commands should accept biome patterns.
- [ ] Add tests for biome parsing, undo/redo, and mixed block/biome operations.

