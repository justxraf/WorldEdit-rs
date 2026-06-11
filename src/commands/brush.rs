//! `/brush` / `/br` - bind WorldEdit-style brushes to the held item.
//!
//! This implements the brush command surface that can be backed by Pumpkin's
//! current block APIs: shape brushes, clipboard stamping, simple terrain
//! smoothing, gravity, extinguish, raise/lower, splatter, and morph presets.
//! Commands that need entities, biomes, features, images, or FAWE's expression
//! engine are accepted as known brush names and report a precise unsupported
//! message instead of silently doing the wrong thing.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    common::BlockPos,
    events::{BlockBreakEvent, EventHandler, EventPriority, InteractAction, PlayerInteractEvent},
    logging::{self, LogLevel},
    player::{Hand, Player},
    text::TextComponent,
    world::{BlockChange, World},
};

use crate::{
    clipboard,
    history::{self, EditEntry},
    mapping,
    pattern::{BlockMask, BlockPattern},
};

use super::{batch_size, block_flags, permission_node, player_key, require_permission};

const MAX_RADIUS: f64 = 64.0;
const MAX_HEIGHT: i32 = 256;
const DEFAULT_RADIUS: f64 = 5.0;
const DEFAULT_HEIGHT: i32 = 1;
const DEFAULT_RANGE: f64 = 200.0;
const MIN_BUILD_Y: i32 = -64;
const MAX_BUILD_Y: i32 = 319;

const BRUSH_COMMAND_PERMISSION: &str = "worldedit-rs:command.brush";

pub fn register(context: &Context) {
    let args = CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
        .execute(BrushCommand);
    let command = Command::new(
        &[
            "brush".to_string(),
            "/brush".to_string(),
            "br".to_string(),
            "/br".to_string(),
        ],
        "Bind or configure a brush on your held item",
    )
    .execute(BrushCommand);
    command.then(args);
    context.register_command(command, BRUSH_COMMAND_PERMISSION);

    if let Err(e) =
        context.register_event_handler(BrushInteractHandler, EventPriority::Normal, true)
    {
        logging::log(
            LogLevel::Warn,
            &format!("WorldEdit-rs: failed to register brush interact handler: {e}"),
        );
    }
    if let Err(e) = context.register_event_handler(BrushBreakHandler, EventPriority::Normal, true) {
        logging::log(
            LogLevel::Warn,
            &format!("WorldEdit-rs: failed to register brush block-break handler: {e}"),
        );
    }
}

struct BrushCommand;

impl pumpkin_plugin_api::commands::CommandHandler for BrushCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Some(player) = sender.as_player() else {
            sender.send_error(TextComponent::text("Only players can use brush commands."));
            return Ok(0);
        };
        let Some(key) = player_key(&sender) else {
            sender.send_error(TextComponent::text("Could not determine your identity."));
            return Ok(0);
        };

        let raw = match args.get_value("args") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => {
                send_brush_usage(&sender);
                return Ok(0);
            }
        };

        let command = match parse_brush_command(&raw) {
            Ok(command) => command,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        match command {
            ParsedBrushCommand::Bind(binding) => {
                if require_permission(&sender, &server, binding.kind.permission()).is_err() {
                    return Ok(0);
                }
                let Some(item) = held_item_key(&player) else {
                    sender.send_error(TextComponent::text(
                        "Hold an item, then run the brush command again.",
                    ));
                    return Ok(0);
                };
                let summary = binding.kind.summary();
                let range = binding.range;
                BRUSHES.with_borrow_mut(|map| {
                    let tools = map.entry(key).or_default();
                    tools.bindings.insert(item.clone(), binding);
                });
                sender.send_message(TextComponent::text(&format!(
                    "Bound {summary} to {item}. Range: {range:.0} blocks."
                )));
                Ok(1)
            }
            ParsedBrushCommand::Unbind => {
                let Some(item) = held_item_key(&player) else {
                    sender.send_error(TextComponent::text("Hold the brush item to unbind it."));
                    return Ok(0);
                };
                let removed = BRUSHES.with_borrow_mut(|map| {
                    map.get_mut(&key)
                        .and_then(|tools| tools.bindings.remove(&item))
                        .is_some()
                });
                if removed {
                    sender
                        .send_message(TextComponent::text(&format!("Unbound brush from {item}.")));
                    Ok(1)
                } else {
                    sender.send_error(TextComponent::text("That item has no brush bound."));
                    Ok(0)
                }
            }
            ParsedBrushCommand::Setting(setting) => {
                if require_permission(&sender, &server, setting.permission()).is_err() {
                    return Ok(0);
                }
                let Some(item) = held_item_key(&player) else {
                    sender.send_error(TextComponent::text("Hold a brush item to configure it."));
                    return Ok(0);
                };
                let result = BRUSHES.with_borrow_mut(|map| {
                    map.get_mut(&key)
                        .and_then(|tools| tools.bindings.get_mut(&item))
                        .map(|binding| setting.apply(binding))
                });
                match result {
                    Some(Ok(message)) => {
                        sender.send_message(TextComponent::text(&message));
                        Ok(1)
                    }
                    Some(Err(message)) => {
                        sender.send_error(TextComponent::text(&message));
                        Ok(0)
                    }
                    None => {
                        sender.send_error(TextComponent::text("That item has no brush bound."));
                        Ok(0)
                    }
                }
            }
            ParsedBrushCommand::List => {
                let list = BRUSHES.with_borrow(|map| {
                    map.get(&key).map_or_else(Vec::new, |tools| {
                        tools
                            .bindings
                            .iter()
                            .map(|(item, binding)| format!("{item}: {}", binding.kind.summary()))
                            .collect()
                    })
                });
                if list.is_empty() {
                    sender.send_message(TextComponent::text("You have no brushes bound."));
                } else {
                    sender.send_message(TextComponent::text(&format!(
                        "Bound brushes: {}.",
                        list.join("; ")
                    )));
                }
                Ok(1)
            }
            ParsedBrushCommand::Unsupported { name, reason } => {
                sender.send_error(TextComponent::text(&format!(
                    "Brush '{name}' is recognized, but {reason}"
                )));
                Ok(0)
            }
        }
    }
}

