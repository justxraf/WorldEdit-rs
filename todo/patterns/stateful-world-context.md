# TODO: Stateful And World-Context Patterns

## Patterns

- [ ] `#buffer <pattern>`
- [ ] `#buffer2d <pattern>`
- [ ] `#relative`
- [ ] `#surfacespread <distance> <pattern>`
- [ ] `#solidspread <dx> <dy> <dz> <pattern>`

## Why It Is Missing

These patterns need operation-level state or world queries beyond the current
single-block evaluation API.

## Implementation Notes

- [ ] Add an operation-scoped pattern context that can persist state across
      calls.
- [ ] Track exact positions for `#buffer`.
- [ ] Track columns for `#buffer2d`.
- [ ] Add a stable origin or clicked-position concept for `#relative`.
- [ ] Add world surface checks for `#surfacespread`.
- [ ] Add solid-block probing for `#solidspread`.
- [ ] Make history and batching preserve deterministic behavior.
- [ ] Add tests for repeated positions, repeated columns, origin changes, and
      world-boundary behavior.

