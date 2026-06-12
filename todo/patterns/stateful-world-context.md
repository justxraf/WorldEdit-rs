# TODO: Stateful And World-Context Patterns

## Patterns

- [x] `#buffer <pattern>`
- [x] `#buffer2d <pattern>`
- [x] `#relative`
- [x] `#surfacespread <distance> <pattern>`
- [x] `#solidspread <dx> <dy> <dz> <pattern>`

## Why It Is Missing

These patterns need operation-level state or world queries beyond the current
single-block evaluation API.

## Implementation Notes

- [x] Add an operation-scoped pattern context that can persist state across
      calls.
- [x] Track exact positions for `#buffer`.
- [x] Track columns for `#buffer2d`.
- [x] Add a stable origin or clicked-position concept for `#relative`.
- [x] Add world surface checks for `#surfacespread`.
- [x] Add solid-block probing for `#solidspread`.
- [x] Make history and batching preserve deterministic behavior.
- [x] Add tests for repeated positions, repeated columns, origin changes, and
      world-boundary behavior.
