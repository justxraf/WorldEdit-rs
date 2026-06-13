//! Shape generation commands copied from FAWE's generation/region behavior
//! where Pumpkin's current APIs allow it.
//!
//! Implemented:
//! - `//sphere`, `//hsphere`
//! - `//cyl`, `//hcyl`
//! - `//pyramid`, `//hpyramid`
//! - `//cone`
//! - `//line`
//!
//! `//curve` is registered, but still reports the missing convex polyhedral
//! selection support until `todo/commands/selection-shapes.md` lands.

use std::collections::HashSet;

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    common::BlockPos,
    logging::{self, LogLevel},
    text::TextComponent,
    world::World,
};

use crate::{
    block_data,
    history::{self, EditEntry},
    mapping,
    pattern::{BlockPattern, PatternEvalContext},
    selection::{self, Region},
};

use super::{
    MAX_BUILD_Y, MIN_BUILD_Y, batch_size, block_flags, command_names, passes_gmask, player_key,
    sender_block_pos,
};

pub fn register(context: &Context) {
    register_shape(
        context,
        "sphere",
        "Generate a filled sphere",
        GenerationKind::Sphere {
            hollow_alias: false,
        },
        "worldedit.generation.sphere",
    );
    register_shape(
        context,
        "hsphere",
        "Generate a hollow sphere",
        GenerationKind::Sphere { hollow_alias: true },
        "worldedit.generation.sphere",
    );
    register_shape(
        context,
        "cyl",
        "Generate a cylinder",
        GenerationKind::Cylinder {
            hollow_alias: false,
        },
        "worldedit.generation.cylinder",
    );
    register_shape(
        context,
        "hcyl",
        "Generate a hollow cylinder",
        GenerationKind::Cylinder { hollow_alias: true },
        "worldedit.generation.cylinder",
    );
    register_shape(
        context,
        "pyramid",
        "Generate a filled pyramid",
        GenerationKind::Pyramid {
            hollow_alias: false,
        },
        "worldedit.generation.pyramid",
    );
    register_shape(
        context,
        "hpyramid",
        "Generate a hollow pyramid",
        GenerationKind::Pyramid { hollow_alias: true },
        "worldedit.generation.pyramid",
    );
    register_shape(
        context,
        "cone",
        "Generate a cone",
        GenerationKind::Cone,
        "worldedit.generation.cone",
    );
    register_shape(
        context,
        "line",
        "Draw a line between selection points",
        GenerationKind::Line,
        "worldedit.region.line",
    );
    register_shape(
        context,
        "curve",
        "Draw a curve through selected points",
        GenerationKind::Curve,
        "worldedit.region.curve",
    );
}

fn register_shape(
    context: &Context,
    name: &str,
    description: &str,
    kind: GenerationKind,
    permission: &str,
) {
    let args = CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
        .execute(GenerationCommand { kind });
    let command =
        Command::new(&command_names(name), description).execute(GenerationCommand { kind });
    command.then(args);
    context.register_command(command, permission);
}

