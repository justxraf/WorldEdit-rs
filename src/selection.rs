//! Per-player two-point selection (`//pos1` / `//pos2`) and the region it
//! describes.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::common::BlockPos;

/// A player's current selection corners. Either or both may be unset.
#[derive(Default, Clone, Copy)]
pub struct Selection {
    pub pos1: Option<BlockPos>,
    pub pos2: Option<BlockPos>,
}

impl Selection {
    /// The axis-aligned region spanning both corners, or `None` if either
    /// corner is unset.
    pub fn region(&self) -> Option<Region> {
        match (self.pos1, self.pos2) {
            (Some(a), Some(b)) => Some(Region::new(a, b)),
            _ => None,
        }
    }
}

/// An inclusive axis-aligned block region, with `min` componentwise <= `max`.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub min: BlockPos,
    pub max: BlockPos,
}

impl Region {
    /// Build a region from two arbitrary corners, normalizing min/max per axis.
    pub fn new(a: BlockPos, b: BlockPos) -> Self {
        Self {
            min: BlockPos {
                x: a.x.min(b.x),
                y: a.y.min(b.y),
                z: a.z.min(b.z),
            },
            max: BlockPos {
                x: a.x.max(b.x),
                y: a.y.max(b.y),
                z: a.z.max(b.z),
            },
        }
    }

    /// Number of blocks contained in the region.
    pub fn volume(&self) -> usize {
        let dx = (self.max.x - self.min.x + 1) as usize;
        let dy = (self.max.y - self.min.y + 1) as usize;
        let dz = (self.max.z - self.min.z + 1) as usize;
        dx * dy * dz
    }

    /// Visit every block position in the region in `x, z, y` order, calling
    /// `flush` with each batch of at most `batch_size` positions (plus once
    /// more for any remainder).
    ///
    /// A large region (e.g. a `//set` over thousands of blocks) must not be
    /// collected into one giant `Vec`, since that can overflow the plugin's
    /// 32-bit wasm linear memory.
    pub fn for_each_batch<F: FnMut(&[BlockPos])>(&self, batch_size: usize, mut flush: F) {
        let batch_size = batch_size.max(1);
        let mut batch = Vec::with_capacity(batch_size);
        for y in self.min.y..=self.max.y {
            for z in self.min.z..=self.max.z {
                for x in self.min.x..=self.max.x {
                    batch.push(BlockPos { x, y, z });
                    if batch.len() >= batch_size {
                        flush(&batch);
                        batch.clear();
                    }
                }
            }
        }
        if !batch.is_empty() {
            flush(&batch);
        }
    }
}

thread_local! {
    /// Selections keyed by player name. The plugin's wasm component is
    /// single-threaded, so a thread-local map is sufficient.
    static SELECTIONS: RefCell<HashMap<String, Selection>> = RefCell::new(HashMap::new());
}

/// Read a player's selection (or the default, empty selection) via `f`.
pub fn with_selection<T>(key: &str, f: impl FnOnce(&Selection) -> T) -> T {
    SELECTIONS.with_borrow(|map| {
        f(map
            .get(key)
            .copied()
            .as_ref()
            .unwrap_or(&Selection::default()))
    })
}

/// Mutate a player's selection, creating it if absent.
pub fn with_selection_mut<T>(key: &str, f: impl FnOnce(&mut Selection) -> T) -> T {
    SELECTIONS.with_borrow_mut(|map| f(map.entry(key.to_string()).or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn region_normalizes_corners() {
        let r = Region::new(at(5, 5, 5), at(0, 10, 2));
        assert_eq!((r.min.x, r.min.y, r.min.z), (0, 5, 2));
        assert_eq!((r.max.x, r.max.y, r.max.z), (5, 10, 5));
    }

    #[test]
    fn volume_is_inclusive() {
        let r = Region::new(at(0, 0, 0), at(1, 1, 1));
        assert_eq!(r.volume(), 8);
        let r = Region::new(at(0, 0, 0), at(0, 0, 0));
        assert_eq!(r.volume(), 1);
    }

    #[test]
    fn for_each_batch_visits_every_block_once() {
        let r = Region::new(at(0, 0, 0), at(2, 1, 2)); // 3x2x3 = 18
        let mut seen = Vec::new();
        r.for_each_batch(5, |batch| seen.extend_from_slice(batch));
        assert_eq!(seen.len(), r.volume());
        let mut unique = seen.clone();
        unique.sort_by_key(|p| (p.x, p.y, p.z));
        unique.dedup_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(unique.len(), r.volume());
    }

    #[test]
    fn selection_region_requires_both_points() {
        let sel = Selection {
            pos1: Some(at(0, 0, 0)),
            pos2: None,
        };
        assert!(sel.region().is_none());

        let sel = Selection {
            pos1: Some(at(0, 0, 0)),
            pos2: Some(at(1, 1, 1)),
        };
        assert!(sel.region().is_some());
    }
}
