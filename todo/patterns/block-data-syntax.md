# TODO: Block Data And Special Syntax

## Syntax

- [x] Block NBT/SNBT, for example `oak_sign{'is_waxed':1}`
- [x] Combined states plus NBT, for example `oak_sign[rotation=12]{'is_waxed':1}`
- [x] Sign text syntax, for example `oak_sign|Line1|Line2`
- [ ] Player head syntax, for example `player_head|dinnerbone`
- [ ] Mob spawner syntax, for example `spawner|squid`

## Why It Is Missing

The current engine resolves to Pumpkin global block-state ids only. These syntax
forms need block entity data and sometimes entity type or profile lookup support.

## Implementation Notes

- [x] Extend the edit pipeline to carry block entity payloads alongside state ids.
- [x] Parse SNBT with a structured parser rather than ad hoc string splitting.
- [x] Decide how to store and undo block entity changes in history.
- [x] Add sign text serialization compatible with the target Minecraft version.
- [ ] Add player profile resolution or document that only raw profile data is
      supported.
- [ ] Add spawner entity-id validation.
- [x] Add tests for parser ordering: block states before NBT, pipe syntax after
      block id, and quoted text with spaces.

## Current Scope

Sign block data now works through the current Pumpkin API, including front-text
pipe syntax and sign SNBT fields such as `is_waxed`, `front_text`, and
`back_text`.

Player heads, spawners, and generic block-entity SNBT are still blocked
because Pumpkin exposes sign setters but not generic block-entity writes,
profile setters, or spawner-type mutation hooks to plugins yet.