#[derive(Clone, Copy)]
enum GenerationKind {
    Sphere { hollow_alias: bool },
    Cylinder { hollow_alias: bool },
    Pyramid { hollow_alias: bool },
    Cone,
    Line,
    Curve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct BlockPosKey(i32, i32, i32);

impl From<BlockPos> for BlockPosKey {
    fn from(pos: BlockPos) -> Self {
        Self(pos.x, pos.y, pos.z)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SphereRequest {
    pattern: String,
    radius_x: f64,
    radius_y: f64,
    radius_z: f64,
    raised: bool,
    hollow: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct CylinderRequest {
    pattern: String,
    radius_x: f64,
    radius_z: f64,
    height: i32,
    hollow: bool,
    thickness: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct PyramidRequest {
    pattern: String,
    size: i32,
    hollow: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ConeRequest {
    pattern: String,
    radius_x: f64,
    radius_z: f64,
    height: i32,
    hollow: bool,
    thickness: f64,
}

#[derive(Clone, Debug, PartialEq)]
struct LineRequest {
    pattern: String,
    thickness: f64,
    shell: bool,
}

#[derive(Clone, Copy, Default)]
struct ParsedSwitches {
    h: bool,
    r: bool,
}

#[derive(Clone, Copy)]
struct AllowedSwitches {
    h: bool,
    r: bool,
}

#[derive(Clone, Copy)]
struct GenerationCommand {
    kind: GenerationKind,
}

impl pumpkin_plugin_api::commands::CommandHandler for GenerationCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        match self.kind {
            GenerationKind::Sphere { hollow_alias } => {
                let Ok((key, world, target)) = require_generation_target(&sender) else {
                    return Ok(0);
                };
                let raw = raw_args(&args);
                let request = match parse_sphere_request(&raw, hollow_alias) {
                    Ok(request) => request,
                    Err(message) => {
                        sender.send_error(TextComponent::text(&message));
                        return Ok(0);
                    }
                };
                let pattern = match parse_pattern(&sender, &request.pattern) {
                    Some(pattern) => pattern,
                    None => return Ok(0),
                };
                let mut center = target;
                if request.raised {
                    center.y = center.y.saturating_add(request.radius_y as i32);
                }
                let positions = sphere_positions(
                    center,
                    request.radius_x,
                    request.radius_y,
                    request.radius_z,
                    !request.hollow,
                );
                apply_generated_pattern(
                    &sender, &key, &world, center, positions, &pattern, "sphere",
                )
            }
            GenerationKind::Cylinder { hollow_alias } => {
                let Ok((key, world, target)) = require_generation_target(&sender) else {
                    return Ok(0);
                };
                let raw = raw_args(&args);
                let request = match parse_cylinder_request(&raw, hollow_alias) {
                    Ok(request) => request,
                    Err(message) => {
                        sender.send_error(TextComponent::text(&message));
                        return Ok(0);
                    }
                };
                let pattern = match parse_pattern(&sender, &request.pattern) {
                    Some(pattern) => pattern,
                    None => return Ok(0),
                };
                let positions = cylinder_positions(
                    target,
                    request.radius_x,
                    request.radius_z,
                    request.height,
                    request.thickness,
                    !request.hollow,
                );
                apply_generated_pattern(
                    &sender, &key, &world, target, positions, &pattern, "cylinder",
                )
            }
            GenerationKind::Pyramid { hollow_alias } => {
                let Ok((key, world, target)) = require_generation_target(&sender) else {
                    return Ok(0);
                };
                let raw = raw_args(&args);
                let request = match parse_pyramid_request(&raw, hollow_alias) {
                    Ok(request) => request,
                    Err(message) => {
                        sender.send_error(TextComponent::text(&message));
                        return Ok(0);
                    }
                };
                let pattern = match parse_pattern(&sender, &request.pattern) {
                    Some(pattern) => pattern,
                    None => return Ok(0),
                };
                let positions = pyramid_positions(target, request.size, !request.hollow);
                apply_generated_pattern(
                    &sender, &key, &world, target, positions, &pattern, "pyramid",
                )
            }
            GenerationKind::Cone => {
                let Ok((key, world, target)) = require_generation_target(&sender) else {
                    return Ok(0);
                };
                let raw = raw_args(&args);
                let request = match parse_cone_request(&raw) {
                    Ok(request) => request,
                    Err(message) => {
                        sender.send_error(TextComponent::text(&message));
                        return Ok(0);
                    }
                };
                let pattern = match parse_pattern(&sender, &request.pattern) {
                    Some(pattern) => pattern,
                    None => return Ok(0),
                };
                let positions = cone_positions(
                    target,
                    request.radius_x,
                    request.radius_z,
                    request.height,
                    !request.hollow,
                    request.thickness,
                );
                apply_generated_pattern(&sender, &key, &world, target, positions, &pattern, "cone")
            }
            GenerationKind::Line => {
                let Ok((key, world, pos1, pos2)) = require_line_selection(&sender) else {
                    return Ok(0);
                };
                let raw = raw_args(&args);
                let request = match parse_line_request(&raw, "//line <pattern> [thickness] [-h]") {
                    Ok(request) => request,
                    Err(message) => {
                        sender.send_error(TextComponent::text(&message));
                        return Ok(0);
                    }
                };
                let pattern = match parse_pattern(&sender, &request.pattern) {
                    Some(pattern) => pattern,
                    None => return Ok(0),
                };
                let positions = line_positions(pos1, pos2, request.thickness, !request.shell);
                apply_generated_pattern(&sender, &key, &world, pos1, positions, &pattern, "line")
            }
            GenerationKind::Curve => {
                if sender.as_player().is_none() {
                    sender.send_error(TextComponent::text("Only players can use this command."));
                    return Ok(0);
                }
                sender.send_error(TextComponent::text(
                    "Convex polyhedral selections are not supported yet, so //curve is unavailable for now.",
                ));
                Ok(0)
            }
        }
    }
}

fn raw_args(args: &ConsumedArgs) -> String {
    match args.get_value("args") {
        Arg::Simple(s) | Arg::Msg(s) => s,
        _ => String::new(),
    }
}

fn require_generation_target(
    sender: &CommandSender,
) -> std::result::Result<(String, World, BlockPos), ()> {
    if sender.as_player().is_none() {
        sender.send_error(TextComponent::text("Only players can use this command."));
        return Err(());
    }
    let Some(key) = player_key(sender) else {
        sender.send_error(TextComponent::text("Could not determine your identity."));
        return Err(());
    };
    let Some(world) = sender.world() else {
        sender.send_error(TextComponent::text("Could not determine your world."));
        return Err(());
    };
    let Ok(target) = sender_block_pos(sender) else {
        return Err(());
    };
    Ok((key, world, target))
}

fn require_line_selection(
    sender: &CommandSender,
) -> std::result::Result<(String, World, BlockPos, BlockPos), ()> {
    if sender.as_player().is_none() {
        sender.send_error(TextComponent::text("Only players can use this command."));
        return Err(());
    }
    let Some(key) = player_key(sender) else {
        sender.send_error(TextComponent::text("Could not determine your identity."));
        return Err(());
    };
    let Some(world) = sender.world() else {
        sender.send_error(TextComponent::text("Could not determine your world."));
        return Err(());
    };
    let (pos1, pos2) = selection::with_selection(&key, |sel| (sel.pos1, sel.pos2));
    match (pos1, pos2) {
        (Some(pos1), Some(pos2)) => Ok((key, world, pos1, pos2)),
        _ => {
            sender.send_error(TextComponent::text("Set both //pos1 and //pos2 first."));
            Err(())
        }
    }
}

fn parse_pattern(sender: &CommandSender, raw_pattern: &str) -> Option<BlockPattern> {
    match BlockPattern::parse(raw_pattern) {
        Ok(pattern) => Some(pattern),
        Err(message) => {
            sender.send_error(TextComponent::text(&message));
            None
        }
    }
}

fn apply_generated_pattern(
    sender: &CommandSender,
    key: &str,
    world: &World,
    origin: BlockPos,
    mut positions: Vec<BlockPos>,
    pattern: &BlockPattern,
    label: &str,
) -> std::result::Result<i32, CommandError> {
    positions.sort_by_key(|pos| (pos.y, pos.z, pos.x));
    let pattern_ctx = PatternEvalContext::for_operation(origin, key, world);
    if let Err(message) = pattern.validate(&pattern_ctx) {
        sender.send_error(TextComponent::text(&message));
        return Ok(0);
    }

    if let Some(region) = region_for_positions(&positions) {
        selection::set_region(key, region);
    }

    let started = std::time::Instant::now();
    let mut placed = 0usize;
    let mut entry = EditEntry::default();
    for batch in positions.chunks(batch_size()) {
        let mut changes = Vec::with_capacity(batch.len());
        for &pos in batch {
            let before_state = world.get_block_state_id(pos);
            if !passes_gmask(key, before_state) {
                continue;
            }
            let before = block_data::capture_block_with_state(world, pos, before_state);
            let after = pattern.placement_at_with(pos, &before, &pattern_ctx);
            if before == after {
                continue;
            }
            entry.push_change(pos, before, after.clone());
            changes.push((pos, after));
        }
        placed += changes.len();
        if !changes.is_empty() {
            block_data::apply_blocks(world, &changes, block_flags());
        }
    }
    history::push(key, entry);

    logging::log(
        LogLevel::Info,
        &format!(
            "WorldEdit-rs: //{label} changed {placed} blocks in {:?}.",
            started.elapsed()
        ),
    );
    let message = TextComponent::text(&format!("{label} changed {placed} block(s) to "));
    if let Some((input, state_id)) = pattern.literal_display() {
        message.add_child(mapping::display_component(input, state_id));
    } else {
        message.add_text(pattern.description());
    }
    message.add_text(".");
    sender.send_message(message);
    Ok(1)
}

fn region_for_positions(positions: &[BlockPos]) -> Option<Region> {
    let mut iter = positions.iter().copied();
    let first = iter.next()?;
    let mut min = first;
    let mut max = first;
    for pos in iter {
        min.x = min.x.min(pos.x);
        min.y = min.y.min(pos.y);
        min.z = min.z.min(pos.z);
        max.x = max.x.max(pos.x);
        max.y = max.y.max(pos.y);
        max.z = max.z.max(pos.z);
    }
    Some(Region::new(min, max))
}

fn split_tokens(raw: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut quote = None::<char>;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut start = None::<usize>;
    let mut escaped = false;

    for (index, ch) in raw.char_indices() {
        if let Some(current_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == current_quote {
                quote = None;
            }
            start.get_or_insert(index);
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '[' => bracket_depth += 1,
            ']' => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    return Err("Unmatched ']' in command arguments.".to_string());
                }
            }
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    return Err("Unmatched '}' in command arguments.".to_string());
                }
            }
            _ => {}
        }

