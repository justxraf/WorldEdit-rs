//! Per-player copy/paste buffer.
//!
//! `//copy` captures every block in the current selection, recording each as
//! a position offset relative to the player's position at copy time plus its
//! state id. `//paste` re-applies those offsets relative to the player's
//! position at paste time.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::{common::BlockPos, world::World};

use crate::mapping;
use crate::schematic::Schematic;
use crate::selection::Region;
use crate::transform::Transform;

/// `(width, height, length, offset, palette_keys)`, as produced by
/// [`ClipboardBuffer::to_schematic_blocks`] and consumed by
/// [`crate::schematic::write`].
pub type SchematicBlocks = (u16, u16, u16, [i32; 3], Vec<String>);

/// A captured region: each entry is `((dx, dy, dz), state_id)` relative to the
/// origin position passed to [`capture`].
#[derive(Clone)]
pub struct ClipboardBuffer {
    pub origin: BlockPos,
    pub blocks: Vec<((i32, i32, i32), u16)>,
}

impl Default for ClipboardBuffer {
    fn default() -> Self {
        Self {
            origin: BlockPos { x: 0, y: 0, z: 0 },
            blocks: Vec::new(),
        }
    }
}

impl ClipboardBuffer {
    pub fn bounds(&self, include_air: bool) -> Option<Region> {
        self.target_region(self.origin, include_air)
    }

    pub fn target_region(&self, paste_origin: BlockPos, include_air: bool) -> Option<Region> {
        let mut min: Option<BlockPos> = None;
        let mut max: Option<BlockPos> = None;

        for &((dx, dy, dz), state) in &self.blocks {
            if !include_air && state == 0 {
                continue;
            }
            let pos = BlockPos {
                x: paste_origin.x.checked_add(dx)?,
                y: paste_origin.y.checked_add(dy)?,
                z: paste_origin.z.checked_add(dz)?,
            };
            min = Some(match min {
                Some(current) => BlockPos {
                    x: current.x.min(pos.x),
                    y: current.y.min(pos.y),
                    z: current.z.min(pos.z),
                },
                None => pos,
            });
            max = Some(match max {
                Some(current) => BlockPos {
                    x: current.x.max(pos.x),
                    y: current.y.max(pos.y),
                    z: current.z.max(pos.z),
                },
                None => pos,
            });
        }

        Some(Region {
            min: min?,
            max: max?,
        })
    }

    /// Flatten this clipboard into a `width x height x length` array of
    /// Sponge palette keys (in `x + z*W + y*W*L` order), plus the dimensions
    /// and `Offset` to pass to [`crate::schematic::write`].
    ///
    /// Cells not present in `blocks` (and any outside the bounding box) are
    /// filled with `minecraft:air`. The `Offset` is the bounding box's `min`
    /// corner relative to `origin`, negated, so [`from_schematic`] can
    /// reconstruct an equivalent clipboard on load.
    ///
    /// Returns `None` if the clipboard is empty.
    pub fn to_schematic_blocks(&self) -> Option<SchematicBlocks> {
        let region = self.bounds(true)?;
        let width = (region.max.x - region.min.x + 1) as u16;
        let height = (region.max.y - region.min.y + 1) as u16;
        let length = (region.max.z - region.min.z + 1) as u16;

        let volume = width as usize * height as usize * length as usize;
        let mut blocks = vec!["minecraft:air".to_string(); volume];

        for &((dx, dy, dz), state) in &self.blocks {
            let wx = self.origin.x + dx;
            let wy = self.origin.y + dy;
            let wz = self.origin.z + dz;
            let x = (wx - region.min.x) as usize;
            let y = (wy - region.min.y) as usize;
            let z = (wz - region.min.z) as usize;
            let index = x + z * width as usize + y * width as usize * length as usize;
            blocks[index] = mapping::palette_key_for_state_id(state);
        }

        let offset = [
            region.min.x - self.origin.x,
            region.min.y - self.origin.y,
            region.min.z - self.origin.z,
        ];
        Some((width, height, length, offset, blocks))
    }
}

