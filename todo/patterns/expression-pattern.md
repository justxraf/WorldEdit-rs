# TODO: Expression Pattern

## Pattern

- [ ] `= <expression>`

## Why It Is Missing

FAWE expression patterns need an expression parser/evaluator and a context with
coordinates, block data, masks, and possibly noise/math helpers.

## Implementation Notes

- [ ] Decide whether to implement a compatible expression language or embed an
      existing evaluator.
- [ ] Define available variables: coordinates, existing block, region size,
      origin, random/noise helpers, and constants.
- [ ] Sandbox evaluation so expressions cannot escape into plugin/server internals.
- [ ] Cache parsed expressions for performance.
- [ ] Add deterministic tests for math, coordinates, conditionals, and invalid
      expressions.