#[derive(Default)]
struct PlayerBrushes {
    bindings: HashMap<String, BrushBinding>,
}

thread_local! {
    static BRUSHES: RefCell<HashMap<String, PlayerBrushes>> = RefCell::new(HashMap::new());
}

#[derive(Clone)]
struct BrushBinding {
    kind: BrushKind,
    mask: Option<BlockMask>,
    range: f64,
}

#[derive(Clone)]
enum BrushKind {
    Sphere {
        pattern: BlockPattern,
        radius: f64,
        hollow: bool,
    },
    Cylinder {
        pattern: BlockPattern,
        radius: f64,
        height: i32,
        hollow: bool,
    },
    Cuboid {
        pattern: BlockPattern,
        radius: i32,
    },
    Clipboard {
        skip_air: bool,
        paste_at_origin: bool,
    },
    Smooth {
        radius: i32,
        iterations: u32,
        height_mask: Option<BlockMask>,
    },
    Gravity {
        radius: i32,
        height: i32,
    },
    Extinguish {
        radius: i32,
    },
    Splatter {
        pattern: BlockPattern,
        radius: f64,
        decay: u32,
    },
    Raise {
        shape: Shape,
        radius: i32,
        lower: bool,
    },
    Morph {
        radius: i32,
        min_erode_faces: u8,
        erode_iterations: u32,
        min_dilate_faces: u8,
        dilate_iterations: u32,
    },
    Snow {
        shape: Shape,
        radius: i32,
        stack: bool,
    },
}

impl BrushKind {
    fn permission(&self) -> &'static str {
        match self {
            Self::Sphere { .. } => "worldedit.brush.sphere",
            Self::Cylinder { .. } => "worldedit.brush.cylinder",
            Self::Cuboid { .. } => "worldedit.brush.set",
            Self::Clipboard { .. } => "worldedit.brush.clipboard",
            Self::Smooth { .. } => "worldedit.brush.smooth",
            Self::Gravity { .. } => "worldedit.brush.gravity",
            Self::Extinguish { .. } => "worldedit.brush.ex",
            Self::Splatter { .. } => "worldedit.brush.splatter",
            Self::Raise { lower, .. } if *lower => "worldedit.brush.lower",
            Self::Raise { .. } => "worldedit.brush.raise",
            Self::Morph { .. } => "worldedit.brush.morph",
            Self::Snow { .. } => "worldedit.brush.snow",
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Sphere {
                pattern,
                radius,
                hollow,
            } => format!(
                "{}sphere brush, radius {radius:.1}, pattern {}",
                if *hollow { "hollow " } else { "" },
                pattern.description()
            ),
            Self::Cylinder {
                pattern,
                radius,
                height,
                hollow,
            } => format!(
                "{}cylinder brush, radius {radius:.1}, height {height}, pattern {}",
                if *hollow { "hollow " } else { "" },
                pattern.description()
            ),
            Self::Cuboid { pattern, radius } => {
                format!(
                    "cuboid set brush, radius {radius}, pattern {}",
                    pattern.description()
                )
            }
            Self::Clipboard {
                skip_air,
                paste_at_origin,
            } => format!(
                "clipboard brush{}{}",
                if *skip_air { ", skipping air" } else { "" },
                if *paste_at_origin {
                    ", origin at target"
                } else {
                    ", centered"
                }
            ),
            Self::Smooth {
                radius, iterations, ..
            } => {
                format!("smooth brush, radius {radius}, {iterations} iterations")
            }
            Self::Gravity { radius, height } => {
                format!("gravity brush, radius {radius}, height {height}")
            }
            Self::Extinguish { radius } => format!("extinguish brush, radius {radius}"),
            Self::Splatter {
                pattern,
                radius,
                decay,
            } => format!(
                "splatter brush, radius {radius:.1}, decay {decay}, pattern {}",
                pattern.description()
            ),
            Self::Raise {
                shape,
                radius,
                lower,
            } => format!(
                "{} brush, shape {}, radius {radius}",
                if *lower { "lower" } else { "raise" },
                shape.name()
            ),
            Self::Morph {
                radius,
                min_erode_faces,
                erode_iterations,
                min_dilate_faces,
                dilate_iterations,
            } => format!(
                "morph brush, radius {radius}, erode {erode_iterations}x/{min_erode_faces}, dilate {dilate_iterations}x/{min_dilate_faces}"
            ),
            Self::Snow {
                shape,
                radius,
                stack,
            } => format!(
                "snow brush, shape {}, radius {radius}{}",
                shape.name(),
                if *stack { ", stacking snow" } else { "" }
            ),
        }
    }

    fn set_radius(&mut self, size: i32) -> Result<(), String> {
        let radius = clamp_radius(size as f64)? as i32;
        match self {
            Self::Sphere { radius: r, .. }
            | Self::Cylinder { radius: r, .. }
            | Self::Splatter { radius: r, .. } => *r = radius as f64,
            Self::Cuboid { radius: r, .. }
            | Self::Smooth { radius: r, .. }
            | Self::Gravity { radius: r, .. }
            | Self::Extinguish { radius: r }
            | Self::Raise { radius: r, .. }
            | Self::Morph { radius: r, .. }
            | Self::Snow { radius: r, .. } => *r = radius,
            Self::Clipboard { .. } => {
                return Err("Clipboard brushes do not have a radius.".to_string());
            }
        }
        Ok(())
    }

    fn set_material(&mut self, pattern: BlockPattern) -> Result<(), String> {
        match self {
            Self::Sphere { pattern: p, .. }
            | Self::Cylinder { pattern: p, .. }
            | Self::Cuboid { pattern: p, .. }
            | Self::Splatter { pattern: p, .. } => {
                *p = pattern;
                Ok(())
            }
            _ => Err("This brush does not use a block material.".to_string()),
        }
    }
}

