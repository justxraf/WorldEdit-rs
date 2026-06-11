//! Per-player two-point selection (`//pos1` / `//pos2`) and the region it
//! describes.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::common::BlockPos;

/// A cardinal direction used by selection transform commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    North,
    South,
    West,
    East,
}

impl Direction {
    pub fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "u" | "up" => Some(Self::Up),
            "d" | "down" => Some(Self::Down),
            "n" | "north" => Some(Self::North),
            "s" | "south" => Some(Self::South),
            "w" | "west" => Some(Self::West),
            "e" | "east" => Some(Self::East),
            _ => None,
        }
    }

    pub fn from_yaw_pitch(yaw: f32, pitch: f32) -> Self {
        if pitch <= -67.5 {
            return Self::Up;
        }
        if pitch >= 67.5 {
            return Self::Down;
        }

        match yaw.rem_euclid(360.0) {
            y if (45.0..135.0).contains(&y) => Self::West,
            y if (135.0..225.0).contains(&y) => Self::North,
            y if (225.0..315.0).contains(&y) => Self::East,
            _ => Self::South,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }

    fn vector(self) -> (i32, i32, i32) {
        match self {
            Self::Up => (0, 1, 0),
            Self::Down => (0, -1, 0),
            Self::North => (0, 0, -1),
            Self::South => (0, 0, 1),
            Self::West => (-1, 0, 0),
            Self::East => (1, 0, 0),
        }
    }
}

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

    /// Expand one face of the region by `amount` blocks in `direction`.
    /// Negative amounts shrink that face. Returns `None` if the region would
    /// invert.
    pub fn expanded(self, amount: i32, direction: Direction) -> Option<Self> {
        let mut out = self;
        match direction {
            Direction::Up => out.max.y = out.max.y.checked_add(amount)?,
            Direction::Down => out.min.y = out.min.y.checked_sub(amount)?,
            Direction::North => out.min.z = out.min.z.checked_sub(amount)?,
            Direction::South => out.max.z = out.max.z.checked_add(amount)?,
            Direction::West => out.min.x = out.min.x.checked_sub(amount)?,
            Direction::East => out.max.x = out.max.x.checked_add(amount)?,
        }
        (out.min.x <= out.max.x && out.min.y <= out.max.y && out.min.z <= out.max.z).then_some(out)
    }

    /// Contract the side opposite `direction`, matching WorldEdit's
    /// `//contract`: contracting down shrinks from the top, contracting north
    /// shrinks from the south, and so on.
    pub fn contracted(self, amount: i32, direction: Direction) -> Option<Self> {
        self.expanded(-amount, direction.opposite())
    }

    /// Move the selection without changing its size.
    pub fn shifted(self, amount: i32, direction: Direction) -> Option<Self> {
        let (dx, dy, dz) = direction.vector();
        let ox = dx.checked_mul(amount)?;
        let oy = dy.checked_mul(amount)?;
        let oz = dz.checked_mul(amount)?;
        Some(Self {
            min: BlockPos {
                x: self.min.x.checked_add(ox)?,
                y: self.min.y.checked_add(oy)?,
                z: self.min.z.checked_add(oz)?,
            },
            max: BlockPos {
                x: self.max.x.checked_add(ox)?,
                y: self.max.y.checked_add(oy)?,
                z: self.max.z.checked_add(oz)?,
            },
        })
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

/// Replace a player's current selection with a normalized region.
pub fn set_region(key: &str, region: Region) {
    with_selection_mut(key, |sel| {
        sel.pos1 = Some(region.min);
        sel.pos2 = Some(region.max);
    });
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

    #[test]
    fn parses_direction_aliases() {
        assert_eq!(Direction::parse("u"), Some(Direction::Up));
        assert_eq!(Direction::parse("north"), Some(Direction::North));
        assert_eq!(Direction::parse("bad"), None);
    }

    #[test]
    fn derives_direction_from_yaw_pitch() {
        assert_eq!(Direction::from_yaw_pitch(0.0, 0.0), Direction::South);
        assert_eq!(Direction::from_yaw_pitch(90.0, 0.0), Direction::West);
        assert_eq!(Direction::from_yaw_pitch(180.0, 0.0), Direction::North);
        assert_eq!(Direction::from_yaw_pitch(270.0, 0.0), Direction::East);
        assert_eq!(Direction::from_yaw_pitch(0.0, -80.0), Direction::Up);
        assert_eq!(Direction::from_yaw_pitch(0.0, 80.0), Direction::Down);
    }

    #[test]
    fn expands_contracts_and_shifts_regions() {
        let r = Region::new(at(0, 0, 0), at(2, 2, 2));
        let expanded = r.expanded(3, Direction::East).unwrap();
        assert_eq!((expanded.min.x, expanded.max.x), (0, 5));

        let contracted = r.contracted(1, Direction::Down).unwrap();
        assert_eq!((contracted.min.y, contracted.max.y), (0, 1));

        let shifted = r.shifted(4, Direction::North).unwrap();
        assert_eq!((shifted.min.z, shifted.max.z), (-4, -2));
    }

    #[test]
    fn rejecting_over_contract_keeps_region_valid() {
        let r = Region::new(at(0, 0, 0), at(2, 2, 2));
        assert!(r.contracted(2, Direction::Down).is_some());
        assert!(r.contracted(3, Direction::Down).is_none());
    }
}
