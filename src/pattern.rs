//! FAWE-style block patterns and the small mask subset used by commands.
//!
//! Pumpkin currently exposes block-state ids to this plugin, not full
//! WorldEdit extents, biome setters, or block entities inside the pattern
//! engine. This parser therefore supports the FAWE/WorldEdit patterns that can
//! be evaluated from `(position, existing_state)` plus a small evaluation
//! context for clipboard-backed patterns, and returns precise errors for
//! patterns that still need richer world context.

use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use pumpkin_data::{Block, BlockState, block_properties};
use pumpkin_plugin_api::{common::BlockPos, world::World};

use crate::{
    block_data::{BlockEntityData, BlockPlacement, SignBlockData, SignColor, SignFace},
    clipboard::{self, ClipboardBuffer},
    expression::CompiledExpression,
    mapping, simplex_noise, snbt,
};

#[derive(Clone, Debug)]
pub enum BlockPattern {
    Literal {
        input: String,
        placement: BlockPlacement,
    },
    Existing,
    Clipboard {
        input: String,
        kind: ClipboardPatternKind,
        offset: (i32, i32, i32),
    },
    Weighted {
        input: String,
        entries: Vec<WeightedBlock>,
        total: u32,
    },
    RandomStates {
        input: String,
        states: Vec<u16>,
    },
    TypeApply {
        input: String,
        pattern: Box<BlockPattern>,
    },
    StateApply {
        input: String,
        properties: String,
    },
    Offset {
        input: String,
        dx: i32,
        dy: i32,
        dz: i32,
        pattern: Box<BlockPattern>,
    },
    Spread {
        input: String,
        dx: i32,
        dy: i32,
        dz: i32,
        pattern: Box<BlockPattern>,
    },
    Buffer {
        input: String,
        pattern: Box<BlockPattern>,
    },
    Buffer2d {
        input: String,
        pattern: Box<BlockPattern>,
    },
    Relative {
        input: String,
        pattern: Box<BlockPattern>,
    },
    SurfaceSpread {
        input: String,
        distance: i32,
        pattern: Box<BlockPattern>,
    },
    SolidSpread {
        input: String,
        dx: i32,
        dy: i32,
        dz: i32,
        pattern: Box<BlockPattern>,
    },
    Mask {
        input: String,
        mask: BlockMask,
        true_pattern: Box<BlockPattern>,
        false_pattern: Box<BlockPattern>,
    },
    Simplex {
        input: String,
        inverse_scale: f64,
        pattern: Box<BlockPattern>,
    },
    Linear {
        input: String,
        cursor: Cell<usize>,
        patterns: Vec<BlockPattern>,
    },
    Linear2d {
        input: String,
        xscale: i32,
        zscale: i32,
        patterns: Vec<BlockPattern>,
    },
    Linear3d {
        input: String,
        xscale: i32,
        yscale: i32,
        zscale: i32,
        patterns: Vec<BlockPattern>,
    },
    AxisMask {
        input: String,
        x: bool,
        y: bool,
        z: bool,
        pattern: Box<BlockPattern>,
    },
    Color {
        input: String,
        kind: ColorPatternKind,
    },
    Expression {
        input: String,
        expression: CompiledExpression,
    },
}