#[derive(Clone, Copy)]
enum Shape {
    Sphere,
    Cylinder,
    Cuboid,
}

impl Shape {
    fn parse(input: &str) -> Option<Self> {
        match input.to_ascii_lowercase().as_str() {
            "sphere" | "s" | "ball" => Some(Self::Sphere),
            "cylinder" | "cyl" | "c" => Some(Self::Cylinder),
            "cuboid" | "cube" | "box" => Some(Self::Cuboid),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Sphere => "sphere",
            Self::Cylinder => "cylinder",
            Self::Cuboid => "cuboid",
        }
    }
}

enum ParsedBrushCommand {
    Bind(BrushBinding),
    Unbind,
    Setting(BrushSetting),
    List,
    Unsupported { name: String, reason: &'static str },
}

enum BrushSetting {
    Size(i32),
    Material(BlockPattern),
    Mask(Option<BlockMask>),
    Range(f64),
    TraceMask,
}

impl BrushSetting {
    fn permission(&self) -> &'static str {
        match self {
            Self::Size(_) => "worldedit.brush.options.size",
            Self::Material(_) => "worldedit.brush.options.material",
            Self::Mask(_) => "worldedit.brush.options.mask",
            Self::Range(_) => "worldedit.brush.options.range",
            Self::TraceMask => "worldedit.brush.options.tracemask",
        }
    }

    fn apply(self, binding: &mut BrushBinding) -> Result<String, String> {
        match self {
            Self::Size(size) => {
                binding.kind.set_radius(size)?;
                Ok(format!("Brush size set to {size}."))
            }
            Self::Material(pattern) => {
                let description = pattern.description().to_string();
                binding.kind.set_material(pattern)?;
                Ok(format!("Brush material set to {description}."))
            }
            Self::Mask(mask) => {
                binding.mask = mask;
                Ok("Brush mask updated.".to_string())
            }
            Self::Range(range) => {
                binding.range = range;
                Ok(format!("Brush range set to {range:.0} blocks."))
            }
            Self::TraceMask => Err(
                "Trace masks need a richer ray trace mask than Pumpkin exposes yet.".to_string(),
            ),
        }
    }
}

fn parse_brush_command(raw: &str) -> Result<ParsedBrushCommand, String> {
    let tokens = tokenize(raw);
    if tokens.is_empty() {
        return Ok(ParsedBrushCommand::List);
    }
    let name = tokens[0].to_ascii_lowercase();
    let args = &tokens[1..];
    match name.as_str() {
        "none" | "unbind" => Ok(ParsedBrushCommand::Unbind),
        "list" | "info" => Ok(ParsedBrushCommand::List),
        "size" => {
            let size = parse_i32(args.first(), "size")?;
            Ok(ParsedBrushCommand::Setting(BrushSetting::Size(size)))
        }
        "material" | "mat" => {
            let pattern = parse_required_pattern(args.first())?;
            Ok(ParsedBrushCommand::Setting(BrushSetting::Material(pattern)))
        }
        "mask" => {
            let mask = if args.first().is_none_or(|s| s.eq_ignore_ascii_case("none")) {
                None
            } else {
                Some(BlockMask::parse(&args.join(","))?)
            };
            Ok(ParsedBrushCommand::Setting(BrushSetting::Mask(mask)))
        }
        "range" => {
            let range = parse_f64(args.first(), "range")?;
            if !range.is_finite() || range <= 0.0 {
                return Err("Brush range must be positive.".to_string());
            }
            Ok(ParsedBrushCommand::Setting(BrushSetting::Range(
                range.min(512.0),
            )))
        }
        "tracemask" | "targetmask" => {
            if !args.first().is_none_or(|s| s.eq_ignore_ascii_case("none")) {
                BlockMask::parse(&args.join(","))?;
            }
            Ok(ParsedBrushCommand::Setting(BrushSetting::TraceMask))
        }
        "sphere" | "s" => parse_sphere(args),
        "cylinder" | "cyl" | "c" => parse_cylinder(args),
        "set" => parse_set(args),
        "clipboard" | "copy" => parse_clipboard(args),
        "smooth" => parse_smooth(args),
        "gravity" | "grav" => parse_gravity(args),
        "extinguish" | "ex" => parse_extinguish(args),
        "splatter" | "splat" => parse_splatter(args),
        "raise" => parse_raise_lower(args, false),
        "lower" => parse_raise_lower(args, true),
        "erode" => parse_morph_preset(args, 3, 1, 5, 1),
        "dilate" => parse_morph_preset(args, 5, 0, 3, 1),
        "morph" => parse_morph(args),
        "snow" => parse_snow(args),
        "forest" | "butcher" | "kill" | "paint" | "snowsmooth" | "heightmap" | "feature"
        | "apply" | "deform" | "biome" => Ok(ParsedBrushCommand::Unsupported {
            name,
            reason: "it needs entities, biomes, generation features, images, or FAWE expressions that this plugin cannot access yet.",
        }),
        _ => Err(format!("Unknown brush '{name}'.")),
    }
}

fn parse_sphere(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let (hollow, rest) = consume_hollow(args);
    let pattern = parse_required_pattern(rest.first())?;
    let radius = parse_optional_radius(rest.get(1), DEFAULT_RADIUS)?;
    Ok(bind(BrushKind::Sphere {
        pattern,
        radius,
        hollow,
    }))
}