/// Build a [`ClipboardBuffer`] from a parsed `.schem` ([`Schematic`]).
///
/// Local cell `(x, y, z)` becomes offset `(x + offset.x, y + offset.y, z +
/// offset.z)` from `origin` (set to world `(0, 0, 0)`), so a later `//paste`
/// places the schematic's `Offset` anchor at the player's position —
/// matching WorldEdit's `//schematic load` + `//paste` behaviour. Every
/// cell, including air, is recorded so paste keeps its "stamp" semantics.
pub fn from_schematic(schematic: &Schematic) -> ClipboardBuffer {
    let mut blocks = Vec::with_capacity(schematic.volume());
    for y in 0..schematic.height {
        for z in 0..schematic.length {
            for x in 0..schematic.width {
                let Some(key) = schematic.block_at(x, y, z) else {
                    continue;
                };
                let state = mapping::state_id_for(key).unwrap_or(0);
                let offset = (
                    x as i32 + schematic.offset[0],
                    y as i32 + schematic.offset[1],
                    z as i32 + schematic.offset[2],
                );
                blocks.push((offset, state));
            }
        }
    }
    ClipboardBuffer {
        origin: BlockPos { x: 0, y: 0, z: 0 },
        blocks,
    }
}

/// Read every block in `region` from `world`, recording it relative to `origin`.
///
/// Air blocks (state id `0`) are still recorded so a paste can overwrite the
/// destination's existing contents, matching `//copy` + `//paste`'s "stamp"
/// semantics rather than a sparse, air-skipping schematic paste.
pub fn capture(world: &World, region: &Region, origin: BlockPos) -> ClipboardBuffer {
    capture_filtered(world, region, origin, |_| true)
}

/// Read every block in `region`, replacing states that fail `include` with air.
///
/// This mirrors FAWE's `//copy -m <mask>` behavior for the literal-mask subset:
/// non-matching source blocks are still represented in the clipboard, but as
/// air, so paste dimensions remain unchanged.
pub fn capture_filtered(
    world: &World,
    region: &Region,
    origin: BlockPos,
    mut include: impl FnMut(u16) -> bool,
) -> ClipboardBuffer {
    let mut blocks = Vec::with_capacity(region.volume());
    for y in region.min.y..=region.max.y {
        for z in region.min.z..=region.max.z {
            for x in region.min.x..=region.max.x {
                let pos = BlockPos { x, y, z };
                let mut state = world.get_block_state_id(pos);
                if !include(state) {
                    state = 0;
                }
                blocks.push(((x - origin.x, y - origin.y, z - origin.z), state));
            }
        }
    }
    ClipboardBuffer { origin, blocks }
}

thread_local! {
    /// Clipboards keyed by player name. Each entry stores a buffer and its pending transform.
    /// The plugin's wasm component is single-threaded, so a thread-local map is sufficient.
    static CLIPBOARDS: RefCell<HashMap<String, (ClipboardBuffer, Transform)>> = RefCell::new(HashMap::new());
}

/// Store `buffer` as `key`'s clipboard with identity transform, replacing any previous contents.
pub fn set(key: &str, buffer: ClipboardBuffer) {
    CLIPBOARDS.with_borrow_mut(|map| {
        map.insert(key.to_string(), (buffer, Transform::identity()));
    });
}

/// Clone `key`'s clipboard buffer (without its transform), if any.
pub fn get(key: &str) -> Option<ClipboardBuffer> {
    CLIPBOARDS.with_borrow(|map| map.get(key).map(|(buffer, _)| buffer.clone()))
}

/// Get `key`'s clipboard buffer and its pending transform.
pub fn get_with_transform(key: &str) -> Option<(ClipboardBuffer, Transform)> {
    CLIPBOARDS.with_borrow(|map| map.get(key).cloned())
}

/// Update the transform for `key`'s clipboard, combining with any existing transform.
pub fn set_transform(key: &str, new_transform: Transform) {
    CLIPBOARDS.with_borrow_mut(|map| {
        if let Some((buffer, transform)) = map.get_mut(key) {
            *transform = transform.combine(new_transform);
        }
    });
}

