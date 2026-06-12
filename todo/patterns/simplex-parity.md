# TODO: Exact Simplex Pattern Parity

## Pattern

- [x] `#simplex <scale=10> <pattern>`

## Current State

The engine has a lightweight coordinate-scaled implementation so the syntax is
represented and nested patterns can be sampled coarsely.

## Why It Is Incomplete

FAWE uses simplex noise to randomize pattern selection. Coordinate scaling alone
does not match FAWE's distribution or visual texture.

## Implementation Notes

- [x] Identify FAWE's exact simplex noise behavior and seed handling.
- [x] Add a small deterministic noise implementation or dependency.
- [x] Make nested weighted/list patterns use the noise value rather than the
      current position hash behavior.
- [x] Add tests with fixed coordinates and expected bucket choices.