fn parse_cylinder(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let (hollow, rest) = consume_hollow(args);
    let pattern = parse_required_pattern(rest.first())?;
    let radius = parse_optional_radius(rest.get(1), DEFAULT_RADIUS)?;
    let height = parse_optional_i32(rest.get(2), DEFAULT_HEIGHT)?.clamp(1, MAX_HEIGHT);
    Ok(bind(BrushKind::Cylinder {
        pattern,
        radius,
        height,
        hollow,
    }))
}

fn parse_set(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let shape = args.first().and_then(|s| Shape::parse(s)).ok_or_else(|| {
        "Usage: //brush set <sphere|cylinder|cuboid> [radius] <pattern>".to_string()
    })?;
    let (radius, pattern_index) = if args.len() >= 3 {
        (parse_optional_i32(args.get(1), DEFAULT_RADIUS as i32)?, 2)
    } else {
        (DEFAULT_RADIUS as i32, 1)
    };
    let pattern = parse_required_pattern(args.get(pattern_index))?;
    match shape {
        Shape::Sphere => Ok(bind(BrushKind::Sphere {
            pattern,
            radius: clamp_radius(radius as f64)?,
            hollow: false,
        })),
        Shape::Cylinder => Ok(bind(BrushKind::Cylinder {
            pattern,
            radius: clamp_radius(radius as f64)?,
            height: DEFAULT_HEIGHT,
            hollow: false,
        })),
        Shape::Cuboid => Ok(bind(BrushKind::Cuboid {
            pattern,
            radius: clamp_radius(radius as f64)? as i32,
        })),
    }
}

fn parse_clipboard(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut skip_air = false;
    let mut paste_at_origin = false;
    for arg in args {
        let Some(flags) = arg.strip_prefix('-') else {
            return Err(format!("Unexpected clipboard brush argument '{arg}'."));
        };
        for flag in flags.chars() {
            match flag {
                'a' => skip_air = true,
                'o' => paste_at_origin = true,
                'b' | 'e' | 'v' | 'm' => {
                    return Err(format!(
                        "Clipboard brush flag '-{flag}' needs entity, biome, structure void, or source-mask support that is not implemented yet."
                    ));
                }
                _ => return Err(format!("Unknown clipboard brush flag '-{flag}'.")),
            }
        }
    }
    Ok(bind(BrushKind::Clipboard {
        skip_air,
        paste_at_origin,
    }))
}

fn parse_smooth(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let radius = parse_optional_i32(args.first(), DEFAULT_RADIUS as i32)?;
    let iterations = parse_optional_i32(args.get(1), 4)?.clamp(1, 20) as u32;
    let height_mask = match args.get(2) {
        Some(mask) => Some(BlockMask::parse(mask)?),
        None => None,
    };
    Ok(bind(BrushKind::Smooth {
        radius: clamp_radius(radius as f64)? as i32,
        iterations,
        height_mask,
    }))
}

fn parse_gravity(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let radius = parse_optional_i32(args.first(), DEFAULT_RADIUS as i32)?;
    let mut height = MAX_HEIGHT;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" => {
                height = parse_i32(args.get(i + 1), "height")?.clamp(1, MAX_HEIGHT);
                i += 2;
            }
            other => return Err(format!("Unexpected gravity brush argument '{other}'.")),
        }
    }
    Ok(bind(BrushKind::Gravity {
        radius: clamp_radius(radius as f64)? as i32,
        height,
    }))
}

fn parse_extinguish(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let radius = parse_optional_i32(args.first(), DEFAULT_RADIUS as i32)?;
    Ok(bind(BrushKind::Extinguish {
        radius: clamp_radius(radius as f64)? as i32,
    }))
}

fn parse_splatter(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = parse_optional_radius(args.get(1), DEFAULT_RADIUS)?;
    let decay = parse_optional_i32(args.get(2), 3)?.clamp(0, 10) as u32;
    Ok(bind(BrushKind::Splatter {
        pattern,
        radius,
        decay,
    }))
}

fn parse_raise_lower(args: &[String], lower: bool) -> Result<ParsedBrushCommand, String> {
    let shape = args.first().and_then(|s| Shape::parse(s)).ok_or_else(|| {
        format!(
            "Usage: //brush {} <sphere|cylinder|cuboid> [radius]",
            if lower { "lower" } else { "raise" }
        )
    })?;
    let radius = parse_optional_i32(args.get(1), DEFAULT_RADIUS as i32)?;
    Ok(bind(BrushKind::Raise {
        shape,
        radius: clamp_radius(radius as f64)? as i32,
        lower,
    }))
}

fn parse_morph_preset(
    args: &[String],
    min_erode_faces: u8,
    erode_iterations: u32,
    min_dilate_faces: u8,
    dilate_iterations: u32,
) -> Result<ParsedBrushCommand, String> {
    let radius = parse_optional_i32(args.first(), DEFAULT_RADIUS as i32)?;
    Ok(bind(BrushKind::Morph {
        radius: clamp_radius(radius as f64)? as i32,
        min_erode_faces,
        erode_iterations,
        min_dilate_faces,
        dilate_iterations,
    }))
}

fn parse_morph(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let radius = parse_optional_i32(args.first(), DEFAULT_RADIUS as i32)?;
    let min_erode_faces = parse_optional_i32(args.get(1), 3)?.clamp(0, 6) as u8;
    let erode_iterations = parse_optional_i32(args.get(2), 1)?.clamp(0, 20) as u32;
    let min_dilate_faces = parse_optional_i32(args.get(3), 3)?.clamp(0, 6) as u8;
    let dilate_iterations = parse_optional_i32(args.get(4), 1)?.clamp(0, 20) as u32;
    Ok(bind(BrushKind::Morph {
        radius: clamp_radius(radius as f64)? as i32,
        min_erode_faces,
        erode_iterations,
        min_dilate_faces,
        dilate_iterations,
    }))
}