        if ch.is_whitespace() && bracket_depth == 0 && brace_depth == 0 {
            if let Some(token_start) = start.take() {
                parts.push(&raw[token_start..index]);
            }
            continue;
        }

        start.get_or_insert(index);
    }

    if quote.is_some() {
        return Err("Unclosed quoted string in command arguments.".to_string());
    }
    if bracket_depth != 0 {
        return Err("Unclosed '[' in command arguments.".to_string());
    }
    if brace_depth != 0 {
        return Err("Unclosed '{' in command arguments.".to_string());
    }

    if let Some(token_start) = start {
        parts.push(&raw[token_start..]);
    }
    Ok(parts)
}

fn parse_pattern_and_tail<'a>(
    raw: &'a str,
    usage: &str,
) -> Result<(&'a str, Vec<&'a str>), String> {
    let tokens = split_tokens(raw.trim())?;
    if tokens.len() < 2 {
        return Err(format!("Usage: {usage}"));
    }
    Ok((tokens[0], tokens[1..].to_vec()))
}

fn is_switch_token(token: &str) -> bool {
    token.starts_with('-')
        && token.len() > 1
        && token[1..].chars().all(|ch| ch.is_ascii_alphabetic())
}

fn parse_switches<'a>(
    tokens: &'a [&'a str],
    allowed: AllowedSwitches,
) -> Result<(Vec<&'a str>, ParsedSwitches), String> {
    let mut positionals = Vec::new();
    let mut switches = ParsedSwitches::default();

    for &token in tokens {
        if !is_switch_token(token) {
            positionals.push(token);
            continue;
        }
        for flag in token[1..].chars() {
            match flag {
                'h' if allowed.h => switches.h = true,
                'r' if allowed.r => switches.r = true,
                _ => return Err(format!("Unknown flag '-{flag}'.")),
            }
        }
    }

    Ok((positionals, switches))
}

