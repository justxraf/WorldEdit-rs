#![allow(dead_code)]
//! Pastes a parsed [`Schematic`] (from [`crate::schematic`]) into the world at
//! a centre point.
//!
//! TODO(FAWE parity): wire this into a `//schematic load <name>` + `//paste`
//! flow (FAWE's `//schem load`/`SchematicCommands`) so `.schem` files can be
//! loaded into the clipboard, not just placed directly. Currently unused by
//! any registered command — kept for that purpose.
//!
//! ## Centring
//! `centre` is the block position the island's middle should land on. The
//! schematic is placed so that its horizontal middle and its stored `Offset`
//! origin line up with `centre`:
//!
//! ```text
//! world_pos = centre + (local - schematic_origin)
//! ```
//!
//! where `schematic_origin` is `(width/2, 0, length/2)` adjusted by the file's
//! `Offset`. The Y base sits at `centre.y` (the schematic grows upward from
//! there). Tune [`PasteOptions`] if your islands need a different anchor.
//!
//! ## Air handling
//! By default air blocks in the schematic are skipped (so you can paste over
//! existing terrain without punching holes). Set `overwrite_with_air` to clear
//! the volume instead.

use std::collections::HashMap;

use pumpkin_plugin_api::common::BlockPos;
use pumpkin_plugin_api::world::BlockFlags;

use crate::{
    mapping,
    schematic::{Schematic, SchematicBlock, is_air_key},
};

/// Knobs controlling how a schematic is placed.
pub struct PasteOptions {
    /// If `true`, air blocks in the schematic overwrite existing blocks.
    /// If `false` (default), air is skipped.
    pub overwrite_with_air: bool,
    /// Block-update flags passed to world placement. Defaults to no neighbour
    /// updates and no drops — fastest and avoids cascade physics during a bulk
    /// paste.
    pub flags: BlockFlags,
}

impl Default for PasteOptions {
    fn default() -> Self {
        Self {
            overwrite_with_air: false,
            // Bulk schematic pastes should be quiet: no drops, no neighbour
            // physics, and no per-block placement callbacks/prep.
            flags: BlockFlags::SKIP_DROPS
                | BlockFlags::FORCE_STATE
                | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
                | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK,
        }
    }
}

/// Outcome of a paste.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PasteReport {
    pub placed: usize,
    pub skipped_air: usize,
    /// Palette keys that couldn't be mapped to a state id (counted once per
    /// occurrence). If this is non-zero you likely need the full `blocks.json`.
    pub unmapped: usize,
}

/// Paste `schematic` centred on `centre`, calling `place` for every block that
/// should be written.
///
/// Blocks are visited grouped by world chunk-section (16x16x16). Air and
/// unmapped cells are counted but never passed to `place`. Callers choose where
/// blocks go via `place`: production collects every `BlockChange` into one
/// `World::set_block_states` request; tests record placements, so this loop runs
/// without a live world.
///
/// ```rust,ignore
/// let mut changes = Vec::new();
/// let report = paste(&schematic, centre, &opts, |(x, y, z), state| {
///     changes.push(BlockChange { pos: BlockPos { x, y, z }, state });
/// });
/// world.set_block_states(&changes, opts.flags);
/// ```
pub fn paste<F: FnMut((i32, i32, i32), u16)>(
    schematic: &Schematic,
    centre: BlockPos,
    opts: &PasteOptions,
    mut place: F,
) -> PasteReport {
    let mut report = PasteReport::default();
    let base = origin(schematic, centre);

    // A schematic has only a handful of *distinct* palette keys, each repeated across
    // many cells. Resolving a key (`mapping::state_id_for`, which scans the block table
    // and allocates) is far costlier than placing a block, so memoize per key: the
    // inner loop then does a cheap lookup instead of a full resolve per block.
    let mut cache: HashMap<&str, Resolution> = HashMap::new();

    if !opts.overwrite_with_air && schematic.blocks.len() == schematic.volume() {
        report.skipped_air = schematic.volume() - schematic.non_air_blocks.len();
        let mut non_air_blocks = schematic.non_air_blocks.clone();
        non_air_blocks.sort_by_key(|block| sparse_order_key(base, *block));

        for block in non_air_blocks {
            let Some(key) = schematic.blocks.get(block.index).map(String::as_str) else {
                continue;
            };
            let resolution = *cache.entry(key).or_insert_with(|| resolve_key(key, opts));
            match resolution {
                Resolution::Place(state_id) => {
                    place(
                        (
                            base.x + block.x as i32,
                            base.y + block.y as i32,
                            base.z + block.z as i32,
                        ),
                        state_id,
                    );
                    report.placed += 1;
                }
                Resolution::SkippedAir => report.skipped_air += 1,
                Resolution::Unmapped => report.unmapped += 1,
            }
        }

        return report;
    }

    for (sy0, sy1) in section_bands(base.y, schematic.height) {
        for (sz0, sz1) in section_bands(base.z, schematic.length) {
            for (sx0, sx1) in section_bands(base.x, schematic.width) {
                for y in sy0..sy1 {
                    for z in sz0..sz1 {
                        for x in sx0..sx1 {
                            let Some(key) = schematic.block_at(x, y, z) else {
                                continue;
                            };
                            let resolution =
                                *cache.entry(key).or_insert_with(|| resolve_key(key, opts));
                            match resolution {
                                Resolution::Place(state_id) => {
                                    place(
                                        (base.x + x as i32, base.y + y as i32, base.z + z as i32),
                                        state_id,
                                    );
                                    report.placed += 1;
                                }
                                Resolution::SkippedAir => report.skipped_air += 1,
                                Resolution::Unmapped => report.unmapped += 1,
                            }
                        }
                    }
                }
            }
        }
    }

    report
}