fn parse_snow(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut stack = false;
    let rest = if args.first().is_some_and(|s| s == "-s") {
        stack = true;
        &args[1..]
    } else {
        args
    };
    let shape = rest
        .first()
        .and_then(|s| Shape::parse(s))
        .ok_or_else(|| "Usage: //brush snow [-s] <sphere|cylinder|cuboid> [radius]".to_string())?;
    let radius = parse_optional_i32(rest.get(1), DEFAULT_RADIUS as i32)?;
    Ok(bind(BrushKind::Snow {
        shape,
        radius: clamp_radius(radius as f64)? as i32,
        stack,
    }))
}

fn bind(kind: BrushKind) -> ParsedBrushCommand {
    ParsedBrushCommand::Bind(BrushBinding {
        kind,
        mask: None,
        range: DEFAULT_RANGE,
    })
}

fn consume_hollow(args: &[String]) -> (bool, &[String]) {
    if args.first().is_some_and(|s| s == "-h") {
        (true, &args[1..])
    } else {
        (false, args)
    }
}

fn parse_required_pattern(raw: Option<&String>) -> Result<BlockPattern, String> {
    let Some(raw) = raw else {
        return Err("Expected a block pattern.".to_string());
    };
    BlockPattern::parse(raw)
}

fn parse_optional_radius(raw: Option<&String>, default: f64) -> Result<f64, String> {
    clamp_radius(match raw {
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid radius '{raw}'."))?,
        None => default,
    })
}

fn parse_optional_i32(raw: Option<&String>, default: i32) -> Result<i32, String> {
    match raw {
        Some(raw) => raw
            .parse::<i32>()
            .map_err(|_| format!("Invalid number '{raw}'.")),
        None => Ok(default),
    }
}

fn parse_i32(raw: Option<&String>, name: &str) -> Result<i32, String> {
    raw.ok_or_else(|| format!("Expected {name}."))
        .and_then(|raw| {
            raw.parse::<i32>()
                .map_err(|_| format!("Invalid {name} '{raw}'."))
        })
}

fn parse_f64(raw: Option<&String>, name: &str) -> Result<f64, String> {
    raw.ok_or_else(|| format!("Expected {name}."))
        .and_then(|raw| {
            raw.parse::<f64>()
                .map_err(|_| format!("Invalid {name} '{raw}'."))
        })
}

fn clamp_radius(radius: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Brush radius must be positive.".to_string());
    }
    Ok(radius.min(MAX_RADIUS))
}