fn parse_sphere_request(raw: &str, hollow_alias: bool) -> Result<SphereRequest, String> {
    let usage = "//sphere <pattern> <radius>[,<radius-y>,<radius-z>] [-r] [-h]";
    let (pattern, tail) = parse_pattern_and_tail(raw, usage)?;
    let (positionals, switches) = parse_switches(
        &tail,
        AllowedSwitches {
            h: !hollow_alias,
            r: true,
        },
    )?;
    if positionals.len() != 1 {
        return Err(format!("Usage: {usage}"));
    }

    let radii = parse_radius_list(positionals[0], 3)?;
    let (radius_x, radius_y, radius_z) = match radii.as_slice() {
        [one] => (one.max(0.0), one.max(0.0), one.max(0.0)),
        [x, y, z] => (x.max(0.0), y.max(0.0), z.max(0.0)),
        _ => {
            return Err(
                "Sphere radii must be one value or three comma-separated values.".to_string(),
            );
        }
    };

    Ok(SphereRequest {
        pattern: pattern.to_string(),
        radius_x,
        radius_y,
        radius_z,
        raised: switches.r,
        hollow: hollow_alias || switches.h,
    })
}

fn parse_cylinder_request(raw: &str, hollow_alias: bool) -> Result<CylinderRequest, String> {
    let usage = if hollow_alias {
        "//hcyl <pattern> <radius>[,<radius-z>] [height] [thickness]"
    } else {
        "//cyl <pattern> <radius>[,<radius-z>] [height] [-h]"
    };
    let (pattern, tail) = parse_pattern_and_tail(raw, usage)?;
    let (positionals, switches) = parse_switches(
        &tail,
        AllowedSwitches {
            h: !hollow_alias,
            r: false,
        },
    )?;
    if positionals.is_empty() || positionals.len() > 3 {
        return Err(format!("Usage: {usage}"));
    }

    let radii = parse_radius_list(positionals[0], 2)?;
    let (radius_x, radius_z) = match radii.as_slice() {
        [one] => (one.max(1.0), one.max(1.0)),
        [x, z] => (x.max(1.0), z.max(1.0)),
        _ => {
            return Err(
                "Cylinder radii must be one value or two comma-separated values.".to_string(),
            );
        }
    };
    let height = match positionals.get(1) {
        Some(value) => parse_i32(value, "height")?,
        None => 1,
    };
    let thickness = match positionals.get(2) {
        Some(value) => parse_f64(value, "thickness")?,
        None => 0.0,
    };
    if thickness < 0.0 {
        return Err("Thickness must be >= 0.".to_string());
    }
    if hollow_alias && (thickness > radius_x || thickness > radius_z) {
        return Err("Thickness cannot exceed the cylinder radii.".to_string());
    }

    Ok(CylinderRequest {
        pattern: pattern.to_string(),
        radius_x,
        radius_z,
        height,
        hollow: hollow_alias || switches.h,
        thickness,
    })
}

fn parse_pyramid_request(raw: &str, hollow_alias: bool) -> Result<PyramidRequest, String> {
    let usage = "//pyramid <pattern> <size> [-h]";
    let (pattern, tail) = parse_pattern_and_tail(raw, usage)?;
    let (positionals, switches) = parse_switches(
        &tail,
        AllowedSwitches {
            h: !hollow_alias,
            r: false,
        },
    )?;
    if positionals.len() != 1 {
        return Err(format!("Usage: {usage}"));
    }
    let size = parse_i32(positionals[0], "size")?;
    if size < 0 {
        return Err("Size must be >= 0.".to_string());
    }

    Ok(PyramidRequest {
        pattern: pattern.to_string(),
        size,
        hollow: hollow_alias || switches.h,
    })
}