#[derive(Clone, Debug)]
pub struct WeightedBlock {
    pub weight: u32,
    pub pattern: Box<BlockPattern>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardPatternKind {
    Clipboard,
    Copy,
    FullCopy,
}

impl ClipboardPatternKind {
    fn source_name(self) -> &'static str {
        match self {
            Self::Clipboard => "clipboard",
            Self::Copy => "copied",
            Self::FullCopy => "full-copy",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ColorPatternKind {
    Match { color: u32 },
    Saturate { color: u32 },
    Average { color: u32 },
    Desaturate { amount: f32 },
    Shade { darken: bool },
}

#[derive(Clone)]
pub struct PatternEvalContext {
    origin: BlockPos,
    clipboard: Option<PreparedClipboardPattern>,
    world: Option<*const World>,
    block_lookup: Option<Rc<dyn Fn(BlockPos) -> u16>>,
    sample_cache: Rc<RefCell<HashMap<(i32, i32, i32), u16>>>,
    runtime: Rc<RefCell<PatternRuntimeState>>,
    min_y: i32,
    max_y: i32,
    random_source: PatternRandomSource,
}

impl PatternEvalContext {
    pub fn new(origin: BlockPos) -> Self {
        Self {
            origin,
            clipboard: None,
            world: None,
            block_lookup: None,
            sample_cache: Rc::new(RefCell::new(HashMap::new())),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: DEFAULT_MIN_Y,
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    pub fn for_player(origin: BlockPos, key: &str) -> Self {
        Self {
            origin,
            clipboard: clipboard::get(key).and_then(PreparedClipboardPattern::from_buffer),
            world: None,
            block_lookup: None,
            sample_cache: Rc::new(RefCell::new(HashMap::new())),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: DEFAULT_MIN_Y,
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    pub fn for_operation(origin: BlockPos, key: &str, world: &World) -> Self {
        Self {
            origin,
            clipboard: clipboard::get(key).and_then(PreparedClipboardPattern::from_buffer),
            world: Some(world as *const World),
            block_lookup: None,
            sample_cache: Rc::new(RefCell::new(HashMap::new())),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: world.get_min_y(),
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    #[cfg(test)]
    pub fn with_clipboard(origin: BlockPos, buffer: ClipboardBuffer) -> Self {
        Self {
            origin,
            clipboard: PreparedClipboardPattern::from_buffer(buffer),
            world: None,
            block_lookup: None,
            sample_cache: Rc::new(RefCell::new(HashMap::new())),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: DEFAULT_MIN_Y,
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    #[cfg(test)]
    pub fn with_world_states(origin: BlockPos, blocks: &[((i32, i32, i32), u16)]) -> Self {
        let mut sample_cache = HashMap::new();
        for &(pos, state_id) in blocks {
            sample_cache.insert(pos, state_id);
        }
        Self {
            origin,
            clipboard: None,
            world: None,
            block_lookup: None,
            sample_cache: Rc::new(RefCell::new(sample_cache)),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: DEFAULT_MIN_Y,
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    #[cfg(test)]
    pub fn with_block_lookup(origin: BlockPos, lookup: Rc<dyn Fn(BlockPos) -> u16>) -> Self {
        Self {
            origin,
            clipboard: None,
            world: None,
            block_lookup: Some(lookup),
            sample_cache: Rc::new(RefCell::new(HashMap::new())),
            runtime: Rc::new(RefCell::new(PatternRuntimeState::default())),
            min_y: DEFAULT_MIN_Y,
            max_y: DEFAULT_MAX_Y,
            random_source: PatternRandomSource::PositionHash,
        }
    }

    fn with_random_source(&self, random_source: PatternRandomSource) -> Self {
        Self {
            origin: self.origin,
            clipboard: self.clipboard.clone(),
            world: self.world,
            block_lookup: self.block_lookup.clone(),
            sample_cache: Rc::clone(&self.sample_cache),
            runtime: Rc::clone(&self.runtime),
            min_y: self.min_y,
            max_y: self.max_y,
            random_source,
        }
    }

    fn weighted_pick(&self, pos: BlockPos, total: u32) -> u32 {
        self.random_source.weighted_pick(pos, total)
    }

    fn random_index(&self, pos: BlockPos, len: usize) -> usize {
        self.random_source.random_index(pos, len)
    }

    fn sample_before(&self, pos: BlockPos, fallback: u16) -> u16 {
        let key = (pos.x, pos.y, pos.z);
        if let Some(state_id) = self.sample_cache.borrow().get(&key).copied() {
            return state_id;
        }
        let state_id = self.sample_block_state(pos).unwrap_or(fallback);
        self.sample_cache.borrow_mut().insert(key, state_id);
        state_id
    }

    pub(crate) fn sample_block_state(&self, pos: BlockPos) -> Option<u16> {
        if let Some(state_id) = self
            .sample_cache
            .borrow()
            .get(&(pos.x, pos.y, pos.z))
            .copied()
        {
            return Some(state_id);
        }
        if let Some(lookup) = &self.block_lookup {
            return Some(lookup(pos));
        }
        let world = self.world?;
        if pos.y < self.min_y || pos.y > self.max_y {
            return Some(0);
        }
        // SAFETY: `for_operation` stores a non-owning pointer to the live
        // command/brush `World` handle and the context does not outlive that call.
        Some(unsafe { (&*world).get_block_state_id(pos) })
    }
}

impl Default for PatternEvalContext {
    fn default() -> Self {
        Self::new(BlockPos { x: 0, y: 0, z: 0 })
    }
}

#[derive(Clone)]
struct PreparedClipboardPattern {
    width: usize,
    height: usize,
    length: usize,
    blocks: Vec<u16>,
}

#[derive(Clone, Copy)]
enum PatternRandomSource {
    PositionHash,
    Simplex { inverse_scale: f64 },
}

#[derive(Clone, Default)]
struct PatternRuntimeState {
    seen_positions: HashSet<(i32, i32, i32)>,
    seen_columns: HashSet<(i32, i32)>,
}

const DEFAULT_MIN_Y: i32 = -64;
const DEFAULT_MAX_Y: i32 = 319;

impl PatternRandomSource {
    fn weighted_pick(self, pos: BlockPos, total: u32) -> u32 {
        match self {
            Self::PositionHash => position_hash(pos) % total,
            Self::Simplex { inverse_scale } => {
                bounded_pick(simplex_noise_unit(pos, inverse_scale), total)
            }
        }
    }

    fn random_index(self, pos: BlockPos, len: usize) -> usize {
        match self {
            Self::PositionHash => (position_hash(pos) as usize) % len,
            Self::Simplex { inverse_scale } => {
                bounded_index(simplex_noise_unit(pos, inverse_scale), len)
            }
        }
    }
}

impl PreparedClipboardPattern {
    fn from_buffer(buffer: ClipboardBuffer) -> Option<Self> {
        let mut min = None::<BlockPos>;
        let mut max = None::<BlockPos>;
        for &((x, y, z), _) in &buffer.blocks {
            let pos = BlockPos { x, y, z };
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

        let min = min?;
        let max = max?;
        let width = (max.x - min.x + 1) as usize;
        let height = (max.y - min.y + 1) as usize;
        let length = (max.z - min.z + 1) as usize;
        let mut blocks = vec![0; width * height * length];
        for &((x, y, z), state) in &buffer.blocks {
            let x = (x - min.x) as usize;
            let y = (y - min.y) as usize;
            let z = (z - min.z) as usize;
            let index = x + z * width + y * width * length;
            blocks[index] = state;
        }

        Some(Self {
            width,
            height,
            length,
            blocks,
        })
    }

    fn state_at(&self, pos: BlockPos, origin: BlockPos, offset: (i32, i32, i32)) -> u16 {
        let local_x = i64::from(pos.x) - i64::from(origin.x);
        let local_y = i64::from(pos.y) - i64::from(origin.y);
        let local_z = i64::from(pos.z) - i64::from(origin.z);
        let x = wrap_pattern_axis(local_x, offset.0, self.width);
        let y = wrap_pattern_axis(local_y, offset.1, self.height);
        let z = wrap_pattern_axis(local_z, offset.2, self.length);
        self.blocks[x + z * self.width + y * self.width * self.length]
    }
}

impl BlockPattern {
    pub fn parse(input: &str) -> Result<Self, String> {
        parse_pattern(input.trim())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn state_at(&self, pos: BlockPos, before: u16) -> u16 {
        self.state_at_with(pos, before, &PatternEvalContext::default())
    }

    pub fn state_at_with(&self, pos: BlockPos, before: u16, ctx: &PatternEvalContext) -> u16 {
        match self {
            Self::Literal { placement, .. } => placement.state_id,
            Self::Existing => before,
            Self::Clipboard { offset, .. } => ctx.clipboard.as_ref().map_or(before, |clipboard| {
                clipboard.state_at(pos, ctx.origin, *offset)
            }),
            Self::Weighted { entries, total, .. } => {
                let mut pick = ctx.weighted_pick(pos, *total);
                for entry in entries {
                    if pick < entry.weight {
                        return entry.pattern.state_at_with(pos, before, ctx);
                    }
                    pick -= entry.weight;
                }
                entries.last().map_or(before, |entry| {
                    entry.pattern.state_at_with(pos, before, ctx)
                })
            }
            Self::RandomStates { states, .. } => {
                let index = ctx.random_index(pos, states.len());
                states[index]
            }
            Self::TypeApply { pattern, .. } => {
                let target = pattern.state_at_with(pos, before, ctx);
                mapping::apply_existing_states(target, before).unwrap_or(target)
            }
            Self::StateApply { properties, .. } => {
                mapping::apply_state_properties(before, properties).unwrap_or(before)
            }
            Self::Buffer { pattern, .. } => {
                let key = (pos.x, pos.y, pos.z);
                if !ctx.runtime.borrow_mut().seen_positions.insert(key) {
                    return before;
                }
                pattern.state_at_with(pos, before, ctx)
            }
            Self::Buffer2d { pattern, .. } => {
                let key = (pos.x, pos.z);
                if !ctx.runtime.borrow_mut().seen_columns.insert(key) {
                    return before;
                }
                pattern.state_at_with(pos, before, ctx)
            }
            Self::Relative { pattern, .. } => {
                let relative = BlockPos {
                    x: pos.x - ctx.origin.x,
                    y: pos.y - ctx.origin.y,
                    z: pos.z - ctx.origin.z,
                };
                let relative_before = ctx.sample_before(relative, before);
                pattern.state_at_with(relative, relative_before, ctx)
            }
            Self::Offset {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let shifted = BlockPos {
                    x: pos.x + dx,
                    y: pos.y + dy,
                    z: pos.z + dz,
                };
                let shifted_before = ctx.sample_before(shifted, before);
                pattern.state_at_with(shifted, shifted_before, ctx)
            }
            Self::Spread {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let hash = position_hash(pos);
                let shifted = BlockPos {
                    x: pos.x + spread_axis(hash, 0, *dx),
                    y: pos.y + spread_axis(hash, 10, *dy),
                    z: pos.z + spread_axis(hash, 20, *dz),
                };
                let shifted_before = ctx.sample_before(shifted, before);
                pattern.state_at_with(shifted, shifted_before, ctx)
            }
            Self::SurfaceSpread {
                distance, pattern, ..
            } => {
                let shifted = surface_spread_target(pos, *distance, before, pattern, ctx);
                let shifted_before = ctx.sample_before(shifted, before);
                pattern.state_at_with(shifted, shifted_before, ctx)
            }
            Self::SolidSpread {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let shifted = BlockPos {
                    x: pos.x + spread_axis(position_hash(pos), 0, *dx),
                    y: pos.y + spread_axis(position_hash(pos), 10, *dy),
                    z: pos.z + spread_axis(position_hash(pos), 20, *dz),
                };
                let shifted_before = ctx.sample_before(shifted, before);
                let shifted_state = pattern.state_at_with(shifted, shifted_before, ctx);
                if state_is_solid(shifted_state) {
                    shifted_state
                } else {
                    pattern.state_at_with(pos, before, ctx)
                }
            }
            Self::Mask {
                mask,
                true_pattern,
                false_pattern,
                ..
            } => {
                if mask.matches(before) {
                    true_pattern.state_at_with(pos, before, ctx)
                } else {
                    false_pattern.state_at_with(pos, before, ctx)
                }
            }
            Self::Simplex {
                inverse_scale,
                pattern,
                ..
            } => pattern.state_at_with(
                pos,
                before,
                &ctx.with_random_source(PatternRandomSource::Simplex {
                    inverse_scale: *inverse_scale,
                }),
            ),
            Self::Linear {
                cursor, patterns, ..
            } => {
                let index = cursor.get() % patterns.len();
                cursor.set(cursor.get().wrapping_add(1));
                patterns[index].state_at_with(pos, before, ctx)
            }
            Self::Linear2d {
                xscale,
                zscale,
                patterns,
                ..
            } => {
                let index = (pos.x.div_euclid(*xscale) + pos.z.div_euclid(*zscale))
                    .rem_euclid(patterns.len() as i32) as usize;
                patterns[index].state_at_with(pos, before, ctx)
            }
            Self::Linear3d {
                xscale,
                yscale,
                zscale,
                patterns,
                ..
            } => {
                let index = (pos.x.div_euclid(*xscale)
                    + pos.y.div_euclid(*yscale)
                    + pos.z.div_euclid(*zscale))
                .rem_euclid(patterns.len() as i32) as usize;
                patterns[index].state_at_with(pos, before, ctx)
            }
            Self::AxisMask {
                x, y, z, pattern, ..
            } => {
                let masked = BlockPos {
                    x: if *x { pos.x } else { 0 },
                    y: if *y { pos.y } else { 0 },
                    z: if *z { pos.z } else { 0 },
                };
                let masked_before = ctx.sample_before(masked, before);
                pattern.state_at_with(masked, masked_before, ctx)
            }
            Self::Color { kind, .. } => match kind {
                ColorPatternKind::Match { color } => {
                    mapping::nearest_color_block(*color).unwrap_or(before)
                }
                ColorPatternKind::Saturate { color } => {
                    mapping::saturate_existing_block(before, *color).unwrap_or(before)
                }
                ColorPatternKind::Average { color } => {
                    mapping::average_existing_block(before, *color).unwrap_or(before)
                }
                ColorPatternKind::Desaturate { amount } => {
                    mapping::desaturate_existing_block(before, *amount).unwrap_or(before)
                }
                ColorPatternKind::Shade { darken } => {
                    mapping::shade_existing_block(before, *darken).unwrap_or(before)
                }
            },
            Self::Expression { expression, .. } => expression
                .evaluate(pos, before, ctx)
                .map(expression_result_to_state_id)
                .unwrap_or(0),
        }
    }

    pub fn placement_at_with(
        &self,
        pos: BlockPos,
        before: &BlockPlacement,
        ctx: &PatternEvalContext,
    ) -> BlockPlacement {
        match self {
            Self::Literal { placement, .. } => placement.clone(),
            Self::Existing => before.clone(),
            Self::Clipboard { offset, .. } => ctx.clipboard.as_ref().map_or_else(
                || before.clone(),
                |clipboard| BlockPlacement::new(clipboard.state_at(pos, ctx.origin, *offset)),
            ),
            Self::Weighted { entries, total, .. } => {
                let mut pick = ctx.weighted_pick(pos, *total);
                for entry in entries {
                    if pick < entry.weight {
                        return entry.pattern.placement_at_with(pos, before, ctx);
                    }
                    pick -= entry.weight;
                }
                entries.last().map_or_else(
                    || before.clone(),
                    |entry| entry.pattern.placement_at_with(pos, before, ctx),
                )
            }
            Self::RandomStates { states, .. } => {
                let index = ctx.random_index(pos, states.len());
                BlockPlacement::new(states[index])
            }
            Self::TypeApply { pattern, .. } => {
                let mut target = pattern.placement_at_with(pos, before, ctx);
                target.state_id = mapping::apply_existing_states(target.state_id, before.state_id)
                    .unwrap_or(target.state_id);
                target
            }
            Self::StateApply { properties, .. } => {
                let mut target = before.clone();
                target.state_id = mapping::apply_state_properties(before.state_id, properties)
                    .unwrap_or(before.state_id);
                target
            }
            Self::Buffer { pattern, .. } => {
                let state_id = self.state_at_with(pos, before.state_id, ctx);
                let mut placement = pattern.placement_at_with(pos, before, ctx);
                placement.state_id = state_id;
                placement
            }
            Self::Buffer2d { pattern, .. } => {
                let state_id = self.state_at_with(pos, before.state_id, ctx);
                let mut placement = pattern.placement_at_with(pos, before, ctx);
                placement.state_id = state_id;
                placement
            }
            Self::Relative { pattern, .. } => {
                let relative = BlockPos {
                    x: pos.x - ctx.origin.x,
                    y: pos.y - ctx.origin.y,
                    z: pos.z - ctx.origin.z,
                };
                let relative_before = ctx.sample_before(relative, before.state_id);
                pattern.placement_at_with(relative, &BlockPlacement::new(relative_before), ctx)
            }
            Self::Offset {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let shifted = BlockPos {
                    x: pos.x + dx,
                    y: pos.y + dy,
                    z: pos.z + dz,
                };
                let shifted_before = ctx.sample_before(shifted, before.state_id);
                pattern.placement_at_with(shifted, &BlockPlacement::new(shifted_before), ctx)
            }
            Self::Spread {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let hash = position_hash(pos);
                let shifted = BlockPos {
                    x: pos.x + spread_axis(hash, 0, *dx),
                    y: pos.y + spread_axis(hash, 10, *dy),
                    z: pos.z + spread_axis(hash, 20, *dz),
                };
                let shifted_before = ctx.sample_before(shifted, before.state_id);
                pattern.placement_at_with(shifted, &BlockPlacement::new(shifted_before), ctx)
            }
            Self::SurfaceSpread {
                distance, pattern, ..
            } => {
                let shifted = surface_spread_target(pos, *distance, before.state_id, pattern, ctx);
                let shifted_before = ctx.sample_before(shifted, before.state_id);
                pattern.placement_at_with(shifted, &BlockPlacement::new(shifted_before), ctx)
            }
            Self::SolidSpread {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let shifted = BlockPos {
                    x: pos.x + spread_axis(position_hash(pos), 0, *dx),
                    y: pos.y + spread_axis(position_hash(pos), 10, *dy),
                    z: pos.z + spread_axis(position_hash(pos), 20, *dz),
                };
                let shifted_before = ctx.sample_before(shifted, before.state_id);
                let shifted_placement =
                    pattern.placement_at_with(shifted, &BlockPlacement::new(shifted_before), ctx);
                if state_is_solid(shifted_placement.state_id) {
                    shifted_placement
                } else {
                    pattern.placement_at_with(pos, before, ctx)
                }
            }
            Self::Mask {
                mask,
                true_pattern,
                false_pattern,
                ..
            } => {
                if mask.matches(before.state_id) {
                    true_pattern.placement_at_with(pos, before, ctx)
                } else {
                    false_pattern.placement_at_with(pos, before, ctx)
                }
            }
            Self::Simplex {
                inverse_scale,
                pattern,
                ..
            } => pattern.placement_at_with(
                pos,
                before,
                &ctx.with_random_source(PatternRandomSource::Simplex {
                    inverse_scale: *inverse_scale,
                }),
            ),
            Self::Linear {
                cursor, patterns, ..
            } => {
                let index = cursor.get() % patterns.len();
                cursor.set(cursor.get().wrapping_add(1));
                patterns[index].placement_at_with(pos, before, ctx)
            }
            Self::Linear2d {
                xscale,
                zscale,
                patterns,
                ..
            } => {
                let index = (pos.x.div_euclid(*xscale) + pos.z.div_euclid(*zscale))
                    .rem_euclid(patterns.len() as i32) as usize;
                patterns[index].placement_at_with(pos, before, ctx)
            }
            Self::Linear3d {
                xscale,
                yscale,
                zscale,
                patterns,
                ..
            } => {
                let index = (pos.x.div_euclid(*xscale)
                    + pos.y.div_euclid(*yscale)
                    + pos.z.div_euclid(*zscale))
                .rem_euclid(patterns.len() as i32) as usize;
                patterns[index].placement_at_with(pos, before, ctx)
            }
            Self::AxisMask {
                x, y, z, pattern, ..
            } => {
                let masked = BlockPos {
                    x: if *x { pos.x } else { 0 },
                    y: if *y { pos.y } else { 0 },
                    z: if *z { pos.z } else { 0 },
                };
                let masked_before = ctx.sample_before(masked, before.state_id);
                pattern.placement_at_with(masked, &BlockPlacement::new(masked_before), ctx)
            }
            Self::Color { .. } => {
                BlockPlacement::new(self.state_at_with(pos, before.state_id, ctx))
            }
            Self::Expression { expression, .. } => BlockPlacement::new(
                expression
                    .evaluate(pos, before.state_id, ctx)
                    .map(expression_result_to_state_id)
                    .unwrap_or(0),
            ),
        }
    }

    pub fn validate(&self, ctx: &PatternEvalContext) -> Result<(), String> {
        match self {
            Self::Literal { .. }
            | Self::Existing
            | Self::RandomStates { .. }
            | Self::StateApply { .. } => Ok(()),
            Self::Color { input, .. } => {
                if mapping::has_color_palette() {
                    Ok(())
                } else {
                    Err(format!(
                        "Pattern '{input}' requires the bundled block color palette from assets/blocks.json."
                    ))
                }
            }
            Self::Expression { input, expression } => {
                if expression.uses_world_queries()
                    && ctx.world.is_none()
                    && ctx.block_lookup.is_none()
                {
                    Err(format!(
                        "Pattern '{input}' uses world-query functions, which are not available in this command context."
                    ))
                } else {
                    Ok(())
                }
            }
            Self::Clipboard { input, kind, .. } => {
                if ctx.clipboard.is_some() {
                    Ok(())
                } else {
                    Err(format!(
                        "Pattern '{input}' requires a non-empty {} clipboard. Use //copy first.",
                        kind.source_name()
                    ))
                }
            }
            Self::Weighted { entries, .. } => {
                for entry in entries {
                    entry.pattern.validate(ctx)?;
                }
                Ok(())
            }
            Self::TypeApply { pattern, .. }
            | Self::Buffer { pattern, .. }
            | Self::Buffer2d { pattern, .. }
            | Self::Relative { pattern, .. }
            | Self::Offset { pattern, .. }
            | Self::Spread { pattern, .. }
            | Self::SurfaceSpread { pattern, .. }
            | Self::SolidSpread { pattern, .. }
            | Self::Simplex { pattern, .. }
            | Self::AxisMask { pattern, .. } => pattern.validate(ctx),
            Self::Mask {
                true_pattern,
                false_pattern,
                ..
            } => {
                true_pattern.validate(ctx)?;
                false_pattern.validate(ctx)
            }
            Self::Linear { patterns, .. }
            | Self::Linear2d { patterns, .. }
            | Self::Linear3d { patterns, .. } => {
                for pattern in patterns {
                    pattern.validate(ctx)?;
                }
                Ok(())
            }
        }
    }

    pub fn literal_display(&self) -> Option<(&str, u16)> {
        match self {
            Self::Literal { input, placement } => Some((input, placement.state_id)),
            _ => None,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Literal { input, .. }
            | Self::Clipboard { input, .. }
            | Self::Weighted { input, .. }
            | Self::RandomStates { input, .. }
            | Self::TypeApply { input, .. }
            | Self::StateApply { input, .. }
            | Self::Buffer { input, .. }
            | Self::Buffer2d { input, .. }
            | Self::Relative { input, .. }
            | Self::Offset { input, .. }
            | Self::Spread { input, .. }
            | Self::SurfaceSpread { input, .. }
            | Self::SolidSpread { input, .. }
            | Self::Mask { input, .. }
            | Self::Simplex { input, .. }
            | Self::Linear { input, .. }
            | Self::Linear2d { input, .. }
            | Self::Linear3d { input, .. }
            | Self::AxisMask { input, .. }
            | Self::Color { input, .. }
            | Self::Expression { input, .. } => input,
            Self::Existing => "#existing",
        }
    }
}

fn surface_spread_target(
    pos: BlockPos,
    distance: i32,
    before: u16,
    pattern: &BlockPattern,
    ctx: &PatternEvalContext,
) -> BlockPos {
    let moves = distance.clamp(0, 255) as usize;
    let mut current = pos;
    for step in 0..moves {
        let mut candidates = Vec::new();
        for (dx, dy, dz) in diagonal_directions() {
            let next = BlockPos {
                x: current.x + dx,
                y: current.y + dy,
                z: current.z + dz,
            };
            if surface_spread_allowed(next, before, pattern, ctx) {
                candidates.push(next);
            }
        }
        if candidates.is_empty() {
            break;
        }
        let index = spread_choice_index(pos, step, candidates.len());
        current = candidates[index];
    }
    current
}

fn surface_spread_allowed(
    pos: BlockPos,
    fallback_before: u16,
    pattern: &BlockPattern,
    ctx: &PatternEvalContext,
) -> bool {
    let before = ctx.sample_before(pos, fallback_before);
    let state = pattern.state_at_with(pos, before, ctx);
    if !state_blocks_movement(state) {
        return false;
    }

    for neighbor in [
        BlockPos {
            x: pos.x,
            y: pos.y + 1,
            z: pos.z,
        },
        BlockPos {
            x: pos.x,
            y: pos.y - 1,
            z: pos.z,
        },
        BlockPos {
            x: pos.x + 1,
            y: pos.y,
            z: pos.z,
        },
        BlockPos {
            x: pos.x - 1,
            y: pos.y,
            z: pos.z,
        },
        BlockPos {
            x: pos.x,
            y: pos.y,
            z: pos.z + 1,
        },
        BlockPos {
            x: pos.x,
            y: pos.y,
            z: pos.z - 1,
        },
    ] {
        if neighbor.y < ctx.min_y || neighbor.y > ctx.max_y {
            continue;
        }
        let neighbor_before = ctx.sample_before(neighbor, fallback_before);
        let neighbor_state = pattern.state_at_with(neighbor, neighbor_before, ctx);
        if !state_blocks_movement(neighbor_state) {
            return true;
        }
    }
    false
}

fn state_is_solid(state_id: u16) -> bool {
    BlockState::from_id(state_id).is_solid()
}

fn state_blocks_movement(state_id: u16) -> bool {
    let state = BlockState::from_id(state_id);
    block_properties::blocks_movement(state, Block::get_raw_id_from_state_id(state_id))
}

fn diagonal_directions() -> &'static [(i32, i32, i32)] {
    &[
        (-1, -1, -1),
        (0, -1, -1),
        (1, -1, -1),
        (-1, 0, -1),
        (0, 0, -1),
        (1, 0, -1),
        (-1, 1, -1),
        (0, 1, -1),
        (1, 1, -1),
        (-1, -1, 0),
        (0, -1, 0),
        (1, -1, 0),
        (-1, 0, 0),
        (1, 0, 0),
        (-1, 1, 0),
        (0, 1, 0),
        (1, 1, 0),
        (-1, -1, 1),
        (0, -1, 1),
        (1, -1, 1),
        (-1, 0, 1),
        (0, 0, 1),
        (1, 0, 1),
        (-1, 1, 1),
        (0, 1, 1),
        (1, 1, 1),
    ]
}

fn spread_choice_index(pos: BlockPos, step: usize, len: usize) -> usize {
    let mixed = position_hash(pos)
        .wrapping_add((step as u32).wrapping_mul(0x9e37_79b9))
        .rotate_left((step % 31) as u32);
    (mixed as usize) % len
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockMask {
    States(Vec<u16>),
    Any(Vec<BlockMask>),
    Not(Box<BlockMask>),
    Existing,
    Air,
}

impl BlockMask {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Expected a mask after -m.".to_string());
        }

        let entries = split_top_level(input, ',')?;
        if entries.len() > 1 {
            return Ok(Self::Any(
                entries
                    .into_iter()
                    .map(BlockMask::parse)
                    .collect::<Result<Vec<_>, _>>()?,
            ));
        }

        if let Some(inner) = input.strip_prefix('!') {
            return Ok(Self::Not(Box::new(Self::parse(inner)?)));
        }

        match input.to_ascii_lowercase().as_str() {
            "#existing" | "#solid" => return Ok(Self::Existing),
            "#air" => return Ok(Self::Air),
            _ => {}
        }

        if input.starts_with('#') || input.contains('%') {
            return Err(format!(
                "Mask '{input}' needs FAWE's full mask parser or world context, which is not implemented yet."
            ));
        }

        let Some(state_id) = mapping::resolve_block(input) else {
            return Err(format!("Unknown block '{input}'."));
        };
        Ok(Self::States(vec![state_id]))
    }

    pub fn matches(&self, state_id: u16) -> bool {
        match self {
            Self::States(states) => states.contains(&state_id),
            Self::Any(masks) => masks.iter().any(|mask| mask.matches(state_id)),
            Self::Not(mask) => !mask.matches(state_id),
            Self::Existing => state_id != 0,
            Self::Air => state_id == 0,
        }
    }
}

fn parse_pattern(input: &str) -> Result<BlockPattern, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Expected a block pattern.".to_string());
    }

    let entries = split_top_level(input, ',')?;
    if entries.len() > 1 {
        return parse_weighted(input, entries);
    }
    if find_top_level_percent(input)?.is_some() {
        return parse_weighted(input, entries);
    }

    if input.eq_ignore_ascii_case("#existing") {
        return Ok(BlockPattern::Existing);
    }

    if let Some(rest) = input.strip_prefix('=') {
        return parse_expression_pattern(input, rest);
    }

    if let Some(rest) = input.strip_prefix('^') {
        return parse_type_or_state_apply(input, rest.trim());
    }

    if let Some(rest) = input.strip_prefix("##") {
        return parse_tag_pattern(input, rest);
    }

    if let Some(rest) = input.strip_prefix('*') {
        return parse_random_state_pattern(input, rest);
    }

    if input.starts_with('#') {
        return parse_fawe_pattern(input);
    }

    parse_literal_pattern(input)
}

fn parse_type_or_state_apply(input: &str, rest: &str) -> Result<BlockPattern, String> {
    if rest.is_empty() {
        return Err(format!(
            "Missing type/state pattern after '^' in '{input}'."
        ));
    }

    if let Some(properties) = single_wrapped_arg(rest) {
        return Ok(BlockPattern::StateApply {
            input: input.to_string(),
            properties: properties.to_string(),
        });
    }

    Ok(BlockPattern::TypeApply {
        input: input.to_string(),
        pattern: Box::new(parse_pattern(rest)?),
    })
}

fn parse_expression_pattern(input: &str, rest: &str) -> Result<BlockPattern, String> {
    let expression_input = rest.trim();
    if expression_input.is_empty() {
        return Err(format!(
            "Missing expression after '=' in pattern '{input}'."
        ));
    }
    Ok(BlockPattern::Expression {
        input: input.to_string(),
        expression: CompiledExpression::compile(expression_input)?,
    })
}

fn parse_tag_pattern(input: &str, rest: &str) -> Result<BlockPattern, String> {
    let (all_states, tag) = rest
        .strip_prefix('*')
        .map_or((false, rest), |tag| (true, tag));
    let states = mapping::state_ids_for_tag(tag, all_states);
    if states.is_empty() {
        return Err(format!("Unknown or empty block category '##{rest}'."));
    }
    Ok(BlockPattern::RandomStates {
        input: input.to_string(),
        states,
    })
}

fn parse_random_state_pattern(input: &str, rest: &str) -> Result<BlockPattern, String> {
    let states = mapping::state_ids_for_block(rest);
    if states.is_empty() {
        return Err(format!("Unknown block '{rest}'."));
    }
    Ok(BlockPattern::RandomStates {
        input: input.to_string(),
        states,
    })
}

fn parse_literal_pattern(input: &str) -> Result<BlockPattern, String> {
    let parts = split_top_level(input, '|')?;
    let base = parts[0].trim();
    let (block_input, inline_snbt) = split_literal_block_and_nbt(base)?;
    let Some(state_id) = mapping::resolve_block(&block_input) else {
        return Err(format!("Unknown block '{block_input}'."));
    };

    let mut placement = BlockPlacement::new(state_id);
    let block_name = mapping::palette_key_for_state_id(state_id);
    let block_name = block_name
        .split_once('[')
        .map_or(block_name.as_str(), |(name, _)| name);

    if let Some(raw_snbt) = inline_snbt.as_ref() {
        apply_literal_snbt(block_name, &mut placement, &raw_snbt)?;
    }

    if parts.len() > 1 {
        if parts[1].trim_start().starts_with('{') {
            if inline_snbt.is_some() {
                return Err(format!(
                    "Pattern '{input}' cannot combine inline SNBT and pipe SNBT."
                ));
            }
            apply_literal_snbt(block_name, &mut placement, &parts[1..].join("|"))?;
        } else {
            apply_literal_pipe_syntax(block_name, &mut placement, &parts[1..], input)?;
        }
    }

    Ok(BlockPattern::Literal {
        input: input.to_string(),
        placement,
    })
}

fn parse_pattern_color(input: &str, args: &[String], name: &str) -> Result<u32, String> {
    if !(args.len() == 3 || args.len() == 4) {
        return Err(format!("Usage: {name} <r> <g> <b> [a]."));
    }

    let red = parse_color_component(&args[0], "red", input)?;
    let green = parse_color_component(&args[1], "green", input)?;
    let blue = parse_color_component(&args[2], "blue", input)?;
    let _alpha = args
        .get(3)
        .map(|raw| parse_color_component(raw, "alpha", input))
        .transpose()?
        .unwrap_or(255);

    Ok((255u32 << 24) | ((red as u32) << 16) | ((green as u32) << 8) | blue as u32)
}

fn parse_desaturate_amount(input: &str, args: &[String]) -> Result<f32, String> {
    let Some(raw) = args.first() else {
        return Err("Usage: #desaturate <percent>.".to_string());
    };
    if args.len() != 1 {
        return Err(format!(
            "Usage: #desaturate <percent> in pattern '{input}'."
        ));
    }
    let percent = raw
        .parse::<f32>()
        .map_err(|_| format!("Invalid percent '{raw}' in pattern '{input}'."))?;
    Ok((percent / 100.0).clamp(0.0, 1.0))
}

fn parse_color_component(raw: &str, name: &str, input: &str) -> Result<u8, String> {
    let value = raw
        .parse::<i32>()
        .map_err(|_| format!("Invalid {name} '{raw}' in pattern '{input}'."))?;
    Ok(value.clamp(0, 255) as u8)
}

fn ensure_no_pattern_args(input: &str, args: &[String], name: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Pattern '{input}' does not take arguments. Usage: {name}."
        ))
    }
}

fn split_literal_block_and_nbt(input: &str) -> Result<(String, Option<String>), String> {
    let mut split = None;
    let mut scan = ScanState::default();
    for (index, ch) in input.char_indices() {
        if ch == '{' && scan.is_top_level() {
            split = Some(index);
            break;
        }
        scan.advance(ch)?;
    }

    match split {
        Some(index) => Ok((
            input[..index].trim().to_string(),
            Some(input[index..].trim().to_string()),
        )),
        None => Ok((input.trim().to_string(), None)),
    }
}

fn apply_literal_pipe_syntax(
    block_name: &str,
    placement: &mut BlockPlacement,
    args: &[&str],
    input: &str,
) -> Result<(), String> {
    if is_sign_block(block_name) {
        let mut sign = sign_data_from_placement(placement);
        sign.front = SignFace::from_lines(
            &args
                .iter()
                .take(4)
                .map(|line| (*line).to_string())
                .collect::<Vec<_>>(),
        );
        placement.block_entity = Some(BlockEntityData::Sign(sign));
        return Ok(());
    }
    if is_player_head_block(block_name) {
        return Err(format!(
            "Pattern '{input}' uses player head syntax, but Pumpkin does not expose profile setters yet."
        ));
    }
    if is_spawner_block(block_name) {
        return Err(format!(
            "Pattern '{input}' uses spawner syntax, but Pumpkin does not expose spawner-type setters yet."
        ));
    }
    Err(format!(
        "Pattern '{input}' does not support pipe data for block '{block_name}'."
    ))
}

fn apply_literal_snbt(
    block_name: &str,
    placement: &mut BlockPlacement,
    raw_snbt: &str,
) -> Result<(), String> {
    let parsed = snbt::parse(raw_snbt)?;
    let Some(compound) = parsed.as_compound() else {
        return Err("Block SNBT must be a compound like '{key:value}'.".to_string());
    };

    if is_sign_block(block_name) {
        let sign = parse_sign_snbt(sign_data_from_placement(placement), compound)?;
        placement.block_entity = Some(BlockEntityData::Sign(sign));
        return Ok(());
    }

    if is_player_head_block(block_name) {
        return Err(format!(
            "Block SNBT for '{block_name}' needs skull/profile setters Pumpkin does not expose yet."
        ));
    }

    if is_spawner_block(block_name) {
        return Err(format!(
            "Block SNBT for '{block_name}' needs spawner mutation hooks Pumpkin does not expose yet."
        ));
    }

    Err(format!(
        "Block SNBT for '{block_name}' is not writable yet because Pumpkin only exposes sign block-entity setters."
    ))
}

fn sign_data_from_placement(placement: &BlockPlacement) -> SignBlockData {
    match &placement.block_entity {
        Some(BlockEntityData::Sign(sign)) => sign.clone(),
        _ => SignBlockData::default(),
    }
}

fn parse_sign_snbt(
    mut sign: SignBlockData,
    compound: &std::collections::BTreeMap<String, snbt::SnbtValue>,
) -> Result<SignBlockData, String> {
    for (key, value) in compound {
        match key.as_str() {
            "is_waxed" => {
                sign.waxed = value
                    .as_bool_loose()
                    .ok_or_else(|| "sign 'is_waxed' must be a boolean or 0/1.".to_string())?;
            }
            "front_text" => {
                let Some(front) = value.as_compound() else {
                    return Err("sign 'front_text' must be an SNBT compound.".to_string());
                };
                sign.front = parse_sign_face(sign.front.clone(), front)?;
            }
            "back_text" => {
                let Some(back) = value.as_compound() else {
                    return Err("sign 'back_text' must be an SNBT compound.".to_string());
                };
                sign.back = parse_sign_face(sign.back.clone(), back)?;
            }
            other => {
                return Err(format!(
                    "Unsupported sign SNBT field '{other}'. Pumpkin currently supports only is_waxed, front_text, and back_text."
                ));
            }
        }
    }
    Ok(sign)
}

fn parse_sign_face(
    mut face: SignFace,
    compound: &std::collections::BTreeMap<String, snbt::SnbtValue>,
) -> Result<SignFace, String> {
    for (key, value) in compound {
        match key.as_str() {
            "messages" => {
                let Some(messages) = value.as_list() else {
                    return Err("sign text 'messages' must be an SNBT list.".to_string());
                };
                for slot in &mut face.messages {
                    slot.clear();
                }
                for (index, message) in messages.iter().take(4).enumerate() {
                    let Some(message) = message.as_str() else {
                        return Err("sign text messages must be strings.".to_string());
                    };
                    face.messages[index] = message.to_string();
                }
            }
            "color" => {
                let Some(color) = value.as_str() else {
                    return Err("sign text 'color' must be a string.".to_string());
                };
                face.color = SignColor::parse(color)
                    .ok_or_else(|| format!("Unknown sign color '{color}'."))?;
            }
            "has_glowing_text" => {
                face.glowing = value.as_bool_loose().ok_or_else(|| {
                    "sign text 'has_glowing_text' must be a boolean or 0/1.".to_string()
                })?;
            }
            other => {
                return Err(format!(
                    "Unsupported sign text SNBT field '{other}'. Supported fields are messages, color, and has_glowing_text."
                ));
            }
        }
    }
    Ok(face)
}

fn is_sign_block(block_name: &str) -> bool {
    block_name.ends_with("_sign") || block_name.ends_with("_hanging_sign")
}

fn is_player_head_block(block_name: &str) -> bool {
    matches!(
        block_name,
        "minecraft:player_head" | "minecraft:player_wall_head"
    )
}

fn is_spawner_block(block_name: &str) -> bool {
    block_name == "minecraft:spawner"
}

fn parse_fawe_pattern(input: &str) -> Result<BlockPattern, String> {
    if let Some(pattern) = parse_clipboard_alias_pattern(input)? {
        return Ok(pattern);
    }

    let (name, args) = parse_prefixed_call(input)?;
    match name.as_str() {
        "#offset" => {
            let (dx, dy, dz, pattern) = parse_offset_like_args(input, &args, "#offset")?;
            Ok(BlockPattern::Offset {
                input: input.to_string(),
                dx,
                dy,
                dz,
                pattern: Box::new(parse_pattern(&pattern)?),
            })
        }
        "#spread" => {
            let (dx, dy, dz, pattern) = parse_offset_like_args(input, &args, "#spread")?;
            Ok(BlockPattern::Spread {
                input: input.to_string(),
                dx,
                dy,
                dz,
                pattern: Box::new(parse_pattern(&pattern)?),
            })
        }
        "#buffer" => Ok(BlockPattern::Buffer {
            input: input.to_string(),
            pattern: Box::new(parse_single_child_pattern(input, &args, "#buffer")?),
        }),
        "#buffer2d" => Ok(BlockPattern::Buffer2d {
            input: input.to_string(),
            pattern: Box::new(parse_single_child_pattern(input, &args, "#buffer2d")?),
        }),
        "#relative" | "#~" | "#r" | "#rel" => Ok(BlockPattern::Relative {
            input: input.to_string(),
            pattern: Box::new(parse_single_child_pattern(input, &args, "#relative")?),
        }),
        "#surfacespread" => {
            let (distance, pattern) = parse_surface_spread_args(input, &args)?;
            Ok(BlockPattern::SurfaceSpread {
                input: input.to_string(),
                distance,
                pattern: Box::new(parse_pattern(&pattern)?),
            })
        }
        "#solidspread" => {
            let (dx, dy, dz, pattern) = parse_solid_spread_args(input, &args)?;
            Ok(BlockPattern::SolidSpread {
                input: input.to_string(),
                dx,
                dy,
                dz,
                pattern: Box::new(parse_pattern(&pattern)?),
            })
        }
        "#linear" => Ok(BlockPattern::Linear {
            input: input.to_string(),
            cursor: Cell::new(0),
            patterns: parse_sequence_arg(input, &args, 0)?,
        }),
        "#linear2d" => Ok(BlockPattern::Linear2d {
            input: input.to_string(),
            xscale: parse_optional_scale(&args, 1, "xscale")?,
            zscale: parse_optional_scale(&args, 2, "zscale")?,
            patterns: parse_sequence_arg(input, &args, 0)?,
        }),
        "#linear3d" => Ok(BlockPattern::Linear3d {
            input: input.to_string(),
            xscale: parse_optional_scale(&args, 1, "xscale")?,
            yscale: parse_optional_scale(&args, 2, "yscale")?,
            zscale: parse_optional_scale(&args, 3, "zscale")?,
            patterns: parse_sequence_arg(input, &args, 0)?,
        }),
        "#mask" => {
            if args.len() < 3 {
                return Err(format!(
                    "Usage: #mask <mask> <pattern-true> <pattern-false>."
                ));
            }
            Ok(BlockPattern::Mask {
                input: input.to_string(),
                mask: BlockMask::parse(&args[0])?,
                true_pattern: Box::new(parse_pattern(&args[1])?),
                false_pattern: Box::new(parse_pattern(&args[2])?),
            })
        }
        "#!x" | "#!y" | "#!z" => {
            if args.is_empty() {
                return Err(format!("Missing child pattern for '{name}'."));
            }
            Ok(BlockPattern::AxisMask {
                input: input.to_string(),
                x: name != "#!x",
                y: name != "#!y",
                z: name != "#!z",
                pattern: Box::new(parse_pattern(&args[0])?),
            })
        }
        "#simplex" => {
            if args.is_empty() {
                return Err(format!("Missing child pattern for '{input}'."));
            }
            let (inverse_scale, pattern_index) = if args.len() >= 2 {
                (parse_simplex_scale(&args[0], input)?, 1)
            } else {
                (1.0 / 10.0, 0)
            };
            Ok(BlockPattern::Simplex {
                input: input.to_string(),
                inverse_scale,
                pattern: Box::new(parse_pattern(&args[pattern_index])?),
            })
        }
        "#biome" => Err(format!(
            "Pattern '{input}' needs biome editing support, but Pumpkin only exposes \
             world.get-biome today and does not provide world.set-biome yet."
        )),
        "#color" | "#colour" => Ok(BlockPattern::Color {
            input: input.to_string(),
            kind: ColorPatternKind::Match {
                color: parse_pattern_color(input, &args, "#color")?,
            },
        }),
        "#saturate" => Ok(BlockPattern::Color {
            input: input.to_string(),
            kind: ColorPatternKind::Saturate {
                color: parse_pattern_color(input, &args, "#saturate")?,
            },
        }),
        "#averagecolor" | "#averagecolour" => Ok(BlockPattern::Color {
            input: input.to_string(),
            kind: ColorPatternKind::Average {
                color: parse_pattern_color(input, &args, "#averagecolor")?,
            },
        }),
        "#desaturate" => Ok(BlockPattern::Color {
            input: input.to_string(),
            kind: ColorPatternKind::Desaturate {
                amount: parse_desaturate_amount(input, &args)?,
            },
        }),
        "#darken" => {
            ensure_no_pattern_args(input, &args, "#darken")?;
            Ok(BlockPattern::Color {
                input: input.to_string(),
                kind: ColorPatternKind::Shade { darken: true },
            })
        }
        "#lighten" => {
            ensure_no_pattern_args(input, &args, "#lighten")?;
            Ok(BlockPattern::Color {
                input: input.to_string(),
                kind: ColorPatternKind::Shade { darken: false },
            })
        }
        "#anglecolor" | "#anglecolour" => Err(format!(
            "Pattern '{input}' still needs terrain-angle sampling from the world, which is not implemented in this engine yet."
        )),
        _ => Err(format!(
            "Pattern '{input}' needs FAWE's full pattern engine, which is not implemented yet."
        )),
    }
}

fn parse_clipboard_alias_pattern(input: &str) -> Result<Option<BlockPattern>, String> {
    let lowered = input.to_ascii_lowercase();
    let (kind, rest) = if let Some(rest) = lowered.strip_prefix("#clipboard") {
        (ClipboardPatternKind::Clipboard, rest)
    } else if let Some(rest) = lowered.strip_prefix("#copy") {
        (ClipboardPatternKind::Copy, rest)
    } else if let Some(rest) = lowered.strip_prefix("#fullcopy") {
        (ClipboardPatternKind::FullCopy, rest)
    } else {
        return Ok(None);
    };

    let raw_rest = &input[input.len() - rest.len()..];
    let offset = if raw_rest.is_empty() {
        (0, 0, 0)
    } else if let Some(offset_input) = raw_rest.strip_prefix("@[") {
        parse_clipboard_offset(input, offset_input)?
    } else {
        return Err(format!(
            "Unexpected clipboard pattern suffix '{raw_rest}' in '{input}'."
        ));
    };

    Ok(Some(BlockPattern::Clipboard {
        input: input.to_string(),
        kind,
        offset,
    }))
}

fn parse_clipboard_offset(input: &str, rest: &str) -> Result<(i32, i32, i32), String> {
    let Some(inner) = rest.strip_suffix(']') else {
        return Err(format!(
            "Unclosed clipboard offset in pattern '{input}'. Expected @[x,y,z]."
        ));
    };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(format!(
            "Invalid clipboard offset in pattern '{input}'. Expected @[x,y,z]."
        ));
    }
    Ok((
        parts[0]
            .parse::<i32>()
            .map_err(|_| format!("Invalid x offset '{}' in pattern '{input}'.", parts[0]))?,
        parts[1]
            .parse::<i32>()
            .map_err(|_| format!("Invalid y offset '{}' in pattern '{input}'.", parts[1]))?,
        parts[2]
            .parse::<i32>()
            .map_err(|_| format!("Invalid z offset '{}' in pattern '{input}'.", parts[2]))?,
    ))
}

fn parse_weighted(input: &str, entries: Vec<&str>) -> Result<BlockPattern, String> {
    let mut parsed = Vec::new();
    let mut total = 0u32;

    for raw in entries {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(format!("Invalid empty entry in pattern '{input}'."));
        }

        let (weight, pattern_input) = match find_top_level_percent(raw)? {
            Some(index) => {
                let weight = parse_weight(raw[..index].trim(), input)?;
                (weight, raw[index + 1..].trim())
            }
            None => (1000, raw),
        };

        if pattern_input.is_empty() {
            return Err(format!("Missing block after weight in pattern '{input}'."));
        }

        total = total
            .checked_add(weight)
            .ok_or_else(|| format!("Pattern '{input}' has too much total weight."))?;
        parsed.push(WeightedBlock {
            weight,
            pattern: Box::new(parse_pattern(pattern_input)?),
        });
    }

    if parsed.len() == 1 {
        return Ok(*parsed.remove(0).pattern);
    }

    Ok(BlockPattern::Weighted {
        input: input.to_string(),
        entries: parsed,
        total,
    })
}

fn parse_prefixed_call(input: &str) -> Result<(String, Vec<String>), String> {
    let mut end = input.len();
    for (index, ch) in input.char_indices() {
        if index > 0 && (ch.is_whitespace() || ch == '[') {
            end = index;
            break;
        }
    }

    let name = input[..end].to_ascii_lowercase();
    let rest = input[end..].trim();
    let args = if rest.starts_with('[') {
        parse_bracket_args(rest)?
    } else if rest.is_empty() {
        Vec::new()
    } else {
        split_whitespace_respecting_brackets(rest)?
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    Ok((name, args))
}

fn parse_offset_like_args(
    whole: &str,
    args: &[String],
    name: &str,
) -> Result<(i32, i32, i32, String), String> {
    if args.len() < 4 {
        return Err(format!("Usage: {name} <dx> <dy> <dz> <pattern>."));
    }
    Ok((
        parse_i32_arg(&args[0], "dx", whole)?,
        parse_i32_arg(&args[1], "dy", whole)?,
        parse_i32_arg(&args[2], "dz", whole)?,
        args[3..].join(" "),
    ))
}

fn parse_single_child_pattern(
    input: &str,
    args: &[String],
    name: &str,
) -> Result<BlockPattern, String> {
    if args.len() != 1 {
        return Err(format!("Usage: {name} <pattern>."));
    }
    parse_pattern(args[0].trim()).map_err(|_| format!("Invalid child pattern in '{input}'."))
}

fn parse_surface_spread_args(whole: &str, args: &[String]) -> Result<(i32, String), String> {
    match args {
        [pattern, distance] => Ok((
            parse_positive_i32_arg(distance, "distance", whole)?,
            pattern.clone(),
        )),
        [distance, rest @ ..] if !rest.is_empty() => Ok((
            parse_positive_i32_arg(distance, "distance", whole)?,
            rest.join(" "),
        )),
        _ => Err("Usage: #surfacespread <distance> <pattern>.".to_string()),
    }
}

fn parse_solid_spread_args(
    whole: &str,
    args: &[String],
) -> Result<(i32, i32, i32, String), String> {
    match args {
        [pattern, radius] => {
            let radius = parse_positive_i32_arg(radius, "distance", whole)?;
            Ok((radius, radius, radius, pattern.clone()))
        }
        [pattern, dx, dy, dz] => Ok((
            parse_positive_i32_arg(dx, "dx", whole)?,
            parse_positive_i32_arg(dy, "dy", whole)?,
            parse_positive_i32_arg(dz, "dz", whole)?,
            pattern.clone(),
        )),
        [radius, rest @ ..] if rest.len() == 1 => {
            let radius = parse_positive_i32_arg(radius, "distance", whole)?;
            Ok((radius, radius, radius, rest[0].clone()))
        }
        [dx, dy, dz, rest @ ..] if !rest.is_empty() => Ok((
            parse_positive_i32_arg(dx, "dx", whole)?,
            parse_positive_i32_arg(dy, "dy", whole)?,
            parse_positive_i32_arg(dz, "dz", whole)?,
            rest.join(" "),
        )),
        _ => Err("Usage: #solidspread <dx> <dy> <dz> <pattern>.".to_string()),
    }
}

fn parse_sequence_arg(
    input: &str,
    args: &[String],
    index: usize,
) -> Result<Vec<BlockPattern>, String> {
    let Some(raw) = args.get(index) else {
        return Err(format!("Missing pattern list for '{input}'."));
    };
    let mut patterns = Vec::new();
    for entry in split_top_level(raw, ',')? {
        let entry = entry.trim();
        let pattern = match find_top_level_percent(entry)? {
            Some(percent) => &entry[percent + 1..],
            None => entry,
        };
        patterns.push(parse_pattern(pattern.trim())?);
    }
    if patterns.is_empty() {
        return Err(format!("Missing pattern list for '{input}'."));
    }
    Ok(patterns)
}

fn parse_optional_scale(args: &[String], index: usize, name: &str) -> Result<i32, String> {
    match args.get(index) {
        Some(raw) => {
            let scale = raw
                .parse::<i32>()
                .map_err(|_| format!("Invalid {name} '{raw}'."))?;
            if scale <= 0 {
                return Err(format!("{name} must be positive."));
            }
            Ok(scale)
        }
        None => Ok(1),
    }
}

fn parse_i32_arg(raw: &str, name: &str, whole: &str) -> Result<i32, String> {
    raw.parse::<i32>()
        .map_err(|_| format!("Invalid {name} '{raw}' in pattern '{whole}'."))
}

fn parse_positive_i32_arg(raw: &str, name: &str, whole: &str) -> Result<i32, String> {
    let value = parse_i32_arg(raw, name, whole)?;
    if value <= 0 {
        return Err(format!("{name} must be positive in pattern '{whole}'."));
    }
    Ok(value)
}

fn parse_simplex_scale(raw: &str, whole: &str) -> Result<f64, String> {
    let scale = raw
        .parse::<f64>()
        .map_err(|_| format!("Invalid scale '{raw}' in pattern '{whole}'."))?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!("scale must be positive in pattern '{whole}'."));
    }
    Ok(1.0 / scale.max(1.0))
}

fn parse_weight(raw: &str, whole: &str) -> Result<u32, String> {
    let value = raw
        .parse::<f64>()
        .map_err(|_| format!("Invalid weight '{raw}' in pattern '{whole}'."))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("Invalid weight '{raw}' in pattern '{whole}'."));
    }
    Ok((value * 1000.0).round().max(1.0) as u32)
}

fn parse_bracket_args(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut rest = input.trim();
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return Err(format!("Unexpected pattern arguments '{rest}'."));
        }

        let mut depth = 0i32;
        let mut end = None;
        for (index, ch) in rest.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            return Err(format!("Unclosed '[' in pattern arguments '{input}'."));
        };