fn tokenize(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

fn held_item_key(player: &Player) -> Option<String> {
    player
        .get_item_in_hand(Hand::Right)
        .map(|stack| stack.get_registry_key())
}

fn send_brush_usage(sender: &CommandSender) {
    sender.send_error(TextComponent::text(
        "Usage: //brush <sphere|cylinder|set|clipboard|smooth|gravity|extinguish|splatter|raise|lower|erode|dilate|morph|snow|none|list|size|material|mask|range> ...",
    ));
}

struct BrushInteractHandler;

impl EventHandler<PlayerInteractEvent> for BrushInteractHandler {
    fn handle(
        &self,
        _server: pumpkin_plugin_api::Server,
        mut data: pumpkin_plugin_api::events::EventData<PlayerInteractEvent>,
    ) -> pumpkin_plugin_api::events::EventData<PlayerInteractEvent> {
        if !matches!(
            data.action,
            InteractAction::RightClickBlock | InteractAction::LeftClickBlock
        ) {
            return data;
        }
        let Some(target) = data.clicked_pos else {
            return data;
        };
        if trigger_player_brush(&data.player, target) {
            data.cancelled = true;
        }
        data
    }
}

struct BrushBreakHandler;

impl EventHandler<BlockBreakEvent> for BrushBreakHandler {
    fn handle(
        &self,
        _server: pumpkin_plugin_api::Server,
        mut data: pumpkin_plugin_api::events::EventData<BlockBreakEvent>,
    ) -> pumpkin_plugin_api::events::EventData<BlockBreakEvent> {
        let Some(player) = &data.player else {
            return data;
        };
        if trigger_player_brush(player, data.block_pos) {
            data.cancelled = true;
        }
        data
    }
}

fn trigger_player_brush(player: &Player, clicked: BlockPos) -> bool {
    let Some(item) = held_item_key(player) else {
        return false;
    };
    let key = player.get_name();
    let Some(binding) = BRUSHES.with_borrow(|map| {
        map.get(&key)
            .and_then(|tools| tools.bindings.get(&item))
            .cloned()
    }) else {
        return false;
    };
    if !player.has_permission(&permission_node(binding.kind.permission())) {
        player.send_system_message(
            TextComponent::text("You do not have permission to use this brush."),
            false,
        );
        return true;
    }

    let target = match player.as_entity().raycast(binding.range, false) {
        Some(hit) => hit.pos,
        None => clicked,
    };
    let world = player.get_world();
    let started = std::time::Instant::now();
    let changed = apply_brush(&key, &world, target, &binding);
    logging::log(
        LogLevel::Info,
        &format!(
            "WorldEdit-rs: brush {} changed {changed} blocks in {:?}.",
            binding.kind.summary(),
            started.elapsed()
        ),
    );
    player.send_system_message(
        TextComponent::text(&format!("Brush changed {changed} blocks.")),
        true,
    );
    true
}

fn apply_brush(key: &str, world: &World, target: BlockPos, binding: &BrushBinding) -> usize {
    match &binding.kind {
        BrushKind::Sphere {
            pattern,
            radius,
            hollow,
        } => apply_pattern_positions(
            key,
            world,
            sphere_positions(target, *radius, *hollow),
            pattern,
            binding.mask.as_ref(),
        ),
        BrushKind::Cylinder {
            pattern,
            radius,
            height,
            hollow,
        } => apply_pattern_positions(
            key,
            world,
            cylinder_positions(target, *radius, *height, *hollow),
            pattern,
            binding.mask.as_ref(),
        ),
        BrushKind::Cuboid { pattern, radius } => apply_pattern_positions(
            key,
            world,
            cuboid_positions(target, *radius),
            pattern,
            binding.mask.as_ref(),
        ),
        BrushKind::Clipboard {
            skip_air,
            paste_at_origin,
        } => apply_clipboard(
            key,
            world,
            target,
            *skip_air,
            *paste_at_origin,
            binding.mask.as_ref(),
        ),
        BrushKind::Smooth {
            radius,
            iterations,
            height_mask,
        } => apply_smooth(
            key,
            world,
            target,
            *radius,
            *iterations,
            height_mask.as_ref(),
            binding.mask.as_ref(),
        ),
        BrushKind::Gravity { radius, height } => {
            apply_gravity(key, world, target, *radius, *height, binding.mask.as_ref())
        }
        BrushKind::Extinguish { radius } => {
            apply_extinguish(key, world, target, *radius, binding.mask.as_ref())
        }
        BrushKind::Splatter {
            pattern,
            radius,
            decay,
        } => apply_pattern_positions(
            key,
            world,
            splatter_positions(target, *radius, *decay),
            pattern,
            binding.mask.as_ref(),
        ),
        BrushKind::Raise {
            shape,
            radius,
            lower,
        } => apply_raise_lower(
            key,
            world,
            target,
            *shape,
            *radius,
            *lower,
            binding.mask.as_ref(),
        ),
        BrushKind::Morph {
            radius,
            min_erode_faces,
            erode_iterations,
            min_dilate_faces,
            dilate_iterations,
        } => apply_morph(
            key,
            world,
            target,
            *radius,
            *min_erode_faces,
            *erode_iterations,
            *min_dilate_faces,
            *dilate_iterations,
            binding.mask.as_ref(),
        ),
        BrushKind::Snow {
            shape,
            radius,
            stack,
        } => apply_snow(
            key,
            world,
            target,
            *shape,
            *radius,
            *stack,
            binding.mask.as_ref(),
        ),
    }
}

fn apply_pattern_positions(
    key: &str,
    world: &World,
    positions: Vec<BlockPos>,
    pattern: &BlockPattern,
    mask: Option<&BlockMask>,
) -> usize {
    let mut entry = EditEntry::default();
    let mut changed = 0usize;
    for batch in positions.chunks(batch_size()) {
        let mut changes = Vec::with_capacity(batch.len());
        for &pos in batch {
            let before = world.get_block_state_id(pos);
            if mask.is_some_and(|mask| !mask.matches(before)) {
                continue;
            }
            let after = pattern.state_at(pos, before);
            if before == after {
                continue;
            }
            entry.changes.push((pos, before, after));
            changes.push(BlockChange { pos, state: after });
        }
        changed += changes.len();
        if !changes.is_empty() {
            world.set_block_states(&changes, block_flags());
        }
    }
    history::push(key, entry);
    changed
}

fn apply_clipboard(
    key: &str,
    world: &World,
    target: BlockPos,
    skip_air: bool,
    paste_at_origin: bool,
    mask: Option<&BlockMask>,
) -> usize {
    let Some(buffer) = clipboard::get(key) else {
        return 0;
    };
    let paste_origin = if paste_at_origin {
        target
    } else if let Some(bounds) = buffer.bounds(!skip_air) {
        let center = BlockPos {
            x: bounds.min.x + (bounds.max.x - bounds.min.x) / 2,
            y: bounds.min.y + (bounds.max.y - bounds.min.y) / 2,
            z: bounds.min.z + (bounds.max.z - bounds.min.z) / 2,
        };
        BlockPos {
            x: target.x - (center.x - buffer.origin.x),
            y: target.y - (center.y - buffer.origin.y),
            z: target.z - (center.z - buffer.origin.z),
        }
    } else {
        target
    };

    let mut entry = EditEntry::default();
    let mut changed = 0usize;
    for batch in buffer.blocks.chunks(batch_size()) {
        let mut changes = Vec::with_capacity(batch.len());
        for &(offset, state) in batch {
            if skip_air && state == 0 {
                continue;
            }
            let pos = BlockPos {
                x: paste_origin.x + offset.0,
                y: paste_origin.y + offset.1,
                z: paste_origin.z + offset.2,
            };
            let before = world.get_block_state_id(pos);
            if mask.is_some_and(|mask| !mask.matches(before)) || before == state {
                continue;
            }
            entry.changes.push((pos, before, state));
            changes.push(BlockChange { pos, state });
        }
        changed += changes.len();
        if !changes.is_empty() {
            world.set_block_states(&changes, block_flags());
        }
    }
    history::push(key, entry);
    changed
}

fn apply_smooth(
    key: &str,
    world: &World,
    target: BlockPos,
    radius: i32,
    iterations: u32,
    height_mask: Option<&BlockMask>,
    mask: Option<&BlockMask>,
) -> usize {
    let mut heights = HashMap::<(i32, i32), i32>::new();
    let mut states = HashMap::<(i32, i32), u16>::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            if let Some((y, state)) = top_solid_in_column(
                world,
                target.x + dx,
                target.z + dz,
                target.y - radius,
                target.y + radius,
                height_mask,
            ) {
                heights.insert((dx, dz), y);
                states.insert((dx, dz), state);
            }
        }
    }

    for _ in 0..iterations {
        let current = heights.clone();
        for (&coord, height) in heights.iter_mut() {
            let mut total = 0i32;
            let mut count = 0i32;
            for nz in -1..=1 {
                for nx in -1..=1 {
                    if let Some(value) = current.get(&(coord.0 + nx, coord.1 + nz)) {
                        total += *value;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                *height = (total as f64 / count as f64).round() as i32;
            }
        }
    }

    let mut entry = EditEntry::default();
    for ((dx, dz), new_y) in heights {
        let x = target.x + dx;
        let z = target.z + dz;
        let Some((old_y, top_state)) = top_solid_in_column(
            world,
            x,
            z,
            target.y - radius,
            target.y + radius,
            height_mask,
        ) else {
            continue;
        };
        if new_y > old_y {
            for y in old_y + 1..=new_y.min(MAX_BUILD_Y) {
                push_change(world, &mut entry, BlockPos { x, y, z }, top_state, mask);
            }
        } else if new_y < old_y {
            for y in (new_y.max(MIN_BUILD_Y) + 1)..=old_y {
                push_change(world, &mut entry, BlockPos { x, y, z }, 0, mask);
            }
        }
    }
    commit_entry(key, world, entry)
}

fn apply_gravity(
    key: &str,
    world: &World,
    target: BlockPos,
    radius: i32,
    height: i32,
    mask: Option<&BlockMask>,
) -> usize {
    let min_y = (target.y - height / 2).max(MIN_BUILD_Y);
    let max_y = (target.y + height / 2).min(MAX_BUILD_Y);
    let mut entry = EditEntry::default();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius {
                continue;
            }
            let x = target.x + dx;
            let z = target.z + dz;
            let mut solids = Vec::new();
            for y in min_y..=max_y {
                let state = world.get_block_state_id(BlockPos { x, y, z });
                if state != 0 && mask.is_none_or(|mask| mask.matches(state)) {
                    solids.push(state);
                }
            }
            for (i, y) in (min_y..=max_y).enumerate() {
                let after = solids.get(i).copied().unwrap_or(0);
                push_change(world, &mut entry, BlockPos { x, y, z }, after, None);
            }
        }
    }
    commit_entry(key, world, entry)
}