fn parse_cone_request(raw: &str) -> Result<ConeRequest, String> {
    let usage = "//cone <pattern> <radius>[,<radius-z>] [height] [-h] [thickness]";
    let (pattern, tail) = parse_pattern_and_tail(raw, usage)?;
    let (positionals, switches) = parse_switches(&tail, AllowedSwitches { h: true, r: false })?;
    if positionals.is_empty() || positionals.len() > 3 {
        return Err(format!("Usage: {usage}"));
    }

    let radii = parse_radius_list(positionals[0], 2)?;
    let (radius_x, radius_z) = match radii.as_slice() {
        [one] => (one.max(1.0), one.max(1.0)),
        [x, z] => (x.max(1.0), z.max(1.0)),
        _ => return Err("Cone radii must be one value or two comma-separated values.".to_string()),
    };
    let height = match positionals.get(1) {
        Some(value) => parse_i32(value, "height")?,
        None => 1,
    };
    let thickness = match positionals.get(2) {
        Some(value) => parse_f64(value, "thickness")?,
        None => 1.0,
    };
    if thickness < 0.0 {
        return Err("Thickness must be >= 0.".to_string());
    }

    Ok(ConeRequest {
        pattern: pattern.to_string(),
        radius_x,
        radius_z,
        height,
        hollow: switches.h,
        thickness,
    })
}

fn parse_line_request(raw: &str, usage: &str) -> Result<LineRequest, String> {
    let (pattern, tail) = parse_pattern_and_tail(raw, usage)?;
    let (positionals, switches) = parse_switches(&tail, AllowedSwitches { h: true, r: false })?;
    if positionals.len() > 1 {
        return Err(format!("Usage: {usage}"));
    }
    let thickness = match positionals.first() {
        Some(value) => parse_f64(value, "thickness")?,
        None => 0.0,
    };
    if thickness < 0.0 {
        return Err("Thickness must be >= 0.".to_string());
    }

    Ok(LineRequest {
        pattern: pattern.to_string(),
        thickness,
        shell: switches.h,
    })
}

fn parse_radius_list(raw: &str, max_parts: usize) -> Result<Vec<f64>, String> {
    let parts: Vec<_> = raw.split(',').map(str::trim).collect();
    if parts.is_empty() || parts.len() > max_parts || parts.iter().any(|part| part.is_empty()) {
        return Err(format!("Invalid radius list '{raw}'."));
    }
    parts
        .into_iter()
        .map(|part| parse_f64(part, "radius"))
        .collect()
}

fn parse_i32(raw: &str, name: &str) -> Result<i32, String> {
    raw.parse::<i32>()
        .map_err(|_| format!("Expected an integer {name}, got '{raw}'."))
}

fn parse_f64(raw: &str, name: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .map_err(|_| format!("Expected a number for {name}, got '{raw}'."))
}

fn push_unique(seen: &mut HashSet<BlockPosKey>, positions: &mut Vec<BlockPos>, pos: BlockPos) {
    if pos.y < MIN_BUILD_Y || pos.y > MAX_BUILD_Y {
        return;
    }
    if seen.insert(pos.into()) {
        positions.push(pos);
    }
}

fn sphere_positions(
    center: BlockPos,
    radius_x: f64,
    radius_y: f64,
    radius_z: f64,
    filled: bool,
) -> Vec<BlockPos> {
    let radius_x = radius_x + 0.5;
    let radius_y = radius_y + 0.5;
    let radius_z = radius_z + 0.5;
    let inv_radius_x = 1.0 / radius_x;
    let inv_radius_y = 1.0 / radius_y;
    let inv_radius_z = 1.0 / radius_z;
    let ceil_radius_x = radius_x.ceil() as i32;
    let ceil_radius_y = radius_y.ceil() as i32;
    let ceil_radius_z = radius_z.ceil() as i32;

    let mut seen = HashSet::new();
    let mut positions = Vec::new();
    let mut next_xn = 0.0;

    'for_x: for x in 0..=ceil_radius_x {
        let xn = next_xn;
        let dx = xn * xn;
        next_xn = f64::from(x + 1) * inv_radius_x;
        let next_xn_sq = next_xn * next_xn;
        let xx = center.x + x;
        let neg_x = center.x - x;
        let mut next_zn = 0.0;

        'for_z: for z in 0..=ceil_radius_z {
            let zn = next_zn;
            let dz = zn * zn;
            let dxz = dx + dz;
            next_zn = f64::from(z + 1) * inv_radius_z;
            let next_zn_sq = next_zn * next_zn;
            let zz = center.z + z;
            let neg_z = center.z - z;
            let mut next_yn = 0.0;

            'for_y: for y in 0..=ceil_radius_y {
                let yn = next_yn;
                let dy = yn * yn;
                let dxyz = dxz + dy;
                next_yn = f64::from(y + 1) * inv_radius_y;

                if dxyz > 1.0 {
                    if y == 0 {
                        if z == 0 {
                            break 'for_x;
                        }
                        break 'for_z;
                    }
                    break 'for_y;
                }

                let next_yn_sq = next_yn * next_yn;
                let dxy = dx + dy;
                let dyz = dy + dz;
                if !filled
                    && next_xn_sq + dyz <= 1.0
                    && next_yn_sq + dxz <= 1.0
                    && next_zn_sq + dxy <= 1.0
                {
                    continue;
                }

                let yy = center.y + y;
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: xx,
                        y: yy,
                        z: zz,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: neg_x,
                        y: yy,
                        z: zz,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: xx,
                        y: yy,
                        z: neg_z,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: neg_x,
                        y: yy,
                        z: neg_z,
                    },
                );

                if y != 0 {
                    let neg_y = center.y - y;
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: neg_y,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: neg_y,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: neg_y,
                            z: neg_z,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: neg_y,
                            z: neg_z,
                        },
                    );
                }
            }
        }
    }

    positions
}