        args.push(rest[1..end].trim().to_string());
        rest = rest[end + 1..].trim();
    }
    Ok(args)
}

#[derive(Default)]
struct ScanState {
    bracket_depth: i32,
    brace_depth: i32,
    quote: Option<char>,
    escaped: bool,
}

impl ScanState {
    fn advance(&mut self, ch: char) -> Result<(), String> {
        if let Some(quote) = self.quote {
            if self.escaped {
                self.escaped = false;
            } else if ch == '\\' {
                self.escaped = true;
            } else if ch == quote {
                self.quote = None;
            }
            return Ok(());
        }

        match ch {
            '\'' | '"' => self.quote = Some(ch),
            '[' => self.bracket_depth += 1,
            ']' => {
                self.bracket_depth -= 1;
                if self.bracket_depth < 0 {
                    return Err("Unmatched ']' in pattern.".to_string());
                }
            }
            '{' => self.brace_depth += 1,
            '}' => {
                self.brace_depth -= 1;
                if self.brace_depth < 0 {
                    return Err("Unmatched '}' in pattern.".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn is_top_level(&self) -> bool {
        self.quote.is_none() && self.bracket_depth == 0 && self.brace_depth == 0
    }

    fn finish(self, input: &str) -> Result<(), String> {
        if self.quote.is_some() {
            return Err(format!("Unclosed quoted string in '{input}'."));
        }
        if self.bracket_depth != 0 {
            return Err(format!("Unclosed '[' in '{input}'."));
        }
        if self.brace_depth != 0 {
            return Err(format!("Unclosed '{{' in '{input}'."));
        }
        Ok(())
    }
}

fn split_top_level(input: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut scan = ScanState::default();
    let mut start = 0usize;
    for (index, ch) in input.char_indices() {
        scan.advance(ch)
            .map_err(|message| format!("{message} in '{input}'."))?;
        if ch == delimiter && scan.is_top_level() {
            parts.push(&input[start..index]);
            start = index + ch.len_utf8();
        }
    }
    scan.finish(input)?;
    parts.push(&input[start..]);
    Ok(parts)
}

fn split_whitespace_respecting_brackets(input: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut scan = ScanState::default();
    let mut start = None;

    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() && scan.is_top_level() {
            if let Some(s) = start.take() {
                parts.push(&input[s..index]);
            }
            continue;
        }
        start.get_or_insert(index);
        scan.advance(ch)
            .map_err(|message| format!("{message} in '{input}'."))?;
    }

    scan.finish(input)?;
    if let Some(s) = start {
        parts.push(&input[s..]);
    }
    Ok(parts)
}

fn find_top_level_percent(input: &str) -> Result<Option<usize>, String> {
    let mut scan = ScanState::default();
    for (index, ch) in input.char_indices() {
        scan.advance(ch)
            .map_err(|message| format!("{message} in '{input}'."))?;
        if ch == '%' && scan.is_top_level() {
            return Ok(Some(index));
        }
    }
    scan.finish(input)?;
    Ok(None)
}

fn single_wrapped_arg(input: &str) -> Option<&str> {
    if !input.starts_with('[') || !input.ends_with(']') {
        return None;
    }

    let mut depth = 0i32;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 && index != input.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0).then_some(&input[1..input.len() - 1])
}

fn spread_axis(hash: u32, shift: u32, radius: i32) -> i32 {
    if radius <= 0 {
        return 0;
    }
    let width = radius.saturating_mul(2).saturating_add(1) as u32;
    (((hash.rotate_left(shift) % width) as i32) - radius).clamp(-radius, radius)
}

fn position_hash(pos: BlockPos) -> u32 {
    let mut x = pos.x as u32;
    x ^= (pos.y as u32).wrapping_mul(0x9e37_79b9);
    x = x.rotate_left(13);
    x ^= (pos.z as u32).wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x
}

fn simplex_noise_unit(pos: BlockPos, inverse_scale: f64) -> f64 {
    cap_unit_interval(
        (simplex_noise::noise3(
            f64::from(pos.x) * inverse_scale,
            f64::from(pos.y) * inverse_scale,
            f64::from(pos.z) * inverse_scale,
        ) + 1.0)
            * 0.5,
    )
}

fn bounded_pick(unit: f64, total: u32) -> u32 {
    (unit * f64::from(total)) as u32
}

fn bounded_index(unit: f64, len: usize) -> usize {
    ((unit * len as f64) as usize).min(len.saturating_sub(1))
}

fn cap_unit_interval(value: f64) -> f64 {
    const MAX_EXCLUSIVE_ONE: f64 = f64::from_bits(0x3fefffffffffffff);
    value.clamp(0.0, MAX_EXCLUSIVE_ONE)
}

fn wrap_pattern_axis(value: i64, offset: i32, len: usize) -> usize {
    (value + i64::from(offset)).rem_euclid(len as i64) as usize
}

fn expression_result_to_state_id(value: f64) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    let truncated = value.trunc();
    if !(0.0..=u16::MAX as f64).contains(&truncated) {
        return 0;
    }
    truncated as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::ClipboardBuffer;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn parses_literal_pattern() {
        let pattern = BlockPattern::parse("stone").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 1);
        assert_eq!(pattern.literal_display(), Some(("stone", 1)));
    }

    #[test]
    fn parses_sign_pipe_syntax() {
        let pattern = BlockPattern::parse("oak_sign|Line1|Line 2").unwrap();
        let placement = pattern.placement_at_with(
            at(0, 0, 0),
            &BlockPlacement::new(0),
            &PatternEvalContext::default(),
        );
        let Some(BlockEntityData::Sign(sign)) = placement.block_entity else {
            panic!("expected sign payload");
        };
        assert_eq!(sign.front.messages[0], "Line1");
        assert_eq!(sign.front.messages[1], "Line 2");
    }

    #[test]
    fn parses_states_before_inline_sign_snbt() {
        let pattern = BlockPattern::parse("oak_sign[rotation=12]{'is_waxed':1}").unwrap();
        let placement = pattern.placement_at_with(
            at(0, 0, 0),
            &BlockPlacement::new(0),
            &PatternEvalContext::default(),
        );
        let Some(BlockEntityData::Sign(sign)) = placement.block_entity else {
            panic!("expected sign payload");
        };
        assert!(sign.waxed);
        assert_eq!(
            mapping::palette_key_for_state_id(placement.state_id),
            "minecraft:oak_sign[rotation=12,waterlogged=false]"
        );
    }

    #[test]
    fn weighted_parser_ignores_snbt_commas() {
        let pattern = BlockPattern::parse("oak_sign{'is_waxed':1},stone");
        assert!(pattern.is_ok());
    }

    #[test]
    fn existing_pattern_keeps_before_state() {
        let pattern = BlockPattern::parse("#existing").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 10);
    }