fn apply_extinguish(
    key: &str,
    world: &World,
    target: BlockPos,
    radius: i32,
    mask: Option<&BlockMask>,
) -> usize {
    let fire = mapping::resolve_block("fire");
    let soul_fire = mapping::resolve_block("soul_fire");
    let mut entry = EditEntry::default();
    for pos in sphere_positions(target, radius as f64, false) {
        let before = world.get_block_state_id(pos);
        if (Some(before) == fire || Some(before) == soul_fire)
            && mask.is_none_or(|mask| mask.matches(before))
        {
            push_change(world, &mut entry, pos, 0, None);
        }
    }
    commit_entry(key, world, entry)
}

fn apply_raise_lower(
    key: &str,
    world: &World,
    target: BlockPos,
    shape: Shape,
    radius: i32,
    lower: bool,
    mask: Option<&BlockMask>,
) -> usize {
    let mut entry = EditEntry::default();
    for (x, z) in shape_columns(target, shape, radius) {
        let Some((top_y, top_state)) =
            top_solid_in_column(world, x, z, target.y - radius, target.y + radius, None)
        else {
            continue;
        };
        if mask.is_some_and(|mask| !mask.matches(top_state)) {
            continue;
        }
        if lower {
            push_change(world, &mut entry, BlockPos { x, y: top_y, z }, 0, None);
        } else if top_y < MAX_BUILD_Y {
            push_change(
                world,
                &mut entry,
                BlockPos { x, y: top_y + 1, z },
                top_state,
                None,
            );
        }
    }
    commit_entry(key, world, entry)
}

fn apply_morph(
    key: &str,
    world: &World,
    target: BlockPos,
    radius: i32,
    min_erode_faces: u8,
    erode_iterations: u32,
    min_dilate_faces: u8,
    dilate_iterations: u32,
    mask: Option<&BlockMask>,
) -> usize {
    let positions = sphere_positions(target, radius as f64, false);
    let mut states = HashMap::<BlockPosKey, u16>::new();
    for pos in &positions {
        states.insert((*pos).into(), world.get_block_state_id(*pos));
    }
    for _ in 0..erode_iterations {
        let current = states.clone();
        for pos in &positions {
            let key_pos: BlockPosKey = (*pos).into();
            let state = *current.get(&key_pos).unwrap_or(&0);
            if state == 0 || mask.is_some_and(|mask| !mask.matches(state)) {
                continue;
            }
            let air_faces = neighbor_states(*pos, &current)
                .iter()
                .filter(|&&s| s == 0)
                .count() as u8;
            if air_faces >= min_erode_faces {
                states.insert(key_pos, 0);
            }
        }
    }
    for _ in 0..dilate_iterations {
        let current = states.clone();
        for pos in &positions {
            let key_pos: BlockPosKey = (*pos).into();
            if *current.get(&key_pos).unwrap_or(&0) != 0 {
                continue;
            }
            let neighbors: Vec<u16> = neighbor_states(*pos, &current)
                .into_iter()
                .filter(|state| *state != 0)
                .collect();
            if neighbors.len() as u8 >= min_dilate_faces {
                states.insert(key_pos, most_common_state(&neighbors));
            }
        }
    }

    let mut entry = EditEntry::default();
    for pos in positions {
        if let Some(after) = states.get(&pos.into()).copied() {
            push_change(world, &mut entry, pos, after, None);
        }
    }
    commit_entry(key, world, entry)
}

fn apply_snow(
    key: &str,
    world: &World,
    target: BlockPos,
    shape: Shape,
    radius: i32,
    stack: bool,
    mask: Option<&BlockMask>,
) -> usize {
    let Some(snow) = mapping::resolve_block("snow") else {
        return 0;
    };
    let mut entry = EditEntry::default();
    for (x, z) in shape_columns(target, shape, radius) {
        let Some((top_y, top_state)) =
            top_solid_in_column(world, x, z, target.y - radius, target.y + radius, None)
        else {
            continue;
        };
        if mask.is_some_and(|mask| !mask.matches(top_state)) {
            continue;
        }
        let y = if stack && top_state == snow {
            top_y
        } else {
            top_y + 1
        };
        if y <= MAX_BUILD_Y {
            push_change(world, &mut entry, BlockPos { x, y, z }, snow, None);
        }
    }
    commit_entry(key, world, entry)
}