fn cylinder_positions(
    center: BlockPos,
    radius_x: f64,
    radius_z: f64,
    mut height: i32,
    thickness: f64,
    filled: bool,
) -> Vec<BlockPos> {
    let radius_x = radius_x + 0.5;
    let radius_z = radius_z + 0.5;
    if height == 0 {
        return Vec::new();
    }

    let mut base_y = center.y;
    if height < 0 {
        height = -height;
        base_y = base_y.saturating_sub(height);
    }
    if base_y < MIN_BUILD_Y {
        base_y = MIN_BUILD_Y;
    } else if base_y.saturating_add(height).saturating_sub(1) > MAX_BUILD_Y {
        height = MAX_BUILD_Y - base_y + 1;
    }
    if height <= 0 {
        return Vec::new();
    }

    let inv_radius_x = 1.0 / radius_x;
    let inv_radius_z = 1.0 / radius_z;
    let ceil_radius_x = radius_x.ceil() as i32;
    let ceil_radius_z = radius_z.ceil() as i32;
    let px = center.x;
    let pz = center.z;
    let mut seen = HashSet::new();
    let mut positions = Vec::new();

    if thickness != 0.0 {
        let min_inv_radius_x = 1.0 / (radius_x - thickness);
        let min_inv_radius_z = 1.0 / (radius_z - thickness);
        let mut next_xn = 0.0;
        let mut next_min_xn = 0.0;

        'for_x: for x in 0..=ceil_radius_x {
            let xn = next_xn;
            let dx2 = next_min_xn * next_min_xn;
            next_xn = f64::from(x + 1) * inv_radius_x;
            next_min_xn = f64::from(x + 1) * min_inv_radius_x;
            let mut next_zn = 0.0;
            let mut next_min_zn = 0.0;
            let x_sqr = xn * xn;
            let xx = px + x;
            let neg_x = px - x;

            'for_z: for z in 0..=ceil_radius_z {
                let zn = next_zn;
                let z_sqr = zn * zn;
                let distance_sq = x_sqr + z_sqr;
                if distance_sq > 1.0 {
                    if z == 0 {
                        break 'for_x;
                    }
                    break 'for_z;
                }

                let dz2 = next_min_zn * next_min_zn;
                next_zn = f64::from(z + 1) * inv_radius_z;
                next_min_zn = f64::from(z + 1) * min_inv_radius_z;

                if dz2 + next_min_xn * next_min_xn <= 1.0 && next_min_zn * next_min_zn + dx2 <= 1.0
                {
                    continue;
                }

                let zz = pz + z;
                let neg_z = pz - z;
                for y in 0..height {
                    let yy = base_y + y;
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: neg_z,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: neg_z,
                        },
                    );
                }
            }
        }
    } else {
        let mut next_xn = 0.0;
        'for_x: for x in 0..=ceil_radius_x {
            let xn = next_xn;
            next_xn = f64::from(x + 1) * inv_radius_x;
            let mut next_zn = 0.0;
            let x_sqr = xn * xn;
            let xx = px + x;
            let neg_x = px - x;

            'for_z: for z in 0..=ceil_radius_z {
                let zn = next_zn;
                let z_sqr = zn * zn;
                let distance_sq = x_sqr + z_sqr;
                if distance_sq > 1.0 {
                    if z == 0 {
                        break 'for_x;
                    }
                    break 'for_z;
                }

                next_zn = f64::from(z + 1) * inv_radius_z;
                if !filled && z_sqr + next_xn * next_xn <= 1.0 && next_zn * next_zn + x_sqr <= 1.0 {
                    continue;
                }

                let zz = pz + z;
                let neg_z = pz - z;
                for y in 0..height {
                    let yy = base_y + y;
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: neg_z,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: neg_z,
                        },
                    );
                }
            }
        }
    }

    positions
}

fn pyramid_positions(center: BlockPos, size: i32, filled: bool) -> Vec<BlockPos> {
    if size <= 0 {
        return Vec::new();
    }

    let mut seen = HashSet::new();
    let mut positions = Vec::new();

    for layer in 0..size {
        let extent = size - layer - 1;
        let yy = center.y + layer;
        if !(MIN_BUILD_Y..=MAX_BUILD_Y).contains(&yy) {
            continue;
        }
        for x in 0..=extent {
            let xx = center.x + x;
            let neg_x = center.x - x;
            for z in 0..=extent {
                if !(filled || z == extent || x == extent) {
                    continue;
                }
                let zz = center.z + z;
                let neg_z = center.z - z;
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: xx,
                        y: yy,
                        z: zz,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: neg_x,
                        y: yy,
                        z: zz,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: xx,
                        y: yy,
                        z: neg_z,
                    },
                );
                push_unique(
                    &mut seen,
                    &mut positions,
                    BlockPos {
                        x: neg_x,
                        y: yy,
                        z: neg_z,
                    },
                );
            }
        }
    }

    positions
}

