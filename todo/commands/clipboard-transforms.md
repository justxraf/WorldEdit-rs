# TODO: Clipboard Rotation And Flipping

## Missing Commands

- [ ] `//rotate <y> [<x> [<z>]]` (degrees; rotates the clipboard's future
      pastes)
- [ ] `//flip [direction]` (mirrors the clipboard's future pastes across an
      axis; defaults to the player's facing direction)
- [ ] `//place [-a] [-o] [-s] [-n] [-e] [-b] [-x]` (paste ignoring the
      clipboard's pending rotate/flip transform)

## Current State

`src/clipboard.rs`'s `ClipboardBuffer { origin, blocks: Vec<((i32,i32,i32),
u16)> }` stores raw offsets and block-state ids with no transform step.
`//paste` (`src/commands/paste.rs`) pastes the buffer as-is; there's no
pending-transform concept anywhere.

## Why It Matters

`//rotate` and `//flip` are core WorldEdit clipboard commands - copy a
structure once, then rotate/mirror it to place variants without re-copying.
Without them, every rotated placement requires a fresh `//copy` from a
differently-oriented source.

## FAWE Reference Behavior

(from `ClipboardCommands.java`, for parity) **`//rotate`/`//flip` do not
mutate the clipboard's stored block data.** They build an `AffineTransform`
(rotation: positive angle = clockwise, angles should be multiples of 90
degrees; flip: `scale(-1)` on the flipped axis) and combine it with whatever
transform is already attached to the `ClipboardHolder` via
`holder.setTransform(transform.combine(existing))`. The accumulated transform
is applied lazily, only when the clipboard is **pasted** - repeated
`//rotate`/`//flip` calls compose. `//rotate` takes the Y-axis angle as a
required argument, with optional X and Z angles (both default `0`).

FAWE additionally provides **`//place [-a] [-o] [-s] [-n] [-e] [-b] [-x]`**
(`ClipboardCommands.place`), a paste variant that **ignores the clipboard
holder's accumulated transform entirely** - it pastes the raw, untransformed
buffer. By default it targets the placement position (see
[terrain-and-radius-tools.md](terrain-and-radius-tools.md)); `-o` targets the
clipboard's stored origin instead. `-n` selects the region the paste *would*
occupy without actually pasting (implies `-s`); `-x` removes existing entities
in the affected region first; `-a`/`-s`/`-e`/`-b` mirror `//paste`'s flags.
This is the escape hatch for "I rotated/flipped the clipboard for other
pastes, but want one paste without that transform."

## API Capability

See [README.md's capability table](README.md#quick-reference-pumpkin-api-capability-summary).
Pure data transformation - no Pumpkin API needed. The hard part is that block
*state ids* encode orientation (`facing`, `axis`, `rotation`, `half`, `shape`,
etc. - stairs, logs, doors, rails, signs, glazed terracotta, ...), so applying
a transform means remapping both block *positions* (around the clipboard's
bounding-box center) and each block's *state properties*.

## Implementation Notes

- [ ] Mirror FAWE's design: store a pending transform alongside the
      clipboard rather than mutating `ClipboardBuffer` in place. Add
      `transform: Transform` (default identity) next to the buffer in
      `CLIPBOARDS`, where `Transform` composes 90-degree-multiple Y/X/Z
      rotations and axis flips.
  - `//rotate`/`//flip` only update this `Transform` (composing with the
    existing one, like FAWE's `combine`).
  - `//paste` (`src/commands/paste.rs`) applies the transform to each
    `((dx, dy, dz), state)` entry *at paste time*: rotate/mirror `(dx, dy,
    dz)` around the buffer's bounds center, and remap `state` via the
    block-state transform helper below.
  - `//schematic save`/`//copy` continue to serialize the *untransformed*
    buffer (matches FAWE: the transform lives on the holder, not the
    clipboard content).
- [ ] Add `transform::rotate_state(state_id, axis, degrees)` and
      `transform::flip_state(state_id, axis)` (new `src/block_transform.rs`,
      or alongside `src/mapping.rs`) that:
  - Split the state into `(name, properties)` via the existing
    `split_key`/`palette_key_for_state_id` helpers.
  - Remap direction-valued properties (`facing`, `axis`, `rotation`,
    `orientation`, `hinge`, `shape` for rails/stairs) according to the
    transform.
  - Re-resolve the transformed `(name, properties)` back to a state id via
    `state_id_for`, falling back to the original state if the transformed
    combination doesn't exist (mirrors `apply_existing_states`'s fallback
    style).
- [ ] Start with 90/180/270-degree Y rotations and single-axis flips - these
      cover the vast majority of real usage and avoid arbitrary-angle
      resampling entirely. FAWE allows arbitrary angles (and X/Z rotation),
      but those require resampling/interpolation across the block grid;
      document that as an explicit follow-up, not part of the first pass.
- [ ] `//paste` already reads clipboard bounds via `clipboard::bounds`/
      `target_region` - verify these are recomputed from the *transformed*
      bounding box when a non-identity transform is set.
- [ ] `//place`: factor `//paste`'s "apply transform then blit buffer" step
      into a helper that takes an explicit `Transform` (identity for
      `//place`, the stored transform for `//paste`), so both commands share
      the blit/selection-update logic. `-o` targets the clipboard's stored
      origin instead of the placement position; other flags (`-a`, `-s`, `-n`,
      `-e`, `-b`, `-x`) mirror whichever of `//paste`'s flags are already
      implemented.
- [ ] Add tests: round-trip rotate 4x90 == identity, flip twice == identity,
      composing rotate-then-flip matches FAWE's `combine` order, and a known
      oriented block (e.g. `oak_stairs[facing=north]`) rotates to the expected
      `facing=east` etc.
