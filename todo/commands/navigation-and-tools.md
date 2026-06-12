# TODO: Navigation Commands And Player Tools

## Missing Commands

- [ ] `//jumpto`
- [ ] `//thru`
- [ ] `//up <distance>`
- [ ] `//ascend [levels]`
- [ ] `//descend [levels]`
- [ ] `//ceil [clearance]`
- [ ] `//unstuck`
- [ ] `//tool <none|repl|farwand|tree|...> [args]`
- [ ] super pickaxe (`//,` / `//superpickaxe` toggle + single/area/recursive
      modes)

## Current State

None of these exist. `src/commands/wand.rs` and `src/commands/brush.rs`
already establish the pattern for binding behavior to player interactions
(left/right-click event handlers, per-player thread-local bindings), which
`//tool`/super pickaxe would reuse. `src/commands/pos.rs`'s `//hpos1`/
`//hpos2` already use `entity.raycast` with `HPOS_MAX_DISTANCE = 300.0`.

## Why It Matters

The `NavigationCommands` (`//jumpto`, `//thru`, `//up`, `//ascend`,
`//descend`, `//ceil`, `//unstuck`) are used constantly while building - quick
movement shortcuts. Super pickaxe and `//tool` bindings are WorldEdit's other
major "click to act" feature alongside brushes, and share almost all of the
brush dispatcher's plumbing.

## FAWE Reference Behavior

(from `NavigationCommands.java`, for parity)

- **`//unstuck`**: finds the nearest free (non-solid) position and teleports
  there. Failure message: "worldedit.unstuck.moved" (i.e. it always succeeds
  in practice, since a free position can always be found nearby).
- **`//ascend [levels]`** / **`//descend [levels]`**: repeatedly find the next
  platform up/down, `levels` times (default `1`). Failure:
  "worldedit.ascend.obstructed" / "worldedit.descend.obstructed" if no further
  platform exists.
- **`//ceil [clearance]`**: teleport to just below the ceiling above the
  player, leaving `clearance` blocks of headroom (default `0`). Failure:
  "worldedit.ceil.obstructed".
- **`//thru`**: passes the player through the wall directly ahead, with a
  **fixed 6-block search range**. Failure: "worldedit.thru.obstructed".
- **`//jumpto`**: traces the player's line of sight up to **300 blocks**
  (matches this plugin's existing `HPOS_MAX_DISTANCE`), finds the first solid
  block hit, then finds a free position at/above it and teleports there.
  Failure: "worldedit.jumpto.none".
- **`//up <distance>`**: teleports the player straight up by `distance`
  (required argument, no default), optionally placing a temporary glass
  platform if the destination would otherwise be air (`alwaysGlass` /
  fly-mode dependent). Failure: "worldedit.up.obstructed".

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
Fully implementable today:

- `player.teleport(position, yaw, pitch, world)` / `entity.teleport(pos,
  world_ref)` - every navigation command reduces to a teleport.
- `entity.raycast(max_distance, fluid_handling)` - already used for
  `//hpos1`/`//hpos2`; directly reusable for `//jumpto`/`//thru`.
- `world.get-block-state-id`/`get-block-state` - column scans for
  `//ascend`/`//descend`/`//ceil`/`//unstuck`.
- Interact event handlers already exist for brushes
  (`BrushInteractHandler`/`BrushBreakHandler` in `src/commands/brush.rs`) -
  `//tool`/super pickaxe follow the same registration shape.

## Implementation Notes

- [ ] `//jumpto`: raycast from the player's eye position (reuse the
      `entity.raycast(300.0, ...)` pattern from `pos.rs`); teleport to the hit
      block's position + face offset (stand on top of the targeted block). No
      hit within range -> "worldedit.jumpto.none"-equivalent error.
- [ ] `//thru`: raycast with a fixed **6-block** range to find the first solid
      block in view; continue scanning forward through solid blocks (bounded
      by the same 6-block range) until reaching the first non-solid position,
      teleport there. No wall found, or the far side is also solid within
      range -> error.
- [ ] `//up <distance>`: teleport the player straight up by `distance`.
      Simplest first pass: no platform placement, just teleport (let gravity
      apply if the destination is air) - document this as a simplification
      vs. FAWE's optional glass-platform behavior.
- [ ] `//ascend [levels]` / `//descend [levels]` (default `levels = 1`): scan
      the player's column upward/downward for the next "platform" (two air
      blocks above a solid block for ascend; a solid block below two air
      blocks for descend), repeat `levels` times, teleport. No further
      platform -> obstructed error.
- [ ] `//ceil [clearance]` (default `clearance = 0`): scan upward from the
      player to the first solid block, teleport to `clearance` blocks below
      it. No ceiling within `MAX_BUILD_Y` -> obstructed error.
- [ ] `//unstuck`: if the player's current position is inside a solid block,
      scan outward (small radius, then upward) for the nearest non-solid
      position and teleport there.
- [ ] `//tool none|repl|farwand|tree|...`: bind a tool to the player's
      currently-held item, mirroring `brush::BrushKind`'s
      bind/dispatch/per-player-thread-local-state pattern. Start with `none`
      (unbind) and `repl <pattern>` (right-click replaces the targeted block
      with `pattern`) - the simplest tools - before `farwand`/`tree` (tree
      placement needs sapling-growth simulation, likely out of scope without a
      "grow tree" API).
- [ ] Super pickaxe: `//,` toggles a per-player flag; while enabled, breaking a
      block with the bound tool breaks an entire connected region of the same
      block type (flood-fill, capped) instead of a single block. Area mode
      (`//,` with a radius) breaks all matching blocks in a cube around the
      broken block.
- [ ] Add tests for the pure scanning/classification helpers (platform
      detection for ascend/descend, solid-block detection for ceil/thru) by
      extracting them as functions over a small mock column.
