# TODO: Clipboard Patterns

## Patterns

- [x] `#clipboard`
- [x] `#copy`
- [x] `#fullcopy`
- [x] Clipboard offset syntax such as `#clipboard@[x,y,z]`

## Why It Is Missing

These patterns needed access to the player's clipboard and pattern-local
coordinate space. The pattern engine now carries a small evaluation context for
clipboard-backed patterns.

## Implementation Notes

- [x] Add a pattern evaluation context that can expose the player's clipboard.
- [x] Decide how clipboard origin, paste origin, and pattern origin are represented.
- [x] Support repeating clipboard blocks across larger regions.
- [x] Support explicit clipboard offsets.
- [x] Decide whether `#copy` should alias the current in-memory clipboard or a
      FAWE-style source clipboard concept.
- [x] Implement `#fullcopy` only after normal clipboard patterns can preserve
      enough block data and placement context.
- [x] Add tests for offset alignment, repeating behavior, air handling, and empty
      clipboards.

## Notes

- `#clipboard`, `#copy`, and `#fullcopy` all currently read from the player's
  current in-memory clipboard.
- Pattern tiling is anchored to the clipboard bounds min corner; `@[x,y,z]`
  offsets are relative to that min corner.
- `#fullcopy` currently preserves everything stored in this plugin's clipboard,
  which is block-state data only. Richer block entity/NBT fidelity remains
  tracked in `block-data-syntax.md`.
