# TODO: Expression Pattern

## Pattern

- [x] `= <expression>` (core first pass)

## Why It Is Missing

FAWE expression patterns need an expression parser/evaluator and a context with
coordinates, block data, masks, and possibly noise/math helpers.

## Implementation Notes

- [x] Implement a cached expression parser/evaluator instead of embedding a
      general scripting runtime.
- [x] Support FAWE-style `=` parsing with `x`, `y`, `z`, constants, arithmetic,
      comparisons, boolean logic, ternaries, and the `query/queryAbs/queryRel`
      family against the operation world context.
- [x] Sandbox evaluation by using a closed expression AST with only explicitly
      registered operators/functions.
- [x] Cache parsed expressions for performance by storing the compiled AST in
      the parsed pattern.
- [x] Add deterministic tests for math, coordinates, conditionals, and invalid
      expressions.
- [ ] Expand toward FAWE's fuller expression language: assignments, statement
      blocks, `if`/loop constructs, buffer helpers, and parity for the legacy
      noise/helper surface.
- [ ] Decide whether to expose more edit-context variables once the rest of the
      pattern engine grows richer world/session state.

// expression.rs