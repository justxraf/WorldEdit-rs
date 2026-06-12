# TODO: History And Session Improvements

## Missing/Incomplete Capabilities

- [ ] Byte/block-count-based history caps (not just entry-count caps)
- [ ] `//undo`/`//redo [times] [player]` applying to the *target* player's
      world, not the invoker's
- [ ] Explicit, documented decision on cross-restart persistence
- [ ] `//history` (list recent operations) - optional, lower priority

## Current State

`src/history.rs` keeps `HISTORY: HashMap<String, PlayerHistory{undo, redo}>`
thread-local, capped at `MAX_HISTORY_ENTRIES = 32` *entries* regardless of how
many blocks each entry touches. The module doc already flags this: "real
WorldEdit/FAWE keeps history per-session and can spill large change sets to
disk... This implementation is purely in-memory and caps the number of
*entries* (not blocks)." `//undo [times] [player]`/`//redo [times] [player]`
(`src/commands/undo.rs`/`redo.rs`) can target another player's history stack by
name, but always apply via the *invoking* player's current world - so undoing
another player's edit made in a different world applies the inverse change in
the **wrong** world.

## Why It Matters

A 32-entry cap means a session of many small edits (e.g. repeated `//set`
inside a loop, or brush strokes) exhausts undo history quickly even though the
total block count is small - while a single `//set` over a huge region counts
as "1 entry" even if it's massive. FAWE's actual limits are memory/disk-based.
The cross-player/cross-world undo bug is a correctness issue: an admin running
`//undo 1 SomePlayer` while in a different world than `SomePlayer` will corrupt
that world instead of `SomePlayer`'s.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
Pure plugin-side bookkeeping - no Pumpkin API gap. The cross-world fix needs
`world.wit`'s `%world` resource to be stored per `EditEntry` (or per
`PlayerHistory` entry) so undo/redo can call `set-block-states` on the
*recorded* world rather than `require_selection`'s current-sender world.
Pumpkin's `%world` resource handles should be storable in thread-local state
(same as how `selection`/`clipboard` already store plain data per player) -
confirm `world` resource handles remain valid across command invocations (if
they're invalidated/dropped between calls, store the world's `get-id()` string
and re-resolve via a server-provided world lookup instead).

## Implementation Notes

- [ ] Record the source `%world` (or its id string) on each `EditEntry`/
      `PlayerHistory` entry; `undo`/`redo` apply `set-block-states` against
      that recorded world.
- [ ] Replace (or augment) `MAX_HISTORY_ENTRIES` with a total-changed-blocks
      budget (e.g. cap total `changes.len()` summed across all entries at some
      configurable number), evicting oldest entries once exceeded - closer to
      FAWE's memory-based limits than a flat entry count.
- [ ] Decide on and document the cross-restart story explicitly: WorldEdit-rs
      history is per-process/in-memory and is lost on plugin reload/server
      restart. This is likely *fine* (FAWE's disk-based history is largely for
      very large operations, not crash recovery) but should be a stated
      decision, not an implicit gap.
- [ ] `//history` (optional, lower priority): list each `PlayerHistory` entry
      with a summary (block count, timestamp, command name if tracked).
      Requires adding a description/timestamp field to `EditEntry`.
- [ ] Add tests: undo/redo against a recorded world id that differs from the
      "current" world (mock two world handles), and budget-based eviction with
      mixed small/large entries.
