# Pattern Engine TODOs

This folder tracks the remaining FAWE/WorldEdit pattern work after the current
block-state-only engine expansion.

The current engine can evaluate patterns from:

```text
BlockPos + existing block-state id -> new block-state id
```

Anything that needs clipboards, block entities, biome writes, full edit-session
context, world surface queries, data-pack registries, or expression evaluation is
tracked here instead of being hidden behind vague parser errors.

## Files

- [block-tags.md](block-tags.md) - real Minecraft/data-pack tag support for `##tag` and `##*tag`.
- [block-data-syntax.md](block-data-syntax.md) - NBT/SNBT, sign text, player heads, and spawners.
- [color-patterns.md](color-patterns.md) - FAWE color matching and color transforms.
- [stateful-world-context.md](stateful-world-context.md) - buffers, relative patterns, and surface/solid spread.
- [biome-patterns.md](biome-patterns.md) - `#biome`.
- [expression-pattern.md](expression-pattern.md) - `= <expression>`.
- [simplex-parity.md](simplex-parity.md) - exact FAWE `#simplex` parity.