fn push_change(
    world: &World,
    entry: &mut EditEntry,
    pos: BlockPos,
    after: u16,
    mask: Option<&BlockMask>,
) {
    let before = world.get_block_state_id(pos);
    if mask.is_some_and(|mask| !mask.matches(before)) || before == after {
        return;
    }
    entry.changes.push((pos, before, after));
}

fn commit_entry(key: &str, world: &World, entry: EditEntry) -> usize {
    let changed = entry.changes.len();
    for batch in entry.changes.chunks(batch_size()) {
        let changes: Vec<BlockChange> = batch
            .iter()
            .map(|(pos, _, after)| BlockChange {
                pos: *pos,
                state: *after,
            })
            .collect();
        world.set_block_states(&changes, block_flags());
    }
    history::push(key, entry);
    changed
}

fn sphere_positions(center: BlockPos, radius: f64, hollow: bool) -> Vec<BlockPos> {
    let r = radius.ceil() as i32;
    let radius2 = radius * radius;
    let inner2 = (radius - 1.0).max(0.0).powi(2);
    let mut positions = Vec::new();
    for dy in -r..=r {
        for dz in -r..=r {
            for dx in -r..=r {
                let d2 = (dx * dx + dy * dy + dz * dz) as f64;
                if d2 <= radius2 && (!hollow || d2 > inner2) {
                    positions.push(BlockPos {
                        x: center.x + dx,
                        y: center.y + dy,
                        z: center.z + dz,
                    });
                }
            }
        }
    }
    positions
}

fn cylinder_positions(center: BlockPos, radius: f64, height: i32, hollow: bool) -> Vec<BlockPos> {
    let r = radius.ceil() as i32;
    let radius2 = radius * radius;
    let inner2 = (radius - 1.0).max(0.0).powi(2);
    let mut positions = Vec::new();
    for y in center.y..center.y.saturating_add(height).min(MAX_BUILD_Y + 1) {
        for dz in -r..=r {
            for dx in -r..=r {
                let d2 = (dx * dx + dz * dz) as f64;
                if d2 <= radius2
                    && (!hollow || d2 > inner2 || y == center.y || y == center.y + height - 1)
                {
                    positions.push(BlockPos {
                        x: center.x + dx,
                        y,
                        z: center.z + dz,
                    });
                }
            }
        }
    }
    positions
}

fn cuboid_positions(center: BlockPos, radius: i32) -> Vec<BlockPos> {
    let mut positions = Vec::new();
    for y in center.y - radius..=center.y + radius {
        for z in center.z - radius..=center.z + radius {
            for x in center.x - radius..=center.x + radius {
                positions.push(BlockPos { x, y, z });
            }
        }
    }
    positions
}

fn splatter_positions(center: BlockPos, radius: f64, decay: u32) -> Vec<BlockPos> {
    let chance = (10 - decay.min(10) + 1) * 100;
    sphere_positions(center, radius, false)
        .into_iter()
        .filter(|pos| position_hash(*pos) % 1000 < chance)
        .collect()
}

fn shape_columns(center: BlockPos, shape: Shape, radius: i32) -> Vec<(i32, i32)> {
    let mut columns = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if matches!(shape, Shape::Sphere | Shape::Cylinder)
                && dx * dx + dz * dz > radius * radius
            {
                continue;
            }
            columns.push((center.x + dx, center.z + dz));
        }
    }
    columns
}

fn top_solid_in_column(
    world: &World,
    x: i32,
    z: i32,
    min_y: i32,
    max_y: i32,
    height_mask: Option<&BlockMask>,
) -> Option<(i32, u16)> {
    for y in (min_y.max(MIN_BUILD_Y)..=max_y.min(MAX_BUILD_Y)).rev() {
        let state = world.get_block_state_id(BlockPos { x, y, z });
        if state != 0 && height_mask.is_none_or(|mask| mask.matches(state)) {
            return Some((y, state));
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct BlockPosKey(i32, i32, i32);

impl From<BlockPos> for BlockPosKey {
    fn from(pos: BlockPos) -> Self {
        Self(pos.x, pos.y, pos.z)
    }
}

fn neighbor_states(pos: BlockPos, states: &HashMap<BlockPosKey, u16>) -> Vec<u16> {
    [
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ]
    .into_iter()
    .map(|(dx, dy, dz)| {
        states
            .get(&BlockPosKey(pos.x + dx, pos.y + dy, pos.z + dz))
            .copied()
            .unwrap_or(0)
    })
    .collect()
}

fn most_common_state(states: &[u16]) -> u16 {
    let mut counts = HashMap::<u16, usize>::new();
    for state in states {
        *counts.entry(*state).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or(0, |(state, _)| state)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_sphere_brush() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("sphere stone 3").expect("valid brush")
        else {
            panic!("expected bind");
        };
        assert!(matches!(binding.kind, BrushKind::Sphere { .. }));
    }

    #[test]
    fn parses_hollow_cylinder_brush() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("cyl -h dirt 4 2").expect("valid brush")
        else {
            panic!("expected bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Cylinder {
                hollow: true,
                height: 2,
                ..
            }
        ));
    }

    #[test]
    fn parses_settings() {
        assert!(matches!(
            parse_brush_command("size 7").unwrap(),
            ParsedBrushCommand::Setting(BrushSetting::Size(7))
        ));
        assert!(matches!(
            parse_brush_command("mask none").unwrap(),
            ParsedBrushCommand::Setting(BrushSetting::Mask(None))
        ));
    }

    #[test]
    fn sphere_hollow_has_fewer_blocks() {
        let solid = sphere_positions(BlockPos { x: 0, y: 0, z: 0 }, 3.0, false);
        let hollow = sphere_positions(BlockPos { x: 0, y: 0, z: 0 }, 3.0, true);
        assert!(hollow.len() < solid.len());
    }
}