    #[test]
    fn parses_expression_pattern() {
        let pattern = BlockPattern::parse("= x > 0 ? 1 : 10").unwrap();
        assert_eq!(pattern.description(), "= x > 0 ? 1 : 10");
    }

    #[test]
    fn expression_pattern_uses_coordinates() {
        let pattern = BlockPattern::parse("= x + y + z").unwrap();
        assert_eq!(pattern.state_at(at(1, 2, 3), 0), 6);
    }

    #[test]
    fn expression_pattern_supports_world_queries() {
        let pattern = BlockPattern::parse("= queryRel(1,0,0,10,-1) ? 1 : 0").unwrap();
        let ctx = PatternEvalContext::with_world_states(at(0, 0, 0), &[((6, 0, 0), 10)]);
        assert_eq!(pattern.state_at_with(at(5, 0, 0), 0, &ctx), 1);
    }

    #[test]
    fn parses_clipboard_pattern() {
        let pattern = BlockPattern::parse("#clipboard").unwrap();
        assert_eq!(pattern.description(), "#clipboard");
        assert!(matches!(
            pattern,
            BlockPattern::Clipboard {
                kind: ClipboardPatternKind::Clipboard,
                offset: (0, 0, 0),
                ..
            }
        ));
    }

    #[test]
    fn parses_clipboard_aliases_and_offsets() {
        let copy = BlockPattern::parse("#copy").unwrap();
        assert!(matches!(
            copy,
            BlockPattern::Clipboard {
                kind: ClipboardPatternKind::Copy,
                offset: (0, 0, 0),
                ..
            }
        ));

        let full = BlockPattern::parse("#fullcopy@[2,0,1]").unwrap();
        assert!(matches!(
            full,
            BlockPattern::Clipboard {
                kind: ClipboardPatternKind::FullCopy,
                offset: (2, 0, 1),
                ..
            }
        ));
    }