fn sparse_order_key(base: BlockPos, block: SchematicBlock) -> (i32, i32, i32, u16, u16, u16) {
    let wx = base.x + block.x as i32;
    let wy = base.y + block.y as i32;
    let wz = base.z + block.z as i32;
    (
        wy.div_euclid(16),
        wz.div_euclid(16),
        wx.div_euclid(16),
        block.y,
        block.z,
        block.x,
    )
}

/// Split a single axis into local-coordinate bands, one per world chunk-section the
/// build spans on that axis. `base` is the world coordinate of local `0`; `dim` is the
/// schematic's size on the axis. Each returned `(lo, hi)` is a half-open local range
/// `lo..hi` whose world coordinates `base + local` all share one section (`coord >> 4`).
fn section_bands(base: i32, dim: u16) -> Vec<(u16, u16)> {
    let dim = dim as i32;
    if dim == 0 {
        return Vec::new();
    }

    let mut bands = Vec::new();
    let mut local = 0i32;
    while local < dim {
        let world = base + local;
        let next_section_start = (world.div_euclid(16) + 1) * 16;
        let band_end_world = next_section_start.min(base + dim);
        let band_end_local = band_end_world - base;
        bands.push((local as u16, band_end_local as u16));
        local = band_end_local;
    }
    bands
}

/// The world position that local `(0,0,0)` of the schematic maps to, given the
/// desired `centre`. Centres horizontally; grows up from `centre.y`.
pub fn origin(schematic: &Schematic, centre: BlockPos) -> BlockPos {
    BlockPos {
        x: centre.x - schematic.width as i32 / 2 - schematic.offset[0],
        y: centre.y - schematic.offset[1],
        z: centre.z - schematic.length as i32 / 2 - schematic.offset[2],
    }
}

/// What [`resolve_key`] decided to do with a palette key, independent of position.
/// `Copy` so it can be cached by key and looked up cheaply per block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Write this block state id.
    Place(u16),
    /// Air that we're configured to skip.
    SkippedAir,
    /// Palette key that didn't resolve to a state id.
    Unmapped,
}

