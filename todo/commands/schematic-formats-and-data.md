# TODO: Schematic Formats And Block/Entity Data Preservation

## Missing Capabilities

- [ ] MCEdit `.schematic` (legacy "Schematic" NBT format) load/save
- [ ] Structure block `.nbt` format load/save
- [ ] Block entity data in `.schem` (chests, spawners, banners, skulls,
      command blocks, etc.) - currently lost
- [ ] Entities in `.schem` (`-e`/`-b` flags already rejected by `//copy`)

## Current State

`src/commands/schematic.rs` supports Sponge `.schem` v2/v3 (gzip NBT) only,
via `clipboard::from_schematic`/`to_schematic_blocks`. `ClipboardBuffer` stores
only `((i32,i32,i32), u16)` - block-state ids, no block-entity or entity data.
`//copy -e`/`-b` are explicitly rejected.
[block-data-syntax.md](../patterns/block-data-syntax.md) already tracks the
*pattern-syntax* side of this (e.g. `oak_sign|Line1|Line2` as a settable
pattern); this doc tracks the *clipboard/schematic round-trip* side.

## Why It Matters

Real-world schematics (downloaded builds, player creations) routinely contain
signs, chests with loot, spawners, and decorative entities (armor stands,
paintings, item frames). Losing all of that on `//schematic load` /
`//copy`+`//paste` makes round-tripping non-trivial structures lossy in ways
users will notice immediately.

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).

- **Sign text**: fully round-trippable. `block-entity.wit`'s
  `sign-block-entity` exposes `get/set-front-text`, `get/set-back-text`,
  `is/set-waxed` - after placing an `oak_sign` block via `set-block-states`,
  the plugin can call `world.get-block-entity(pos)`, match the
  `sign-block-entity` variant, and call `set-front-text`/`set-back-text` to
  restore saved text.
- **Chests, spawners, jukeboxes, command blocks**: `block-entity.wit` only
  exposes *read-only metadata* (chest viewer count; spawner spawn
  count/range/delay; jukebox playing state; command block's command/output/
  conditions). **There is no generic NBT get/set and no `set-block-entity`
  function** - so chest *contents*, spawner *entity type*, command block
  *command text* (writing it), banner *patterns*, skull *profiles*, etc.
  cannot be restored from a schematic. This is a hard API blocker for full
  block-entity fidelity beyond signs.
- **Entities**: `world.spawn-entity(entity-type, pos)` exists, so entities
  *can* be re-created on paste in principle - but `spawn-entity` only takes a
  type and position, not arbitrary NBT (no custom name, equipment, AI state,
  etc.). Combined with `//copy -e` being explicitly unimplemented, treat full
  entity round-tripping as a stretch goal: spawning bare entities of the right
  type at the right relative position is possible, but won't preserve their
  data.
- **MCEdit `.schematic` / structure `.nbt`**: pure parsing work, same
  `fastnbt`+`flate2` dependencies already in use. No API gap - this is the
  most tractable item in this doc.

## Implementation Notes

- [ ] Add MCEdit `.schematic` parsing (`TAG_Compound` with `Blocks`, `Data`,
      `Width`/`Height`/`Length` arrays - pre-flattening numeric id+data
      format) in `src/clipboard.rs`/`src/schematic.rs`, mapping legacy numeric
      id+data pairs to modern state ids. This will need a legacy-id mapping
      table - check whether one already exists upstream in Pumpkin before
      building one from scratch.
- [ ] Add structure `.nbt` (`size`, `blocks: [{pos, state}]`, `palette: [...]`,
      `entities: [...]`) parsing - closest to Sponge's format, likely the
      easiest second format to add.
- [ ] Extend `ClipboardBuffer` with an optional
      `block_entities: Vec<((i32,i32,i32), BlockEntityData)>` where
      `BlockEntityData` initially only has a `Sign { front, back, waxed }`
      variant (everything else stored as "unsupported, dropped" with a
      warning count, mirroring `PasteReport`'s existing `unmapped` counter in
      `src/schem_paste.rs`).
- [ ] On paste, after `set-block-states`, do a second pass over positions with
      `block_entities` entries: call `world.get-block-entity(pos)`, match
      `sign-block-entity`, and apply the saved text.
- [ ] On `//schematic save`/`//copy`, capture sign text via
      `get-block-entity`/`sign-block-entity::get-front-text`/`get-back-text`
      for any position whose block is a sign.
- [ ] Document (in code comments and `//schematic`/`//copy` help text) that
      chest contents, spawner types, command-block commands, banner patterns,
      and entities are **not preserved**, pending upstream Pumpkin API
      additions (`set-block-entity` / generic NBT access).
- [ ] Add tests for MCEdit/structure parsing against small known-good fixture
      files, and for sign round-trip (capture -> paste -> read-back) using a
      mocked `block-entity`.