    #[test]
    fn parses_weighted_pattern() {
        let pattern = BlockPattern::parse("50%stone,50%dirt").unwrap();
        match pattern {
            BlockPattern::Weighted { total, entries, .. } => {
                assert_eq!(total, 100_000);
                assert_eq!(entries.len(), 2);
            }
            _ => panic!("expected weighted pattern"),
        }
    }

    #[test]
    fn single_weighted_entry_collapses_to_child() {
        let pattern = BlockPattern::parse("100%stone").unwrap();
        assert_eq!(pattern.literal_display(), Some(("stone", 1)));
    }

    #[test]
    fn weighted_parser_ignores_property_commas() {
        let pattern = BlockPattern::parse("water[level=0,falling=false],stone");
        assert!(pattern.is_ok());
    }

    #[test]
    fn parses_random_state_pattern() {
        let pattern = BlockPattern::parse("*stone").unwrap();
        assert!(matches!(pattern, BlockPattern::RandomStates { .. }));
    }

    #[test]
    fn parses_namespaced_block_tag_pattern() {
        let pattern = BlockPattern::parse("##minecraft:slabs").unwrap();
        assert!(matches!(pattern, BlockPattern::RandomStates { .. }));
    }

    #[test]
    fn block_tag_pattern_rejects_empty_tags() {
        let err = BlockPattern::parse("##c:ropes").unwrap_err();
        assert!(err.contains("empty block category"));
    }

