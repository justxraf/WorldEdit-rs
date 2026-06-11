//! Per-player copy/paste buffer.
//!
//! `//copy` captures every block in the current selection, recording each as
//! a position offset relative to the player's position at copy time plus its
//! state id. `//paste` re-applies those offsets relative to the player's
//! position at paste time.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::{common::BlockPos, world::World};

use crate::selection::Region;

/// A captured region: each entry is `((dx, dy, dz), state_id)` relative to the
/// origin position passed to [`capture`].
#[derive(Default, Clone)]
pub struct ClipboardBuffer {
    pub blocks: Vec<((i32, i32, i32), u16)>,
}

/// Read every block in `region` from `world`, recording it relative to `origin`.
///
/// Air blocks (state id `0`) are still recorded so a paste can overwrite the
/// destination's existing contents, matching `//copy` + `//paste`'s "stamp"
/// semantics rather than a sparse, air-skipping schematic paste.
pub fn capture(world: &World, region: &Region, origin: BlockPos) -> ClipboardBuffer {
    let mut blocks = Vec::with_capacity(region.volume());
    for y in region.min.y..=region.max.y {
        for z in region.min.z..=region.max.z {
            for x in region.min.x..=region.max.x {
                let pos = BlockPos { x, y, z };
                let state = world.get_block_state_id(pos);
                blocks.push(((x - origin.x, y - origin.y, z - origin.z), state));
            }
        }
    }
    ClipboardBuffer { blocks }
}

thread_local! {
    /// Clipboards keyed by player name. The plugin's wasm component is
    /// single-threaded, so a thread-local map is sufficient.
    static CLIPBOARDS: RefCell<HashMap<String, ClipboardBuffer>> = RefCell::new(HashMap::new());
}

/// Store `buffer` as `key`'s clipboard, replacing any previous contents.
pub fn set(key: &str, buffer: ClipboardBuffer) {
    CLIPBOARDS.with_borrow_mut(|map| {
        map.insert(key.to_string(), buffer);
    });
}

/// Clone `key`'s clipboard, if any.
pub fn get(key: &str) -> Option<ClipboardBuffer> {
    CLIPBOARDS.with_borrow(|map| map.get(key).cloned())
}
