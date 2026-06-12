# TODO: Entity And Biome Commands

## Missing Commands

- [ ] `//butcher [radius] [flags]`
- [ ] `//remove <type> [radius]`
- [ ] `//setbiome <biome> [-p]` (blocked - see below)
- [ ] `#biome <biome>` pattern (blocked - tracked in
      [biome-patterns.md](../patterns/biome-patterns.md); not duplicated here)

## Current State

None of these exist. `src/commands/brush.rs` explicitly rejects brushes that
"need entities, biomes, generation features... that this plugin cannot access
yet" for `forest`, `butcher`, `kill`, `paint`, `biome`, etc.

## Why It Matters

`//butcher` (clear mobs) and `//remove` (clear non-living entities like item
frames, paintings, boats, XP orbs) are extremely common server-maintenance
commands. `//setbiome` and `#biome` are core terraforming tools in
WorldEdit/FAWE.

## FAWE Reference Behavior

(from `UtilityCommands.java`, for parity) `//butcher`'s flags classify
entities into categories: `FRIENDLY`, `PETS`, `NPCS`, `GOLEMS`, `ANIMALS`,
`AMBIENT`, `TAGGED`, `ARMOR_STAND`, `WATER`. The `-f` flag is shorthand for all
"friendly" categories at once. Default radius comes from server config; a
configurable `MAX_BUTCHER_RADIUS` caps it. `//remove <type> [radius]` uses a
similar entity-type classification to pick which non-living entities to
remove.

## API Capability - Entities: Available

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
`world.wit` exposes everything `//butcher`/`//remove` need:

- `world.get-entities() -> list<entity>` and `entity.get-position()` for
  radius filtering.
- `entity.get-type() -> entity-type` to classify entities (per
  `entity-types.wit`'s enum - note this enum has *no* hostile/passive/etc.
  labels, just type names, so the categories below need a hand-maintained
  table).
- `entity.remove()` to delete an entity outright, or `entity.damage(amount)` /
  `entity.is-dead()` for a "kill" (drops-respecting) semantic instead of
  instant removal.
- `entity.is-invulnerable()` - useful for FAWE's "don't butcher
  named/invulnerable pets" behavior.

## API Capability - Biomes: Blocked

`biomes.wit` defines the `biome` enum, and `world.wit` exposes
`get-biome(pos) -> biome` - **but there is no `set-biome` function anywhere in
`world.wit`**. Biome data is read-only from a plugin's perspective today.
**`//setbiome` and the `#biome` pattern cannot be implemented until Pumpkin
adds a `world.set-biome` (or equivalent biome-region) host function.** Do not
attempt a workaround (e.g. "set biome via block placement") - biomes are a
separate per-(x,z,quart-y) data layer from block states and there is no
plugin-visible way to write to it.

## Implementation Notes

- [ ] `//butcher [radius] [flags]`: pick a conservative default radius and
      document it (FAWE's is server-configurable). Iterate
      `world.get-entities()`, filter by distance from the player, then by a
      curated classification table mapping `entity-types.wit` variants to
      FAWE's categories (`FRIENDLY`, `PETS`, `NPCS`, `GOLEMS`, `ANIMALS`,
      `AMBIENT`, `TAGGED`, `ARMOR_STAND`, `WATER`) - similar in spirit to
      `mapping::FALLBACK`'s hand-maintained table. Ship a minimal "kill all
      hostile mobs in radius" (no flags) first, then add flag letters
      incrementally, with `-f` as shorthand for the friendly categories
      together.
- [ ] `//remove <type> [radius]`: same iteration, filtering by entity-type
      category (`projectiles`, `items`/XP orbs, `paintings`, `itemframes`,
      `boats`, `minecarts`, `all`) using `entity.get-type()`.
- [ ] Both commands should report a count (`"Removed N entities."`), and
      should **not** push to `history` - entity removal isn't currently
      undoable by this plugin's block-based history. Document this as a known
      limitation, matching FAWE's own "entity removal cannot be undone"
      behavior.
- [ ] Once `world.set-biome` (or an equivalent) exists upstream: implement
      `//setbiome` as a region-wide biome write, and `#biome` per
      [biome-patterns.md](../patterns/biome-patterns.md), including extending
      `history`/`EditEntry` to record biome changes (today it's block-state
      only).
- [ ] Add tests for the entity classification tables (given an `entity-type`,
      which `//butcher`/`//remove` category does it fall into).