    #[test]
    fn parses_fawe_offset_bracket_syntax() {
        let pattern = BlockPattern::parse("#offset[1][0][0][stone,dirt]").unwrap();
        assert_eq!(pattern.description(), "#offset[1][0][0][stone,dirt]");
    }

    #[test]
    fn parses_linear2d_pattern() {
        let pattern = BlockPattern::parse("#linear2d[stone,dirt][2][3]").unwrap();
        assert!(matches!(pattern, BlockPattern::Linear2d { .. }));
    }

    #[test]
    fn parses_stateful_world_context_patterns() {
        assert!(matches!(
            BlockPattern::parse("#buffer[stone]").unwrap(),
            BlockPattern::Buffer { .. }
        ));
        assert!(matches!(
            BlockPattern::parse("#buffer2d dirt").unwrap(),
            BlockPattern::Buffer2d { .. }
        ));
        assert!(matches!(
            BlockPattern::parse("#relative[#existing]").unwrap(),
            BlockPattern::Relative { .. }
        ));
        assert!(matches!(
            BlockPattern::parse("#surfacespread[#existing][2]").unwrap(),
            BlockPattern::SurfaceSpread { .. }
        ));
        assert!(matches!(
            BlockPattern::parse("#solidspread 1 2 3 #existing").unwrap(),
            BlockPattern::SolidSpread { .. }
        ));
    }