fn cone_positions(
    center: BlockPos,
    radius_x: f64,
    radius_z: f64,
    height: i32,
    filled: bool,
    thickness: f64,
) -> Vec<BlockPos> {
    if height == 0 {
        return Vec::new();
    }

    let ceil_radius_x = radius_x.ceil() as i32;
    let ceil_radius_z = radius_z.ceil() as i32;
    let radius_x_pow = radius_x.powi(2);
    let radius_z_pow = radius_z.powi(2);
    let height_pow = f64::from(height).powi(2);
    let layers = height.abs();
    let mut seen = HashSet::new();
    let mut positions = Vec::new();

    for y in 0..layers {
        let y_adjust = f64::from(y - layers).powi(2) / height_pow;
        let yy = if height < 0 {
            center.y - y
        } else {
            center.y + y
        };
        if yy < MIN_BUILD_Y || yy > MAX_BUILD_Y {
            continue;
        }

        'for_x: for x in 0..=ceil_radius_x {
            let x_term = f64::from(x).powi(2) / radius_x_pow;
            for z in 0..=ceil_radius_z {
                let z_term = f64::from(z).powi(2) / radius_z_pow;
                let distance = x_term + z_term - y_adjust;
                if distance > 1.0 {
                    if z == 0 {
                        break 'for_x;
                    }
                    break;
                }

                if !filled {
                    let x_next =
                        (f64::from(x) + thickness).powi(2) / radius_x_pow + z_term - y_adjust;
                    let y_next = x_term + z_term
                        - (f64::from(y) + thickness - f64::from(layers)).powi(2) / radius_z_pow;
                    let z_next =
                        x_term + (f64::from(z) + thickness).powi(2) / height_pow - y_adjust;
                    if x_next <= 0.0
                        && z_next <= 0.0
                        && (y_next <= 0.0 && (f64::from(y) + thickness) != f64::from(layers))
                    {
                        continue;
                    }
                }

                if distance <= 0.0 {
                    let xx = center.x + x;
                    let neg_x = center.x - x;
                    let zz = center.z + z;
                    let neg_z = center.z - z;
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: zz,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: xx,
                            y: yy,
                            z: neg_z,
                        },
                    );
                    push_unique(
                        &mut seen,
                        &mut positions,
                        BlockPos {
                            x: neg_x,
                            y: yy,
                            z: neg_z,
                        },
                    );
                }
            }
        }
    }

    positions
}

fn line_positions(pos1: BlockPos, pos2: BlockPos, thickness: f64, filled: bool) -> Vec<BlockPos> {
    let samples = line_samples(pos1, pos2);
    if thickness == 0.0 {
        return samples
            .into_iter()
            .filter(|pos| (MIN_BUILD_Y..=MAX_BUILD_Y).contains(&pos.y))
            .collect();
    }

    let mut seen = HashSet::new();
    let mut positions = Vec::new();
    for sample in samples {
        for pos in sphere_positions(sample, thickness, thickness, thickness, filled) {
            push_unique(&mut seen, &mut positions, pos);
        }
    }
    positions
}

