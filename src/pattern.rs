//! FAWE-style block patterns and the small mask subset used by commands.
//!
//! Pumpkin currently exposes block-state ids to this plugin, not full
//! WorldEdit extents, biome setters, or block entities inside the pattern
//! engine. This parser therefore supports the FAWE/WorldEdit patterns that can
//! be evaluated from `(position, existing_state)` plus a small evaluation
//! context for clipboard-backed patterns, and returns precise errors for
//! patterns that still need richer world context.

use std::cell::Cell;

use pumpkin_plugin_api::common::BlockPos;

use crate::{
    clipboard::{self, ClipboardBuffer},
    mapping,
};

#[derive(Clone, Debug)]
pub enum BlockPattern {
    Literal {
        input: String,
        state_id: u16,
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
    Mask {
        input: String,
        mask: BlockMask,
        true_pattern: Box<BlockPattern>,
        false_pattern: Box<BlockPattern>,
    },
    Simplex {
        input: String,
        scale: i32,
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

#[derive(Clone)]
pub struct PatternEvalContext {
    origin: BlockPos,
    clipboard: Option<PreparedClipboardPattern>,
}

impl PatternEvalContext {
    pub fn new(origin: BlockPos) -> Self {
        Self {
            origin,
            clipboard: None,
        }
    }

    pub fn for_player(origin: BlockPos, key: &str) -> Self {
        Self {
            origin,
            clipboard: clipboard::get(key).and_then(PreparedClipboardPattern::from_buffer),
        }
    }

    #[cfg(test)]
    pub fn with_clipboard(origin: BlockPos, buffer: ClipboardBuffer) -> Self {
        Self {
            origin,
            clipboard: PreparedClipboardPattern::from_buffer(buffer),
        }
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
            Self::Literal { state_id, .. } => *state_id,
            Self::Existing => before,
            Self::Clipboard { offset, .. } => ctx.clipboard.as_ref().map_or(before, |clipboard| {
                clipboard.state_at(pos, ctx.origin, *offset)
            }),
            Self::Weighted { entries, total, .. } => {
                let mut pick = position_hash(pos) % *total;
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
                let index = (position_hash(pos) as usize) % states.len();
                states[index]
            }
            Self::TypeApply { pattern, .. } => {
                let target = pattern.state_at_with(pos, before, ctx);
                mapping::apply_existing_states(target, before).unwrap_or(target)
            }
            Self::StateApply { properties, .. } => {
                mapping::apply_state_properties(before, properties).unwrap_or(before)
            }
            Self::Offset {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => pattern.state_at_with(
                BlockPos {
                    x: pos.x + dx,
                    y: pos.y + dy,
                    z: pos.z + dz,
                },
                before,
                ctx,
            ),
            Self::Spread {
                dx,
                dy,
                dz,
                pattern,
                ..
            } => {
                let hash = position_hash(pos);
                pattern.state_at_with(
                    BlockPos {
                        x: pos.x + spread_axis(hash, 0, *dx),
                        y: pos.y + spread_axis(hash, 10, *dy),
                        z: pos.z + spread_axis(hash, 20, *dz),
                    },
                    before,
                    ctx,
                )
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
            Self::Simplex { scale, pattern, .. } => pattern.state_at_with(
                BlockPos {
                    x: pos.x.div_euclid(*scale),
                    y: pos.y.div_euclid(*scale),
                    z: pos.z.div_euclid(*scale),
                },
                before,
                ctx,
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
            } => pattern.state_at_with(
                BlockPos {
                    x: if *x { pos.x } else { 0 },
                    y: if *y { pos.y } else { 0 },
                    z: if *z { pos.z } else { 0 },
                },
                before,
                ctx,
            ),
        }
    }

    pub fn validate(&self, ctx: &PatternEvalContext) -> Result<(), String> {
        match self {
            Self::Literal { .. }
            | Self::Existing
            | Self::RandomStates { .. }
            | Self::StateApply { .. } => Ok(()),
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
            | Self::Offset { pattern, .. }
            | Self::Spread { pattern, .. }
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
            Self::Literal { input, state_id } => Some((input, *state_id)),
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
            | Self::Offset { input, .. }
            | Self::Spread { input, .. }
            | Self::Mask { input, .. }
            | Self::Simplex { input, .. }
            | Self::Linear { input, .. }
            | Self::Linear2d { input, .. }
            | Self::Linear3d { input, .. }
            | Self::AxisMask { input, .. } => input,
            Self::Existing => "#existing",
        }
    }
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

    let Some(state_id) = mapping::resolve_block(input) else {
        return Err(format!("Unknown block '{input}'."));
    };
    Ok(BlockPattern::Literal {
        input: input.to_string(),
        state_id,
    })
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
            let (scale, pattern_index) = if args.len() >= 2 {
                (parse_positive_i32_arg(&args[0], "scale", input)?, 1)
            } else {
                (10, 0)
            };
            Ok(BlockPattern::Simplex {
                input: input.to_string(),
                scale,
                pattern: Box::new(parse_pattern(&args[pattern_index])?),
            })
        }
        "#biome" => Err(format!(
            "Pattern '{input}' needs biome editing support, which is not implemented yet."
        )),
        "#color" | "#saturate" | "#darken" | "#anglecolor" | "#desaturate" | "#averagecolor"
        | "#lighten" => Err(format!(
            "Pattern '{input}' needs FAWE's block color matcher, which is not implemented yet."
        )),
        "#buffer" | "#buffer2d" | "#relative" | "#surfacespread" | "#solidspread" => Err(format!(
            "Pattern '{input}' needs operation-wide FAWE pattern state or world surface checks, which is not implemented yet."
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

fn split_top_level(input: &str, delimiter: char) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("Unmatched ']' in '{input}'."));
                }
            }
            _ if ch == delimiter && depth == 0 => {
                parts.push(&input[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("Unclosed '[' in '{input}'."));
    }
    parts.push(&input[start..]);
    Ok(parts)
}

fn split_whitespace_respecting_brackets(input: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = None;

    for (index, ch) in input.char_indices() {
        match ch {
            '[' => {
                depth += 1;
                start.get_or_insert(index);
            }
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("Unmatched ']' in '{input}'."));
                }
            }
            _ if ch.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() {
                    parts.push(&input[s..index]);
                }
            }
            _ => {
                start.get_or_insert(index);
            }
        }
    }

    if depth != 0 {
        return Err(format!("Unclosed '[' in '{input}'."));
    }
    if let Some(s) = start {
        parts.push(&input[s..]);
    }
    Ok(parts)
}

fn find_top_level_percent(input: &str) -> Result<Option<usize>, String> {
    let mut depth = 0i32;
    for (index, ch) in input.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth < 0 {
                    return Err(format!("Unmatched ']' in '{input}'."));
                }
            }
            '%' if depth == 0 => return Ok(Some(index)),
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("Unclosed '[' in '{input}'."));
    }
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

fn wrap_pattern_axis(value: i64, offset: i32, len: usize) -> usize {
    (value + i64::from(offset)).rem_euclid(len as i64) as usize
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
    fn existing_pattern_keeps_before_state() {
        let pattern = BlockPattern::parse("#existing").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 10);
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
    fn parses_mask_pattern() {
        let pattern = BlockPattern::parse("#mask[#existing][stone][dirt]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 1);
        assert_eq!(pattern.state_at(at(0, 0, 0), 0), 10);
    }

    #[test]
    fn parses_simplex_pattern() {
        let pattern = BlockPattern::parse("#simplex[8][stone,dirt]").unwrap();
        assert!(matches!(pattern, BlockPattern::Simplex { .. }));
    }

    #[test]
    fn state_apply_keeps_existing_type() {
        let pattern = BlockPattern::parse("^[waterlogged=false]").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 1), 1);
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