/// Resolve a palette key to a [`Resolution`] under `opts` — pure, no world access and
/// position-independent, so [`paste`] can memoize it per distinct key. This is the
/// expensive step (it calls [`mapping::state_id_for`], which scans the block table and
/// allocates), which is exactly why the result is cached rather than recomputed per
/// block.
pub fn resolve_key(key: &str, opts: &PasteOptions) -> Resolution {
    if is_air_key(key) && !opts.overwrite_with_air {
        return Resolution::SkippedAir;
    }

    match mapping::state_id_for(key) {
        Some(state_id) => Resolution::Place(state_id),
        None => Resolution::Unmapped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic schematic from a flat block list (in `x + z*W + y*W*L`
    /// order — the same order [`Schematic`] uses internally).
    fn schem(width: u16, height: u16, length: u16, blocks: &[&str]) -> Schematic {
        assert_eq!(
            blocks.len(),
            width as usize * height as usize * length as usize,
            "test schematic block count must equal W*H*L"
        );
        Schematic {
            width,
            height,
            length,
            offset: [0, 0, 0],
            blocks: blocks.iter().map(|s| (*s).to_string()).collect(),
            non_air_blocks: blocks
                .iter()
                .enumerate()
                .filter_map(|(index, key)| {
                    if is_air_key(key) {
                        return None;
                    }
                    let x = (index % width as usize) as u16;
                    let z = ((index / width as usize) % length as usize) as u16;
                    let y = (index / (width as usize * length as usize)) as u16;
                    Some(SchematicBlock { x, y, z, index })
                })
                .collect(),
        }
    }

    /// A `BlockPos` for `centre`/`base` arguments.
    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    /// `paste` over a 1x1x1 stone schematic should place exactly one block, and the
    /// report should reflect it.
    #[test]
    fn pastes_single_block_via_paste_loop() {
        let s = schem(1, 1, 1, &["minecraft:stone"]);
        let opts = PasteOptions::default();
        let centre = at(0, 64, 0);

        let mut placed: Vec<((i32, i32, i32), u16)> = Vec::new();
        let report = paste(&s, centre, &opts, |pos, id| placed.push((pos, id)));

        assert_eq!(report.placed, 1);
        assert_eq!(report.skipped_air, 0);
        assert_eq!(report.unmapped, 0);
        assert_eq!(placed.len(), 1);
        // origin for 1x1x1: centre.x - 0 (width/2==0), centre.y, centre.z - 0.
        assert_eq!(placed[0].0, (0, 64, 0));
        assert_eq!(placed[0].1, 1); // stone -> state id 1
    }

    /// Air is skipped by default and must not reach the sink.
    #[test]
    fn skips_air_by_default() {
        let s = schem(2, 1, 1, &["minecraft:stone", "minecraft:air"]);
        let opts = PasteOptions::default();
        let mut placed = Vec::new();
        let report = paste(&s, at(0, 0, 0), &opts, |p, i| placed.push((p, i)));

        assert_eq!(report.placed, 1);
        assert_eq!(report.skipped_air, 1);
        assert_eq!(placed.len(), 1, "air must not be sent to the sink");
    }

    /// With `overwrite_with_air`, air resolves and is placed instead of skipped.
    #[test]
    fn air_placed_when_overwriting() {
        let s = schem(1, 1, 1, &["minecraft:air"]);
        let opts = PasteOptions {
            overwrite_with_air: true,
            ..PasteOptions::default()
        };
        let report = paste(&s, at(0, 0, 0), &opts, |_, _| {});
        assert_eq!(report.skipped_air, 0);
        // air maps to state id 0, so it counts as placed when overwriting.
        assert_eq!(report.placed, 1);
    }

    /// Unknown palette keys are tallied as unmapped, not placed.
    #[test]
    fn counts_unmapped_blocks() {
        let s = schem(1, 1, 1, &["minecraft:not_a_real_block"]);
        let opts = PasteOptions::default();
        let mut placed = 0usize;
        let report = paste(&s, at(0, 0, 0), &opts, |_, _| placed += 1);
        assert_eq!(report.unmapped, 1);
        assert_eq!(report.placed, 0);
        assert_eq!(placed, 0);
    }

    /// Blocks must land in `x, z, y` order at the right world coordinates. Use a
    /// 2(w) x 2(h) x 1(l) volume of dirt and check every destination position.
    #[test]
    fn places_blocks_at_correct_positions() {
        // W=2,H=2,L=1 => 4 cells. Layout index = x + z*W + y*W*L.
        // y=0: (0,0,0),(1,0,0)   y=1: (0,1,0),(1,1,0)
        let s = schem(
            2,
            2,
            1,
            &[
                "minecraft:dirt", // (x0,y0,z0)
                "minecraft:dirt", // (x1,y0,z0)
                "minecraft:dirt", // (x0,y1,z0)
                "minecraft:dirt", // (x1,y1,z0)
            ],
        );
        let opts = PasteOptions::default();
        let centre = at(100, 64, 100);
        // origin.x = 100 - 2/2 - 0 = 99 ; origin.y = 64 ; origin.z = 100 - 1/2 - 0 = 100.
        let mut placed: Vec<(i32, i32, i32)> = Vec::new();
        let report = paste(&s, centre, &opts, |pos, _| placed.push(pos));

        assert_eq!(report.placed, 4);
        // Expected world positions (base = (99,64,100)) for local x in {0,1}, y in {0,1}, z=0:
        let expected = [(99, 64, 100), (100, 64, 100), (99, 65, 100), (100, 65, 100)];
        assert_eq!(placed, expected);
        // dirt -> state id 10
        let report2 = paste(&s, centre, &opts, |_, _| {});
        assert_eq!(report2.placed, 4);
    }

    /// `resolve_key` maps palette keys to the right resolution.
    #[test]
    fn resolve_key_matches_expectations() {
        let opts = PasteOptions::default();
        assert_eq!(resolve_key("minecraft:stone", &opts), Resolution::Place(1));
        assert_eq!(resolve_key("minecraft:air", &opts), Resolution::SkippedAir);
        assert_eq!(
            resolve_key("minecraft:air[level=8]", &opts),
            Resolution::SkippedAir,
            "air with block-state suffix is still treated as air"
        );
        assert_eq!(resolve_key("minecraft:nope", &opts), Resolution::Unmapped);

        // With overwrite_with_air, air resolves to its state id instead of being skipped.
        let overwrite = PasteOptions {
            overwrite_with_air: true,
            ..PasteOptions::default()
        };
        assert_eq!(
            resolve_key("minecraft:air", &overwrite),
            Resolution::Place(0)
        );
    }

    /// `section_bands` splits an axis at world multiples of 16, expressed in local coords.
    #[test]
    fn section_bands_split_at_world_section_boundaries() {
        assert_eq!(section_bands(5, 30), vec![(0, 11), (11, 27), (27, 30)]);
        assert_eq!(section_bands(16, 32), vec![(0, 16), (16, 32)]);
        assert_eq!(section_bands(0, 10), vec![(0, 10)]);
        assert_eq!(section_bands(-5, 8), vec![(0, 5), (5, 8)]);
        assert_eq!(section_bands(0, 0), Vec::<(u16, u16)>::new());
    }

    /// Section-ordered paste must place exactly the same set of blocks as a direct
    /// `x, z, y` scan.
    #[test]
    fn section_order_places_same_set_as_naive_scan() {
        // 20x4x20 so the footprint crosses several 16-wide section boundaries.
        let (w, h, l) = (20u16, 4u16, 20u16);
        let mut blocks = Vec::new();
        for i in 0..(w as usize * h as usize * l as usize) {
            // Deterministic mix of mappable / air / unmapped.
            blocks.push(match i % 3 {
                0 => "minecraft:stone",
                1 => "minecraft:air",
                _ => "minecraft:dirt",
            });
        }
        let s = schem(w, h, l, &blocks);
        let opts = PasteOptions::default();
        let centre = at(7, 65, -3); // arbitrary, unaligned to 16

        let mut got: Vec<((i32, i32, i32), u16)> = Vec::new();
        let report = paste(&s, centre, &opts, |p, i| got.push((p, i)));

        // Reference set: naive x,z,y scan via resolve_key.
        let base = origin(&s, centre);
        let mut expected: Vec<((i32, i32, i32), u16)> = Vec::new();
        for y in 0..h {
            for z in 0..l {
                for x in 0..w {
                    if let Resolution::Place(state_id) =
                        resolve_key(s.block_at(x, y, z).unwrap(), &opts)
                    {
                        let pos = (base.x + x as i32, base.y + y as i32, base.z + z as i32);
                        expected.push((pos, state_id));
                    }
                }
            }
        }

        // Same multiset of placements.
        let mut got_sorted = got.clone();
        got_sorted.sort();
        let mut expected_sorted = expected.clone();
        expected_sorted.sort();
        assert_eq!(got_sorted, expected_sorted);
        assert_eq!(report.placed, expected.len());
        assert_eq!(
            report.placed + report.skipped_air + report.unmapped,
            w as usize * h as usize * l as usize
        );
    }

    /// The batch payload should finish one chunk-section before moving to another,
    /// so sequential host consumption does not visibly sweep bottom-up across the
    /// entire schematic.
    #[test]
    fn section_order_does_not_revisit_a_section() {
        let (w, h, l) = (40u16, 2u16, 8u16);
        let blocks = vec!["minecraft:stone"; w as usize * h as usize * l as usize];
        let s = schem(w, h, l, &blocks);
        let opts = PasteOptions::default();
        let centre = at(0, 64, 0);

        let mut sections_in_order: Vec<(i32, i32, i32)> = Vec::new();
        paste(&s, centre, &opts, |(x, y, z), _| {
            let sec = (x >> 4, y >> 4, z >> 4);
            if sections_in_order.last() != Some(&sec) {
                sections_in_order.push(sec);
            }
        });

        let mut seen = std::collections::HashSet::new();
        for sec in &sections_in_order {
            assert!(seen.insert(*sec), "section {sec:?} was revisited");
        }
        assert!(
            sections_in_order.len() > 1,
            "test should span multiple sections"
        );
    }
}