fn line_samples(pos1: BlockPos, pos2: BlockPos) -> Vec<BlockPos> {
    let x1 = pos1.x;
    let y1 = pos1.y;
    let z1 = pos1.z;
    let x2 = pos2.x;
    let y2 = pos2.y;
    let z2 = pos2.z;
    let dx = (x2 - x1).abs();
    let dy = (y2 - y1).abs();
    let dz = (z2 - z1).abs();

    let mut seen = HashSet::new();
    let mut positions = Vec::new();
    if dx + dy + dz == 0 {
        push_unique(&mut seen, &mut positions, pos1);
        return positions;
    }

    let dominant = dx.max(dy).max(dz);
    if dominant == dx {
        for step in 0..=dx {
            let x = x1 + step * if x2 >= x1 { 1 } else { -1 };
            let y = (f64::from(y1)
                + f64::from(step) * f64::from(dy) / f64::from(dx) * f64::from((y2 - y1).signum()))
            .round() as i32;
            let z = (f64::from(z1)
                + f64::from(step) * f64::from(dz) / f64::from(dx) * f64::from((z2 - z1).signum()))
            .round() as i32;
            push_unique(&mut seen, &mut positions, BlockPos { x, y, z });
        }
    } else if dominant == dy {
        for step in 0..=dy {
            let y = y1 + step * if y2 >= y1 { 1 } else { -1 };
            let x = (f64::from(x1)
                + f64::from(step) * f64::from(dx) / f64::from(dy) * f64::from((x2 - x1).signum()))
            .round() as i32;
            let z = (f64::from(z1)
                + f64::from(step) * f64::from(dz) / f64::from(dy) * f64::from((z2 - z1).signum()))
            .round() as i32;
            push_unique(&mut seen, &mut positions, BlockPos { x, y, z });
        }
    } else {
        for step in 0..=dz {
            let z = z1 + step * if z2 >= z1 { 1 } else { -1 };
            let y = (f64::from(y1)
                + f64::from(step) * f64::from(dy) / f64::from(dz) * f64::from((y2 - y1).signum()))
            .round() as i32;
            let x = (f64::from(x1)
                + f64::from(step) * f64::from(dx) / f64::from(dz) * f64::from((x2 - x1).signum()))
            .round() as i32;
            push_unique(&mut seen, &mut positions, BlockPos { x, y, z });
        }
    }

    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn sphere_parser_accepts_alias_and_flags() {
        assert_eq!(
            parse_sphere_request("stone 2,3,4 -r", true).unwrap(),
            SphereRequest {
                pattern: "stone".to_string(),
                radius_x: 2.0,
                radius_y: 3.0,
                radius_z: 4.0,
                raised: true,
                hollow: true,
            }
        );
    }

    #[test]
    fn cylinder_parser_rejects_oversized_hollow_thickness() {
        let err = parse_cylinder_request("stone 2 4 3", true).unwrap_err();
        assert!(err.contains("Thickness"));
    }

    #[test]
    fn cone_parser_accepts_negative_height_and_hollow_thickness() {
        assert_eq!(
            parse_cone_request("stone 2,3 -4 -h 1.5").unwrap(),
            ConeRequest {
                pattern: "stone".to_string(),
                radius_x: 2.0,
                radius_z: 3.0,
                height: -4,
                hollow: true,
                thickness: 1.5,
            }
        );
    }

    #[test]
    fn sphere_counts_match_fawe_small_radii() {
        assert_eq!(sphere_positions(at(0, 0, 0), 1.0, 1.0, 1.0, true).len(), 19);
        assert_eq!(sphere_positions(at(0, 0, 0), 2.0, 2.0, 2.0, true).len(), 81);
        assert_eq!(
            sphere_positions(at(0, 0, 0), 3.0, 3.0, 3.0, true).len(),
            179
        );
    }

    #[test]
    fn hollow_sphere_has_shell_only() {
        assert_eq!(
            sphere_positions(at(0, 0, 0), 3.0, 3.0, 3.0, false).len(),
            98
        );
        assert!(
            sphere_positions(at(0, 0, 0), 3.0, 3.0, 3.0, false).len()
                < sphere_positions(at(0, 0, 0), 3.0, 3.0, 3.0, true).len()
        );
    }

    #[test]
    fn cylinder_counts_match_fawe_small_radii() {
        assert_eq!(
            cylinder_positions(at(0, 0, 0), 1.0, 1.0, 1, 0.0, true).len(),
            9
        );
        assert_eq!(
            cylinder_positions(at(0, 0, 0), 2.0, 2.0, 1, 0.0, true).len(),
            21
        );
        assert_eq!(
            cylinder_positions(at(0, 0, 0), 2.0, 2.0, 3, 0.0, false).len(),
            36
        );
    }

    #[test]
    fn hcyl_thickness_changes_shell_width() {
        let thin = cylinder_positions(at(0, 0, 0), 4.0, 4.0, 1, 1.0, false).len();
        let thick = cylinder_positions(at(0, 0, 0), 4.0, 4.0, 1, 2.0, false).len();
        assert!(thick > thin);
    }

    #[test]
    fn cone_counts_match_fawe_small_cases() {
        assert_eq!(cone_positions(at(0, 0, 0), 1.0, 1.0, 1, true, 1.0).len(), 5);
        assert_eq!(
            cone_positions(at(0, 0, 0), 2.0, 2.0, 3, true, 1.0).len(),
            19
        );
        assert_eq!(
            cone_positions(at(0, 0, 0), 2.0, 2.0, 3, false, 1.0).len(),
            13
        );
        assert_eq!(
            cone_positions(at(0, 0, 0), 2.0, 2.0, -3, true, 1.0).len(),
            19
        );
    }

    #[test]
    fn pyramid_counts_match_fawe_small_sizes() {
        assert_eq!(pyramid_positions(at(0, 0, 0), 1, true).len(), 1);
        assert_eq!(pyramid_positions(at(0, 0, 0), 2, true).len(), 10);
        assert_eq!(pyramid_positions(at(0, 0, 0), 3, false).len(), 25);
    }

    #[test]
    fn line_samples_follow_fawe_dominant_axis_walk() {
        let got: Vec<_> = line_samples(at(0, 0, 0), at(3, 1, 0))
            .into_iter()
            .map(|pos| (pos.x, pos.y, pos.z))
            .collect();
        assert_eq!(got, vec![(0, 0, 0), (1, 0, 0), (2, 1, 0), (3, 1, 0)]);
    }
}