    #[test]
    fn parses_mask_pattern() {
        let pattern = BlockPattern::parse("#mask[#existing][stone][dirt]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 1);
        assert_eq!(pattern.state_at(at(0, 0, 0), 0), 10);
    }

    #[test]
    fn parses_simplex_pattern() {
        let pattern = BlockPattern::parse("#simplex[2.5][stone,dirt]").unwrap();
        assert!(matches!(pattern, BlockPattern::Simplex { .. }));
    }

    #[test]
    fn simplex_pattern_matches_fawe_weighted_bucket_choices() {
        let pattern = BlockPattern::parse("#simplex[10][stone,dirt]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 0), 10);
        assert_eq!(pattern.state_at(at(10, 0, 0), 0), 1);
        assert_eq!(pattern.state_at(at(15, 0, 0), 0), 1);
        assert_eq!(pattern.state_at(at(20, 0, 0), 0), 10);
        assert_eq!(pattern.state_at(at(3, 7, 11), 0), 1);
        assert_eq!(pattern.state_at(at(-9, 2, 14), 0), 10);
    }

    #[test]
    fn simplex_pattern_matches_fawe_list_bucket_choices() {
        let pattern = BlockPattern::parse("#simplex[8][stone,dirt,grass_block]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 0), 10);
        assert_eq!(pattern.state_at(at(4, 0, 0), 0), 9);
        assert_eq!(pattern.state_at(at(8, 0, 0), 0), 1);
        assert_eq!(pattern.state_at(at(12, 0, 0), 0), 10);
        assert_eq!(pattern.state_at(at(-9, 2, 14), 0), 1);
    }

    #[test]
    fn biome_pattern_reports_missing_biome_write_support() {
        let err = BlockPattern::parse("#biome[plains]").unwrap_err();
        assert!(err.contains("world.get-biome"));
        assert!(err.contains("world.set-biome"));
    }

    #[test]
    fn state_apply_keeps_existing_type() {
        let pattern = BlockPattern::parse("^[waterlogged=false]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 1), 1);
    }

    #[test]
    fn buffer_pattern_skips_repeated_positions() {
        let pattern = BlockPattern::parse("#buffer[stone]").unwrap();
        let ctx = PatternEvalContext::new(at(0, 0, 0));
        assert_eq!(pattern.state_at_with(at(1, 2, 3), 0, &ctx), 1);
        assert_eq!(pattern.state_at_with(at(1, 2, 3), 10, &ctx), 10);
    }

    #[test]
    fn buffer2d_pattern_skips_repeated_columns() {
        let pattern = BlockPattern::parse("#buffer2d[stone]").unwrap();
        let ctx = PatternEvalContext::new(at(0, 0, 0));
        assert_eq!(pattern.state_at_with(at(1, 2, 3), 0, &ctx), 1);
        assert_eq!(pattern.state_at_with(at(1, 20, 3), 10, &ctx), 10);
    }

    #[test]
    fn offset_pattern_samples_shifted_world_state() {
        let pattern = BlockPattern::parse("#offset[1][0][0][#existing]").unwrap();
        let ctx = PatternEvalContext::with_world_states(at(0, 0, 0), &[((1, 0, 0), 10)]);
        assert_eq!(pattern.state_at_with(at(0, 0, 0), 0, &ctx), 10);
    }

    #[test]
    fn relative_pattern_uses_operation_origin() {
        let pattern = BlockPattern::parse("#relative[#offset[1][0][0][#existing]]").unwrap();
        let ctx = PatternEvalContext::with_world_states(at(10, 0, 10), &[((1, 0, 0), 10)]);
        assert_eq!(pattern.state_at_with(at(10, 0, 10), 0, &ctx), 10);
    }

    #[test]
    fn surface_spread_samples_surface_neighbor() {
        let pattern = BlockPattern::parse("#surfacespread[#existing][1]").unwrap();
        let ctx = PatternEvalContext::with_world_states(at(0, 0, 0), &[((1, 0, 0), 10)]);
        assert_eq!(pattern.state_at_with(at(0, 0, 0), 0, &ctx), 10);
    }

    #[test]
    fn solid_spread_falls_back_when_sampled_block_is_not_solid() {
        let pattern = BlockPattern::parse("#solidspread[#existing][1]").unwrap();
        let ctx = PatternEvalContext::with_world_states(
            at(0, 0, 0),
            &[((0, 0, 0), 1), ((-1, -1, -1), 0)],
        );
        assert_eq!(pattern.state_at_with(at(0, 0, 0), 1, &ctx), 1);
    }

    #[test]
    fn clipboard_pattern_requires_clipboard_context() {
        let pattern = BlockPattern::parse("#clipboard").unwrap();
        let err = pattern
            .validate(&PatternEvalContext::new(at(0, 0, 0)))
            .unwrap_err();
        assert!(err.contains("clipboard"));
    }

    #[test]
    fn clipboard_pattern_repeats_across_target_positions() {
        let pattern = BlockPattern::parse("#clipboard").unwrap();
        let ctx = PatternEvalContext::with_clipboard(
            at(20, 5, -3),
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 1), ((1, 0, 0), 10)],
                block_entities: Vec::new(),
            },
        );
        pattern.validate(&ctx).unwrap();
        assert_eq!(pattern.state_at_with(at(20, 5, -3), 0, &ctx), 1);
        assert_eq!(pattern.state_at_with(at(21, 5, -3), 0, &ctx), 10);
        assert_eq!(pattern.state_at_with(at(22, 5, -3), 0, &ctx), 1);
    }

    #[test]
    fn clipboard_pattern_aligns_first_tile_to_clipboard_min_corner() {
        let pattern = BlockPattern::parse("#clipboard").unwrap();
        let ctx = PatternEvalContext::with_clipboard(
            at(100, 64, 100),
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((-1, 0, 0), 1), ((0, 0, 0), 10)],
                block_entities: Vec::new(),
            },
        );
        assert_eq!(pattern.state_at_with(at(100, 64, 100), 0, &ctx), 1);
        assert_eq!(pattern.state_at_with(at(101, 64, 100), 0, &ctx), 10);
    }

    #[test]
    fn clipboard_pattern_offset_shifts_the_sampled_cell() {
        let pattern = BlockPattern::parse("#clipboard@[1,0,0]").unwrap();
        let ctx = PatternEvalContext::with_clipboard(
            at(7, 8, 9),
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 1), ((1, 0, 0), 10)],
                block_entities: Vec::new(),
            },
        );
        assert_eq!(pattern.state_at_with(at(7, 8, 9), 0, &ctx), 10);
        assert_eq!(pattern.state_at_with(at(8, 8, 9), 0, &ctx), 1);
    }

    #[test]
    fn clipboard_pattern_preserves_air_cells() {
        let pattern = BlockPattern::parse("#fullcopy").unwrap();
        let ctx = PatternEvalContext::with_clipboard(
            at(0, 0, 0),
            ClipboardBuffer {
                origin: at(0, 0, 0),
                blocks: vec![((0, 0, 0), 0), ((1, 0, 0), 10)],
                block_entities: Vec::new(),
            },
        );
        assert_eq!(pattern.state_at_with(at(0, 0, 0), 99, &ctx), 0);
        assert_eq!(pattern.state_at_with(at(1, 0, 0), 99, &ctx), 10);
    }

    #[test]
    fn clipboard_pattern_rejects_bad_offsets() {
        assert!(BlockPattern::parse("#clipboard@[1,2]").is_err());
        assert!(BlockPattern::parse("#clipboard@[1,two,3]").is_err());
        assert!(BlockPattern::parse("#clipboard@[").is_err());
    }

    #[test]
    fn color_pattern_matches_exact_palette_color() {
        let pattern = BlockPattern::parse("#color[255][255][255]").unwrap();
        let expected = crate::mapping::resolve_block("white_wool").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 1), expected);
    }

    #[test]
    fn color_pattern_accepts_rgb_without_alpha() {
        let pattern = BlockPattern::parse("#averagecolor[255][255][255]").unwrap();
        let before = crate::mapping::resolve_block("stone").unwrap();
        assert_eq!(pattern.validate(&PatternEvalContext::default()), Ok(()));
        assert_ne!(pattern.state_at(at(0, 0, 0), before), 0);
    }

    #[test]
    fn saturate_pattern_changes_supported_blocks() {
        let pattern = BlockPattern::parse("#saturate[255][64][64]").unwrap();
        let before = crate::mapping::resolve_block("stone").unwrap();
        assert_ne!(pattern.state_at(at(0, 0, 0), before), before);
    }

    #[test]
    fn color_patterns_leave_unsupported_transparent_blocks_unchanged() {
        let pattern = BlockPattern::parse("#darken").unwrap();
        let glass = crate::mapping::resolve_block("glass").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), glass), glass);
    }

    #[test]
    fn parses_literal_mask_list() {
        let mask = BlockMask::parse("stone,dirt").unwrap();
        assert!(mask.matches(1));
        assert!(mask.matches(10));
        assert!(!mask.matches(0));
    }

    #[test]
    fn parses_existing_mask() {
        let mask = BlockMask::parse("#existing").unwrap();
        assert!(mask.matches(1));
        assert!(!mask.matches(0));
    }
}
