# TODO: Global Mask And Wider Mask Support

## Missing Features

- [ ] `//gmask [mask]` (global mask applied to *every* edit command/brush)
- [ ] `//count <mask>` - currently exact single-state match only
- [ ] `//replace <mask> <to>` - currently exact single-state match (or "all
      non-air") only
- [ ] Mask support on brushes (the `-m` brush setting is parsed but brushes
      don't consult `BlockMask` beyond what's already wired)

## Current State

`src/pattern.rs` already defines `BlockMask` (`States`, `Any`, `Not`,
`Existing`, `Air`) and `BlockMask::parse`, but rejects any `##tag`/`%`-bearing
mask. `//count` (`src/commands/count.rs`) does `before == target_state` only -
no `BlockMask` involved. `//replace` (`src/commands/replace.rs`)'s `from`
argument resolves to a single block via `mapping::resolve_block` - also no
`BlockMask`. There is no global mask state anywhere.

## Why It Matters

In real WorldEdit/FAWE, almost every region command's "what blocks does this
affect" is governed by a mask (a comma-separated list of blocks/tags/states,
optionally negated). `//gmask` layers an additional player-set mask on top of
*all* commands. Without `BlockMask` wired into `//count`/`//replace`/brushes,
and without `//gmask` at all, this plugin's masking story is far behind even
basic WorldEdit usage - e.g. `//replace stone,andesite glass` (replacing
*multiple* source blocks at once) currently doesn't work.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
Pure plugin-side logic - `world.set-block-states` takes whatever filtered list
of changes the plugin computes, so there's no Pumpkin API gap here. The
remaining `##tag`-mask gap (real data-pack tags) is tracked separately in
[block-tags.md](../patterns/block-tags.md) and not duplicated here -
`BlockMask` already has a best-effort tag fallback to build on.

## Implementation Notes

- [ ] `//count <mask>`: parse `mask` with `BlockMask::parse` (which already
      supports comma-separated `States`), iterate the selection, and report
      per-distinct-block-name counts plus a total (matching WorldEdit's
      `//count` output format of one line per matched block).
- [ ] `//replace <mask> <to>`: change the `from_or_to`/`to` two-arg path in
      `src/commands/replace.rs` to parse `from` as a `BlockMask` instead of a
      single `mapping::resolve_block` lookup, and use `mask.matches(before)`
      in `should_replace` instead of `before == from_id`. The one-arg
      "replace non-air" behavior is `BlockMask::Not(Box::new(BlockMask::Air))`
      and already representable.
- [ ] `//gmask [mask]` (alias `/gmask`): new command storing an
      `Option<BlockMask>` per player (thread-local, alongside
      `SELECTIONS`/`CLIPBOARDS`/`HISTORY` in their respective modules, or a new
      `mask` module). `//gmask` with no argument clears it.
- [ ] Thread the global mask through every edit path: `//set`, `//replace`,
      `//cut`'s leave-fill, `//paste`, shell commands, brushes, and the new
      commands from [shape-generation.md](shape-generation.md),
      [region-manipulation.md](region-manipulation.md), and
      [terrain-and-radius-tools.md](terrain-and-radius-tools.md). The natural
      integration point is wherever each command currently does
      `let before = world.get_block_state_id(pos);` - skip the position
      (don't add it to `changes`) if a global mask is set and
      `!mask.matches(before)`.
  - Consider a small shared helper in `src/commands/mod.rs` -
    `fn passes_gmask(key: &str, before: u16) -> bool` - so every command opts
    in with one extra `if` rather than re-deriving the lookup.
- [ ] Add tests: `BlockMask::parse` for comma-separated multi-block masks
      (already may partially work - verify), `//replace` with a multi-block
      `from`, and `passes_gmask` with/without a set mask.