/// Remove `key`'s clipboard. Returns `true` if one existed.
pub fn clear(key: &str) -> bool {
    CLIPBOARDS.with_borrow_mut(|map| map.remove(key).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn target_region_uses_offsets_relative_to_paste_origin() {
        let buffer = ClipboardBuffer {
            origin: at(10, 20, 30),
            blocks: vec![((0, 0, 0), 1), ((2, 3, -1), 2)],
        };
        let region = buffer.target_region(at(100, 50, -10), true).unwrap();
        assert_eq!((region.min.x, region.min.y, region.min.z), (100, 50, -11));
        assert_eq!((region.max.x, region.max.y, region.max.z), (102, 53, -10));
    }

    #[test]
    fn bounds_use_original_origin() {
        let buffer = ClipboardBuffer {
            origin: at(10, 20, 30),
            blocks: vec![((0, 0, 0), 1), ((2, 3, -1), 2)],
        };
        let region = buffer.bounds(true).unwrap();
        assert_eq!((region.min.x, region.min.y, region.min.z), (10, 20, 29));
        assert_eq!((region.max.x, region.max.y, region.max.z), (12, 23, 30));
    }

    #[test]
    fn target_region_can_ignore_air() {
        let buffer = ClipboardBuffer {
            origin: at(0, 0, 0),
            blocks: vec![((0, 0, 0), 0), ((1, 0, 0), 1)],
        };
        let region = buffer.target_region(at(5, 5, 5), false).unwrap();
        assert_eq!((region.min.x, region.max.x), (6, 6));
    }

    #[test]
    fn target_region_is_none_when_everything_is_skipped() {
        let buffer = ClipboardBuffer {
            origin: at(0, 0, 0),
            blocks: vec![((0, 0, 0), 0)],
        };
        assert!(buffer.target_region(at(0, 0, 0), false).is_none());
    }

    #[test]
    fn clear_removes_stored_clipboard() {
        let key = "clear_removes_stored_clipboard";
        set(
            key,
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 1)],
            },
        );
        assert!(get(key).is_some());
        assert!(clear(key));
        assert!(get(key).is_none());
        assert!(!clear(key));
    }

    #[test]
    fn get_with_transform_returns_identity_by_default() {
        let key = "get_with_transform_returns_identity_by_default";
        set(
            key,
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 1)],
            },
        );
        let (_buffer, transform) = get_with_transform(key).unwrap();
        assert!(transform.is_identity());
    }

    #[test]
    fn set_transform_combines_with_existing() {
        use crate::transform::Transform;

        let key = "set_transform_combines_with_existing";
        set(
            key,
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 1)],
            },
        );
        set_transform(key, Transform::rotate_y(90).unwrap());
        set_transform(key, Transform::rotate_y(90).unwrap());
        let (_, transform) = get_with_transform(key).unwrap();
        assert_eq!(transform.rot_y, 2); // Two 90° rotations = 180°
    }

    #[test]
    fn to_schematic_blocks_lays_out_in_x_z_y_order() {
        // 2x2x1 region: stone at (0,0,0), dirt at (1,0,0), the rest air.
        let buffer = ClipboardBuffer {
            origin: at(0, 0, 0),
            blocks: vec![((0, 0, 0), 1), ((1, 0, 0), 10)],
        };
        let (width, height, length, offset, blocks) = buffer.to_schematic_blocks().unwrap();
        assert_eq!((width, height, length), (2, 1, 1));
        assert_eq!(offset, [0, 0, 0]);
        assert_eq!(blocks, vec!["minecraft:stone", "minecraft:dirt"]);
    }

    #[test]
    fn to_schematic_blocks_offset_reflects_origin_to_min_corner() {
        let buffer = ClipboardBuffer {
            origin: at(10, 20, 30),
            blocks: vec![((0, 0, 0), 1), ((-1, 0, 0), 10)],
        };
        let (_, _, _, offset, _) = buffer.to_schematic_blocks().unwrap();
        // min corner is at origin + (-1,0,0), so Offset = min - origin = (-1,0,0).
        assert_eq!(offset, [-1, 0, 0]);
    }

    #[test]
    fn to_schematic_blocks_none_for_empty_clipboard() {
        let buffer = ClipboardBuffer::default();
        assert!(buffer.to_schematic_blocks().is_none());
    }

    #[test]
    fn from_schematic_round_trips_to_schematic_blocks() {
        use crate::schematic::{Schematic, SchematicBlock};

        let schem = Schematic {
            width: 2,
            height: 1,
            length: 1,
            offset: [-1, 0, 0],
            blocks: vec!["minecraft:stone".to_string(), "minecraft:dirt".to_string()],
            non_air_blocks: vec![
                SchematicBlock {
                    x: 0,
                    y: 0,
                    z: 0,
                    index: 0,
                },
                SchematicBlock {
                    x: 1,
                    y: 0,
                    z: 0,
                    index: 1,
                },
            ],
        };

        let buffer = from_schematic(&schem);
        let (width, height, length, offset, blocks) = buffer.to_schematic_blocks().unwrap();
        assert_eq!((width, height, length), (2, 1, 1));
        assert_eq!(offset, [-1, 0, 0]);
        assert_eq!(blocks, schem.blocks);
    }
}
