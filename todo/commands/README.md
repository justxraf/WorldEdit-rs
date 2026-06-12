# Command & Feature TODOs

This folder tracks essential WorldEdit/FAWE commands and capabilities that
WorldEdit-rs does not implement yet, beyond the pattern-engine gaps already
tracked in [`todo/patterns/`](../patterns/README.md).

Findings here are grounded in three things:

- The current command surface registered by `src/commands/mod.rs`: `//pos1`,
  `//pos2`, `//hpos1`, `//hpos2`, `//sel`, `//set`, `//replace`, `//copy`,
  `//cut`, `//paste`, `//undo`, `//redo`, `//size`, `//clearclipboard`,
  `//clearhistory`, `//expand`, `//contract`, `//shift`, `//outset`,
  `//inset`, `//count`, `//walls`, `//faces`, `//outline`, `//wand`,
  `//schematic` (`//schem`), and `//brush` (`//br`).
- The Pumpkin plugin API exposed by `pumpkin-plugin-wit/v0.1/*.wit`
  (`world.wit`, `player.wit`, `biomes.wit`, `block-entity.wit`, `common.wit`),
  which determines whether a missing feature can be built today or is blocked
  on a future Pumpkin API addition.
- FAWE's actual command behavior (argument order, defaults, algorithms) from
  [IntellectualSites/FastAsyncWorldEdit](https://github.com/IntellectualSites/FastAsyncWorldEdit),
  so these plans target real parity rather than guesses.

## Files

- [selection-shapes.md](selection-shapes.md) - non-cuboid `//sel` types
  (sphere, cylinder, polygon, convex, ellipsoid) and the region/iteration
  changes they require. Foundational for several other docs below.
- [shape-generation.md](shape-generation.md) - `//sphere`, `//hsphere`,
  `//cyl`, `//hcyl`, `//cone`, `//pyramid`/`//hpyramid`, `//line`, `//curve`.
- [clipboard-transforms.md](clipboard-transforms.md) - `//rotate`, `//flip`,
  `//place`, and the block-state orientation transform they need.
- [region-manipulation.md](region-manipulation.md) - `//move`, `//stack`,
  `//overlay`, `//hollow`, `//deform`, `//regen`.
- [terrain-and-radius-tools.md](terrain-and-radius-tools.md) - `//smooth`,
  `//naturalize`, `//green`, `//snow`, `//thaw`, `//drain`, `//fixwater`,
  `//fixlava`, `//removeabove`, `//removebelow`, `//removenear`,
  `//replacenear`, `//fill`/`//fillr`.
- [navigation-and-tools.md](navigation-and-tools.md) - `//jumpto`, `//thru`,
  `//up`, `//ascend`, `//descend`, `//ceil`, `//unstuck`, `//toggleplace`,
  `//tool`, super pickaxe.
- [entity-and-biome-commands.md](entity-and-biome-commands.md) - `//butcher`,
  `//remove`, `//setbiome`, and the `#biome` pattern.
- [mask-coverage-and-global-mask.md](mask-coverage-and-global-mask.md) -
  `//gmask` and wider `BlockMask` support in `//count`/`//replace`/brushes.
- [schematic-formats-and-data.md](schematic-formats-and-data.md) - additional
  schematic formats and block-entity/entity data preservation.
- [history-and-sessions.md](history-and-sessions.md) - undo/redo limits,
  cross-player/cross-world history correctness, and session bookkeeping.

## Quick Reference: Pumpkin API Capability Summary

Each doc below links back here instead of re-deriving these findings. Source:
`pumpkin-plugin-wit/v0.1/world.wit`, `player.wit`, `biomes.wit`,
`block-entity.wit`, `common.wit`.

| Capability | Available? | Source |
| --- | --- | --- |
| Single + bulk block-state get/set | Yes | `world.get-block-state-id`, `get-block-state`, `set-block-state`, `set-block-states` |
| Heightmap / top-block queries | Yes | `world.get-top-block-y(x, z)`, `world.get-motion-blocking-height(x, z)` |
| Sky/block light get/set | Yes | `world.get/set-sky-light`, `world.get/set-block-light` |
| Player & entity teleport | Yes | `player.teleport`, `player.teleport-world`, `entity.teleport` |
| Raycast | Yes | `entity.raycast(max_distance, fluid_handling)` (already used for `//hpos1`/`//hpos2` and brushes) |
| World bounds / sea level | Yes | `world.get-min-y`, `world.get-sea-level` |
| Entity spawn / list / remove / damage | Yes | `world.spawn-entity`, `world.get-entities`, `entity.remove`, `entity.damage`, `entity.is-dead` |
| Entity type / classification | Yes (enum only, no hostile/passive labels) | `entity-types.wit` `entity-type` enum |
| **Biome read** | Yes | `world.get-biome(pos) -> biome` |
| **Biome write** | **No** - not in `world.wit` | no `set-biome` function exists |
| Block entity read: signs (full text r/w) | Yes | `block-entity.wit` `sign-block-entity::get/set-front-text`, `get/set-back-text`, `is/set-waxed` |
| Block entity read: chest/jukebox/spawner/command block | Read-only metadata only | `block-entity.wit` (viewer count, playing state, spawn count/range/delay, command/output/conditions) |
| **Arbitrary block-entity NBT / `set-block-entity`** | **No** | not in `block-entity.wit` |
| **World regeneration / chunk generator access** | **No** | not in `world.wit` |
| Explosions / particles / sounds | Yes | `world.create-explosion`, `spawn-particle`, `play-sound` |
| Weather | Yes | `world.is/set-raining`, `is/set-thundering` |

The two **No** rows above (biome writes, and arbitrary block-entity NBT /
world regeneration) are the hard blockers referenced throughout these docs.
Everything else here is a pure plugin-side implementation task.
