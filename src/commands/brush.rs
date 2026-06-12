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
    events::{BlockBreakEvent, EventHandler, EventPriority, PlayerInteractEvent},
    logging::{self, LogLevel},
    player::{Hand, Player},
    text::TextComponent,
    world::World,
};

use crate::{
    block_data::{self, BlockPlacement},
    clipboard,
    history::{self, EditEntry},
    mapping,
    pattern::{BlockMask, BlockPattern, PatternEvalContext},
    transform::Transform,
};

use super::{
    batch_size, block_flags, passes_gmask, permission_node, player_key, require_permission,
};

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
    for (name, prefix) in [
        ("none", "none"),
        ("unbind", "none"),
        ("list", "list"),
        ("info", "list"),
        ("size", "size"),
        ("material", "material"),
        ("mat", "material"),
        ("mask", "mask"),
        ("range", "range"),
        ("tracemask", "tracemask"),
        ("tm", "tracemask"),
        ("targetmask", "targetmask"),
        ("tarmask", "targetmask"),
        ("target", "target"),
        ("tar", "target"),
        ("vis", "vis"),
        ("scroll", "scroll"),
        ("sphere", "sphere"),
        ("s", "sphere"),
        ("cylinder", "cylinder"),
        ("cyl", "cylinder"),
        ("c", "cylinder"),
        ("set", "set"),
        ("clipboard", "clipboard"),
        ("copy", "clipboard"),
        ("smooth", "smooth"),
        ("gravity", "gravity"),
        ("grav", "gravity"),
        ("extinguish", "extinguish"),
        ("ex", "extinguish"),
        ("splatter", "splatter"),
        ("splat", "splatter"),
        ("raise", "raise"),
        ("lower", "lower"),
        ("erode", "erode"),
        ("dilate", "dilate"),
        ("morph", "morph"),
        ("snow", "snow"),
        ("blendball", "blendball"),
        ("bb", "blendball"),
        ("blend", "blendball"),
        ("surface", "surface"),
        ("surf", "surface"),
        ("scatter", "scatter"),
        ("spline", "spline"),
        ("spl", "spline"),
        ("curve", "spline"),
        ("surfacespline", "surfacespline"),
        ("sspline", "surfacespline"),
        ("sspl", "surfacespline"),
        ("sweep", "sweep"),
        ("sw", "sweep"),
        ("catenary", "catenary"),
        ("cat", "catenary"),
        ("gravityline", "catenary"),
        ("saggedline", "catenary"),
        ("shatter", "shatter"),
        ("partition", "shatter"),
        ("split", "shatter"),
        ("flatten", "flatten"),
        ("flat", "flatten"),
        ("flatmap", "flatten"),
        ("height", "height"),
        ("surfaceoverlay", "overlay"),
        ("overlay", "overlay"),
        ("scattercommand", "scattercommand"),
        ("scattercmd", "scattercommand"),
        ("scmd", "scattercommand"),
        ("scommand", "scattercommand"),
        ("command", "command"),
        ("cmd", "command"),
        ("populateschematic", "populateschematic"),
        ("populateschem", "populateschematic"),
        ("popschem", "populateschematic"),
        ("pschem", "populateschematic"),
        ("ps", "populateschematic"),
        ("forest", "forest"),
        ("butcher", "butcher"),
        ("kill", "kill"),
        ("paint", "paint"),
        ("snowsmooth", "snowsmooth"),
        ("heightmap", "heightmap"),
        ("feature", "feature"),
        ("apply", "apply"),
        ("deform", "deform"),
        ("biome", "biome"),
    ] {
        command.then(brush_literal(name, prefix));
    }
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
struct BrushLiteralCommand {
    prefix: &'static str,
}

impl pumpkin_plugin_api::commands::CommandHandler for BrushCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let raw = match args.get_value("args") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => {
                send_brush_usage(&sender);
                return Ok(0);
            }
        };

        handle_brush_command(sender, server, raw)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for BrushLiteralCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let tail = match args.get_value("args") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => String::new(),
        };
        let raw = if tail.trim().is_empty() {
            self.prefix.to_string()
        } else {
            format!("{} {}", self.prefix, tail)
        };
        handle_brush_command(sender, server, raw)
    }
}

fn handle_brush_command(
    sender: CommandSender,
    server: pumpkin_plugin_api::Server,
    raw: String,
) -> std::result::Result<i32, CommandError> {
    let Some(player) = sender.as_player() else {
        sender.send_error(TextComponent::text("Only players can use brush commands."));
        return Ok(0);
    };
    let Some(key) = player_key(&sender) else {
        sender.send_error(TextComponent::text("Could not determine your identity."));
        return Ok(0);
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
            let item_label = item.label();
            let mut range = DEFAULT_RANGE;
            BRUSHES.with_borrow_mut(|map| {
                let tools = map.entry(key).or_default();
                let binding = BrushBinding::with_kind(binding.kind, tools.bindings.get(&item));
                range = binding.range;
                tools.bindings.insert(item.clone(), binding);
            });
            sender.send_message(TextComponent::text(&format!(
                "Bound {summary} to {item_label}. Range: {range:.0} blocks."
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
                sender.send_message(TextComponent::text(&format!(
                    "Unbound brush from {}.",
                    item.label()
                )));
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
                        .map(|(item, binding)| {
                            format!(
                                "{}: {}{}; range {:.0}",
                                item.label(),
                                binding.kind.summary(),
                                binding
                                    .mask
                                    .as_ref()
                                    .map_or(String::new(), |_| ", masked".to_string()),
                                binding.range
                            )
                        })
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

fn brush_literal(name: &str, prefix: &'static str) -> CommandNode {
    let args = CommandNode::argument("args", &ArgumentType::String(StringType::Greedy))
        .execute(BrushLiteralCommand { prefix });
    let node = CommandNode::literal(name).execute(BrushLiteralCommand { prefix });
    node.then(args);
    node
}

#[derive(Default)]
struct PlayerBrushes {
    bindings: HashMap<ToolBindingKey, BrushBinding>,
}

thread_local! {
    static BRUSHES: RefCell<HashMap<String, PlayerBrushes>> = RefCell::new(HashMap::new());
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct ToolBindingKey {
    slot: u8,
    item: String,
}

impl ToolBindingKey {
    fn label(&self) -> String {
        format!("slot {} ({})", self.slot + 1, self.item)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum BrushTargetMode {
    #[default]
    TargetBlockRange,
    ForwardPointPitch,
    TargetPointHeight,
    TargetFaceRange,
}

impl BrushTargetMode {
    fn parse(mode: i32) -> Result<Self, String> {
        match mode {
            0 => Ok(Self::TargetBlockRange),
            1 => Ok(Self::ForwardPointPitch),
            2 => Ok(Self::TargetPointHeight),
            3 => Ok(Self::TargetFaceRange),
            _ => Err("Brush target mode must be 0, 1, 2, or 3.".to_string()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TargetBlockRange => "0 (target block range)",
            Self::ForwardPointPitch => "1 (forward point pitch)",
            Self::TargetPointHeight => "2 (target point height)",
            Self::TargetFaceRange => "3 (target face range)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BrushVisualizationMode(u8);

impl BrushVisualizationMode {
    fn parse(mode: i32) -> Result<Self, String> {
        if !(0..=2).contains(&mode) {
            return Err("Brush visualization mode must be 0, 1, or 2.".to_string());
        }
        Ok(Self(mode as u8))
    }

    fn value(self) -> u8 {
        self.0
    }
}

#[derive(Clone)]
enum BrushScrollAction {
    None,
    Size,
    Range,
    Pattern(Vec<BlockPattern>),
}

impl Default for BrushScrollAction {
    fn default() -> Self {
        Self::None
    }
}

impl BrushScrollAction {
    fn summary(&self) -> String {
        match self {
            Self::None => "disabled".to_string(),
            Self::Size => "size".to_string(),
            Self::Range => "range".to_string(),
            Self::Pattern(patterns) => format!("pattern ({})", patterns.len()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
struct BrushTransformSettings {
    transform: Transform,
    random_rotate: bool,
    auto_rotate: bool,
}

impl Default for BrushTransformSettings {
    fn default() -> Self {
        Self {
            transform: Transform::identity(),
            random_rotate: false,
            auto_rotate: false,
        }
    }
}

#[derive(Clone)]
struct TerrainBrushSettings {
    radius: f64,
    image: Option<String>,
    rotation: i32,
    y_scale: f64,
    random_rotate: bool,
    layers: bool,
    smooth: bool,
}

impl TerrainBrushSettings {
    fn summary(&self, name: &str) -> String {
        let image = self.image.as_deref().unwrap_or("default");
        format!(
            "{name} brush, radius {:.1}, y-scale {}, image {image}, rotation {}{}{}{}",
            self.radius,
            self.y_scale,
            self.rotation,
            if self.random_rotate {
                ", random rotate"
            } else {
                ""
            },
            if self.layers { ", layers" } else { "" },
            if self.smooth { "" } else { ", no smoothing" },
        )
    }

    fn validate_runtime(&self) -> Result<(), String> {
        if let Some(image) = &self.image {
            return Err(format!(
                "Terrain brush source '{image}' is parsed, but image and clipboard-backed heightmaps are not implemented yet."
            ));
        }
        if self.layers {
            return Err(
                "Terrain brush '-l' layer precision is not implemented with Pumpkin's current block-only brush path yet."
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Clone)]
struct BrushBinding {
    kind: BrushKind,
    mask: Option<BlockMask>,
    range: f64,
    trace_mask: Option<BlockMask>,
    target_mask: Option<BlockMask>,
    target_mode: BrushTargetMode,
    visualization: BrushVisualizationMode,
    scroll_action: BrushScrollAction,
    transform: BrushTransformSettings,
}

impl BrushBinding {
    fn with_kind(kind: BrushKind, existing: Option<&BrushBinding>) -> Self {
        Self {
            kind,
            mask: existing.and_then(|binding| binding.mask.clone()),
            range: existing.map_or(DEFAULT_RANGE, |binding| binding.range),
            trace_mask: existing.and_then(|binding| binding.trace_mask.clone()),
            target_mask: existing.and_then(|binding| binding.target_mask.clone()),
            target_mode: existing.map_or(BrushTargetMode::default(), |binding| binding.target_mode),
            visualization: existing.map_or(BrushVisualizationMode::default(), |binding| {
                binding.visualization
            }),
            scroll_action: existing.map_or(BrushScrollAction::default(), |binding| {
                binding.scroll_action.clone()
            }),
            transform: existing.map_or(BrushTransformSettings::default(), |binding| {
                binding.transform
            }),
        }
    }
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
        random_rotate: bool,
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
        points: i32,
        recursion: u32,
        solid: bool,
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
    Flatten {
        settings: TerrainBrushSettings,
    },
    Height {
        settings: TerrainBrushSettings,
    },
    Heightmap {
        settings: TerrainBrushSettings,
    },
    Overlay {
        pattern: BlockPattern,
        radius: f64,
    },
    Surface {
        pattern: BlockPattern,
        radius: f64,
    },
    BlendBall {
        radius: i32,
        min_frequency_diff: u8,
        only_air: bool,
    },
    Scatter {
        pattern: BlockPattern,
        radius: f64,
        points: i32,
        distance: i32,
    },
    ScatterOverlay {
        pattern: BlockPattern,
        radius: f64,
        points: i32,
        distance: i32,
    },
    ScatterCommand {
        radius: f64,
        points: i32,
        distance: i32,
        command: String,
        print: bool,
    },
    Spline {
        pattern: BlockPattern,
        radius: f64,
    },
    SurfaceSpline {
        pattern: BlockPattern,
        radius: f64,
        tension: f64,
        bias: f64,
        continuity: f64,
        quality: f64,
    },
    Sweep {
        copies: i32,
    },
    Catenary {
        pattern: BlockPattern,
        radius: f64,
        length_factor: f64,
        shell: bool,
        select: bool,
        facing_direction: bool,
    },
    Shatter {
        pattern: BlockPattern,
        radius: f64,
        count: i32,
    },
    Command {
        radius: f64,
        command: String,
        print: bool,
    },
    PopulateSchematic {
        clipboard: String,
        placement_mask: Option<BlockMask>,
        radius: f64,
        density: i32,
        rotate: bool,
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
            Self::Flatten { .. } | Self::Height { .. } | Self::Heightmap { .. } => {
                "worldedit.brush.height"
            }
            Self::Overlay { .. } | Self::Surface { .. } => "worldedit.brush.surface",
            Self::BlendBall { .. } => "worldedit.brush.blendball",
            Self::Scatter { .. } | Self::ScatterOverlay { .. } => "worldedit.brush.scatter",
            Self::ScatterCommand { .. } => "worldedit.brush.scattercommand",
            Self::Spline { .. } | Self::Catenary { .. } => "worldedit.brush.spline",
            Self::SurfaceSpline { .. } => "worldedit.brush.surfacespline",
            Self::Sweep { .. } => "worldedit.brush.sweep",
            Self::Shatter { .. } => "worldedit.brush.shatter",
            Self::Command { .. } => "worldedit.brush.command",
            Self::PopulateSchematic { .. } => "worldedit.brush.populateschematic",
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
                random_rotate,
            } => format!(
                "clipboard brush{}{}{}",
                if *skip_air { ", skipping air" } else { "" },
                if *paste_at_origin {
                    ", origin at target"
                } else {
                    ", centered"
                },
                if *random_rotate {
                    ", random rotate"
                } else {
                    ""
                },
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
                points,
                recursion,
                solid,
            } => format!(
                "splatter brush, radius {radius:.1}, points {points}, recursion {recursion}{}, pattern {}",
                if *solid { ", solid" } else { "" },
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
            Self::Flatten { settings } => settings.summary("flatten"),
            Self::Height { settings } => settings.summary("height"),
            Self::Heightmap { settings } => settings.summary("heightmap"),
            Self::Overlay { pattern, radius } => format!(
                "overlay brush, radius {radius:.1}, pattern {}",
                pattern.description()
            ),
            Self::Surface { pattern, radius } => format!(
                "surface brush, radius {radius:.1}, pattern {}",
                pattern.description()
            ),
            Self::BlendBall {
                radius,
                min_frequency_diff,
                only_air,
            } => format!(
                "blendball brush, radius {radius}, min diff {min_frequency_diff}{}",
                if *only_air { ", air only" } else { "" }
            ),
            Self::Scatter {
                pattern,
                radius,
                points,
                distance,
            } => format!(
                "scatter brush, radius {radius:.1}, points {points}, distance {distance}, pattern {}",
                pattern.description()
            ),
            Self::ScatterOverlay {
                pattern,
                radius,
                points,
                distance,
            } => format!(
                "scatter overlay brush, radius {radius:.1}, points {points}, distance {distance}, pattern {}",
                pattern.description()
            ),
            Self::ScatterCommand {
                radius,
                points,
                distance,
                command,
                print,
            } => format!(
                "scatter command brush, radius {radius:.1}, points {points}, distance {distance}, command '{command}'{}",
                if *print { ", printing output" } else { "" }
            ),
            Self::Spline { pattern, radius } => format!(
                "spline brush, radius {radius:.1}, pattern {}",
                pattern.description()
            ),
            Self::SurfaceSpline {
                pattern,
                radius,
                tension,
                bias,
                continuity,
                quality,
            } => format!(
                "surface spline brush, radius {radius:.1}, tension {tension}, bias {bias}, continuity {continuity}, quality {quality}, pattern {}",
                pattern.description()
            ),
            Self::Sweep { copies } => format!("sweep brush, copies {copies}"),
            Self::Catenary {
                pattern,
                radius,
                length_factor,
                shell,
                select,
                facing_direction,
            } => format!(
                "catenary brush, radius {radius:.1}, length factor {length_factor}{}{}{}, pattern {}",
                if *shell { ", shell" } else { "" },
                if *select { ", select" } else { "" },
                if *facing_direction {
                    ", facing direction"
                } else {
                    ""
                },
                pattern.description()
            ),
            Self::Shatter {
                pattern,
                radius,
                count,
            } => format!(
                "shatter brush, radius {radius:.1}, lines {count}, pattern {}",
                pattern.description()
            ),
            Self::Command {
                radius,
                command,
                print,
            } => format!(
                "command brush, radius {radius:.1}, command '{command}'{}",
                if *print { ", printing output" } else { "" }
            ),
            Self::PopulateSchematic {
                clipboard,
                placement_mask,
                radius,
                density,
                rotate,
            } => format!(
                "populate schematic brush, radius {radius:.1}, density {density}, clipboard {clipboard}{}{}",
                if placement_mask.is_some() {
                    ", masked placement"
                } else {
                    ""
                },
                if *rotate { ", random rotate" } else { "" }
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
            | Self::Snow { radius: r, .. }
            | Self::BlendBall { radius: r, .. } => *r = radius,
            Self::Flatten { settings }
            | Self::Height { settings }
            | Self::Heightmap { settings } => settings.radius = radius as f64,
            Self::Overlay { radius: r, .. }
            | Self::Surface { radius: r, .. }
            | Self::Scatter { radius: r, .. }
            | Self::ScatterOverlay { radius: r, .. }
            | Self::ScatterCommand { radius: r, .. }
            | Self::Spline { radius: r, .. }
            | Self::SurfaceSpline { radius: r, .. }
            | Self::Catenary { radius: r, .. }
            | Self::Shatter { radius: r, .. }
            | Self::Command { radius: r, .. }
            | Self::PopulateSchematic { radius: r, .. } => *r = radius as f64,
            Self::Clipboard { .. } => {
                return Err("Clipboard brushes do not have a radius.".to_string());
            }
            Self::Sweep { .. } => {
                return Err("Sweep brushes do not have a radius.".to_string());
            }
        }
        Ok(())
    }

    fn set_material(&mut self, pattern: BlockPattern) -> Result<(), String> {
        match self {
            Self::Sphere { pattern: p, .. }
            | Self::Cylinder { pattern: p, .. }
            | Self::Cuboid { pattern: p, .. }
            | Self::Splatter { pattern: p, .. }
            | Self::Overlay { pattern: p, .. }
            | Self::Surface { pattern: p, .. }
            | Self::Scatter { pattern: p, .. }
            | Self::ScatterOverlay { pattern: p, .. }
            | Self::Spline { pattern: p, .. }
            | Self::SurfaceSpline { pattern: p, .. }
            | Self::Catenary { pattern: p, .. }
            | Self::Shatter { pattern: p, .. } => {
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
    TraceMask(Option<BlockMask>),
    TargetMask(Option<BlockMask>),
    TargetMode(BrushTargetMode),
    Visualization(BrushVisualizationMode),
    Scroll(BrushScrollAction),
}

impl BrushSetting {
    fn permission(&self) -> &'static str {
        match self {
            Self::Size(_) => "worldedit.brush.options.size",
            Self::Material(_) => "worldedit.brush.options.material",
            Self::Mask(_) => "worldedit.brush.options.mask",
            Self::Range(_) => "worldedit.brush.options.range",
            Self::TraceMask(_) => "worldedit.brush.options.tracemask",
            Self::TargetMask(_) => "worldedit.brush.options.targetmask",
            Self::TargetMode(_) => "worldedit.brush.target",
            Self::Visualization(_) => "worldedit.brush.options.vis",
            Self::Scroll(_) => "worldedit.brush.scroll",
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
            Self::TraceMask(mask) => {
                binding.trace_mask = mask;
                Ok("Brush trace mask updated.".to_string())
            }
            Self::TargetMask(mask) => {
                binding.target_mask = mask;
                Ok("Brush target mask updated.".to_string())
            }
            Self::TargetMode(mode) => {
                binding.target_mode = mode;
                Ok(format!("Brush target mode set to {}.", mode.label()))
            }
            Self::Visualization(mode) => {
                binding.visualization = mode;
                Ok(format!(
                    "Brush visualization mode stored as {}.",
                    mode.value()
                ))
            }
            Self::Scroll(action) => {
                let summary = action.summary();
                binding.scroll_action = action;
                Ok(format!("Brush scroll action set to {summary}."))
            }
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
        "tracemask" | "tm" => parse_mask_setting(args, BrushSettingKind::TraceMask),
        "targetmask" | "tarmask" => parse_mask_setting(args, BrushSettingKind::TargetMask),
        "target" | "tar" => parse_target(args),
        "vis" => parse_vis(args),
        "scroll" => parse_scroll(args),
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
        "blendball" | "bb" | "blend" => parse_blendball(args),
        "surface" | "surf" => parse_surface(args),
        "overlay" | "surfaceoverlay" => parse_overlay(args),
        "scatter" => parse_scatter(args),
        "scattercommand" | "scattercmd" | "scmd" | "scommand" => parse_scatter_command(args),
        "spline" | "spl" | "curve" => parse_spline(args),
        "surfacespline" | "sspline" | "sspl" => parse_surface_spline(args),
        "sweep" | "sw" => parse_sweep(args),
        "catenary" | "cat" | "gravityline" | "saggedline" => parse_catenary(args),
        "shatter" | "partition" | "split" => parse_shatter(args),
        "command" | "cmd" => parse_command_brush(args),
        "populateschematic" | "populateschem" | "popschem" | "pschem" | "ps" => {
            parse_populate_schematic(args)
        }
        "flatten" | "flat" | "flatmap" => parse_terrain_kind(args, TerrainKind::Flatten),
        "height" => parse_terrain_kind(args, TerrainKind::Height),
        "heightmap" => parse_terrain_kind(args, TerrainKind::Heightmap),
        "forest" | "butcher" | "kill" | "paint" | "snowsmooth" | "feature" | "apply" | "deform"
        | "biome" => Ok(ParsedBrushCommand::Unsupported {
            name,
            reason: "it needs entities, biomes, generation features, images, or FAWE expressions that this plugin cannot access yet.",
        }),
        _ => Err(format!("Unknown brush '{name}'.")),
    }
}

enum BrushSettingKind {
    TraceMask,
    TargetMask,
}

#[derive(Clone, Copy)]
enum TerrainKind {
    Flatten,
    Height,
    Heightmap,
}

fn parse_mask_setting(
    args: &[String],
    kind: BrushSettingKind,
) -> Result<ParsedBrushCommand, String> {
    let mask = if args.first().is_none_or(|s| s.eq_ignore_ascii_case("none")) {
        None
    } else {
        Some(BlockMask::parse(&args.join(","))?)
    };
    Ok(ParsedBrushCommand::Setting(match kind {
        BrushSettingKind::TraceMask => BrushSetting::TraceMask(mask),
        BrushSettingKind::TargetMask => BrushSetting::TargetMask(mask),
    }))
}

fn parse_target(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mode = parse_optional_i32(args.first(), 0)?;
    if let Some(unexpected) = args.get(1) {
        return Err(format!("Unexpected target mode argument '{unexpected}'."));
    }
    Ok(ParsedBrushCommand::Setting(BrushSetting::TargetMode(
        BrushTargetMode::parse(mode)?,
    )))
}

fn parse_vis(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mode = parse_optional_i32(args.first(), 0)?;
    if let Some(unexpected) = args.get(1) {
        return Err(format!(
            "Unexpected brush visualization argument '{unexpected}'."
        ));
    }
    Ok(ParsedBrushCommand::Setting(BrushSetting::Visualization(
        BrushVisualizationMode::parse(mode)?,
    )))
}

fn parse_scroll(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let Some(mode) = args.first() else {
        return Err("Usage: //brush scroll <none|size|range|pattern> ...".to_string());
    };
    let action = match mode.to_ascii_lowercase().as_str() {
        "none" => {
            if let Some(unexpected) = args.get(1) {
                return Err(format!("Unexpected scroll argument '{unexpected}'."));
            }
            BrushScrollAction::None
        }
        "size" => {
            if let Some(unexpected) = args.get(1) {
                return Err(format!("Unexpected scroll argument '{unexpected}'."));
            }
            BrushScrollAction::Size
        }
        "range" => {
            if let Some(unexpected) = args.get(1) {
                return Err(format!("Unexpected scroll argument '{unexpected}'."));
            }
            BrushScrollAction::Range
        }
        "pattern" | "material" | "mat" => {
            if args.len() < 2 {
                return Err("Usage: //brush scroll pattern <pattern> [pattern ...]".to_string());
            }
            let mut patterns = Vec::with_capacity(args.len() - 1);
            for raw in &args[1..] {
                patterns.push(BlockPattern::parse(raw)?);
            }
            BrushScrollAction::Pattern(patterns)
        }
        "mask" | "clipboard" | "target" | "targetoffset" => {
            return Err(format!(
                "Scroll mode '{mode}' is recognized, but only size, range, and pattern switching are stored in Phase 1."
            ));
        }
        _ => return Err(format!("Unknown scroll mode '{mode}'.")),
    };
    Ok(ParsedBrushCommand::Setting(BrushSetting::Scroll(action)))
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
    let mut random_rotate = false;
    for arg in args {
        let Some(flags) = arg.strip_prefix('-') else {
            return Err(format!("Unexpected clipboard brush argument '{arg}'."));
        };
        for flag in flags.chars() {
            match flag {
                'a' => skip_air = true,
                'o' => paste_at_origin = true,
                'r' => random_rotate = true,
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
        random_rotate,
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
    let points = parse_optional_i32(args.get(2), 1)?.max(1);
    let recursion = parse_optional_i32(args.get(3), 5)?.clamp(0, 20) as u32;
    let solid = args
        .get(4)
        .map_or(Ok(true), |raw| parse_bool_str(raw, "solid"))?;
    Ok(bind(BrushKind::Splatter {
        pattern,
        radius,
        points,
        recursion,
        solid,
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

fn parse_blendball(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut only_air = false;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-a" => only_air = true,
            "-m" => {
                return Err(
                    "Blendball '-m' masks need FAWE's sampling masks, which are not stored here yet."
                        .to_string(),
                );
            }
            other if other.starts_with('-') => {
                return Err(format!("Unknown blendball flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let radius = parse_optional_i32_str(positional.first().copied(), DEFAULT_RADIUS as i32)?;
    let min_frequency_diff = parse_optional_i32_str(positional.get(1).copied(), 1)?.clamp(0, 26);
    Ok(bind(BrushKind::BlendBall {
        radius: clamp_radius(radius as f64)? as i32,
        min_frequency_diff: min_frequency_diff as u8,
        only_air,
    }))
}

fn parse_surface(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = parse_optional_radius(args.get(1), DEFAULT_RADIUS)?;
    Ok(bind(BrushKind::Surface { pattern, radius }))
}

fn parse_overlay(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = parse_optional_radius(args.get(1), DEFAULT_RADIUS)?;
    Ok(bind(BrushKind::Overlay { pattern, radius }))
}

fn parse_scatter(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut overlay = false;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-o" => overlay = true,
            other if other.starts_with('-') => {
                return Err(format!("Unknown scatter flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let pattern = BlockPattern::parse(
        positional
            .first()
            .copied()
            .ok_or_else(|| "Expected a block pattern.".to_string())?,
    )?;
    let radius = parse_optional_radius_str(positional.get(1).copied(), DEFAULT_RADIUS)?;
    let points = parse_optional_i32_str(positional.get(2).copied(), 5)?.max(1);
    let distance = parse_optional_i32_str(positional.get(3).copied(), 1)?.max(1);
    let kind = if overlay {
        BrushKind::ScatterOverlay {
            pattern,
            radius,
            points,
            distance,
        }
    } else {
        BrushKind::Scatter {
            pattern,
            radius,
            points,
            distance,
        }
    };
    Ok(bind(kind))
}

fn parse_scatter_command(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut print = false;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-p" => print = true,
            other if other.starts_with('-') => {
                return Err(format!("Unknown scattercommand flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    if positional.len() < 4 {
        return Err(
            "Usage: //brush scattercommand <radius> [points] [distance] <command ...>".to_string(),
        );
    }
    let radius = clamp_zeroable_radius(parse_f64_str(positional[0], "radius")?)?;
    let points = parse_optional_i32_str(positional.get(1).copied(), 1)?.max(1);
    let distance = parse_optional_i32_str(positional.get(2).copied(), 1)?.max(1);
    let command = positional[3..].join(" ");
    Ok(bind(BrushKind::ScatterCommand {
        radius,
        points,
        distance,
        command,
        print,
    }))
}

fn parse_spline(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = parse_optional_radius(args.get(1), 25.0)?;
    Ok(bind(BrushKind::Spline { pattern, radius }))
}

fn parse_surface_spline(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = clamp_zeroable_radius(parse_optional_f64_str(
        args.get(1).map(String::as_str),
        0.0,
    )?)?;
    let tension = parse_optional_f64_str(args.get(2).map(String::as_str), 0.0)?;
    let bias = parse_optional_f64_str(args.get(3).map(String::as_str), 0.0)?;
    let continuity = parse_optional_f64_str(args.get(4).map(String::as_str), 0.0)?;
    let quality = parse_optional_f64_str(args.get(5).map(String::as_str), 10.0)?;
    Ok(bind(BrushKind::SurfaceSpline {
        pattern,
        radius,
        tension,
        bias,
        continuity,
        quality,
    }))
}

fn parse_sweep(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let copies = parse_optional_i32(args.first(), -1)?;
    if let Some(unexpected) = args.get(1) {
        return Err(format!("Unexpected sweep argument '{unexpected}'."));
    }
    Ok(bind(BrushKind::Sweep { copies }))
}

fn parse_catenary(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut shell = false;
    let mut select = false;
    let mut facing_direction = false;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-h" => shell = true,
            "-s" => select = true,
            "-d" => facing_direction = true,
            other if other.starts_with('-') => {
                return Err(format!("Unknown catenary flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let pattern = BlockPattern::parse(
        positional
            .first()
            .copied()
            .ok_or_else(|| "Expected a block pattern.".to_string())?,
    )?;
    let length_factor = parse_optional_f64_str(positional.get(1).copied(), 1.2)?;
    let radius = clamp_zeroable_radius(parse_optional_f64_str(positional.get(2).copied(), 0.0)?)?;
    Ok(bind(BrushKind::Catenary {
        pattern,
        radius,
        length_factor,
        shell,
        select,
        facing_direction,
    }))
}

fn parse_shatter(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let pattern = parse_required_pattern(args.first())?;
    let radius = parse_optional_radius(args.get(1), 10.0)?;
    let count = parse_optional_i32(args.get(2), 10)?.max(1);
    Ok(bind(BrushKind::Shatter {
        pattern,
        radius,
        count,
    }))
}

fn parse_command_brush(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut print = true;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-h" => print = false,
            other if other.starts_with('-') => {
                return Err(format!("Unknown command brush flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    if positional.len() < 2 {
        return Err("Usage: //brush command <radius> <command ...>".to_string());
    }
    let radius = clamp_zeroable_radius(parse_f64_str(positional[0], "radius")?)?;
    Ok(bind(BrushKind::Command {
        radius,
        command: positional[1..].join(" "),
        print,
    }))
}

fn parse_populate_schematic(args: &[String]) -> Result<ParsedBrushCommand, String> {
    let mut rotate = false;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-r" => rotate = true,
            other if other.starts_with('-') => {
                return Err(format!("Unknown populate schematic flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let clipboard = positional
        .first()
        .copied()
        .ok_or_else(|| "Expected a clipboard or schematic source.".to_string())?
        .to_string();
    let mut index = 1usize;
    let placement_mask = if let Some(raw) = positional.get(index) {
        if raw.parse::<f64>().is_ok() {
            None
        } else {
            index += 1;
            Some(BlockMask::parse(raw)?)
        }
    } else {
        None
    };
    let radius = clamp_zeroable_radius(parse_optional_f64_str(
        positional.get(index).copied(),
        30.0,
    )?)?;
    let density = parse_optional_i32_str(positional.get(index + 1).copied(), 50)?.max(1);
    Ok(bind(BrushKind::PopulateSchematic {
        clipboard,
        placement_mask,
        radius,
        density,
        rotate,
    }))
}

fn parse_terrain_kind(args: &[String], kind: TerrainKind) -> Result<ParsedBrushCommand, String> {
    let settings = parse_terrain_settings(args, kind)?;
    Ok(bind(match kind {
        TerrainKind::Flatten => BrushKind::Flatten { settings },
        TerrainKind::Height => BrushKind::Height { settings },
        TerrainKind::Heightmap => BrushKind::Heightmap { settings },
    }))
}

fn parse_terrain_settings(
    args: &[String],
    kind: TerrainKind,
) -> Result<TerrainBrushSettings, String> {
    let mut random_rotate = false;
    let mut layers = false;
    let mut smooth = true;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-r" => random_rotate = true,
            "-l" => layers = true,
            "-s" => smooth = false,
            other if other.starts_with('-') => {
                return Err(format!("Unknown terrain brush flag '{other}'."));
            }
            _ => positional.push(arg.as_str()),
        }
    }
    let radius = clamp_zeroable_radius(parse_optional_f64_str(
        positional.first().copied(),
        DEFAULT_RADIUS,
    )?)?;
    let (y_scale, image, rotation, consumed_after_radius) =
        parse_terrain_positionals(&positional[1..], kind)?;
    if let Some(unexpected) = positional.get(consumed_after_radius + 1) {
        return Err(format!("Unexpected terrain brush argument '{unexpected}'."));
    }
    Ok(TerrainBrushSettings {
        radius,
        image,
        rotation,
        y_scale,
        random_rotate,
        layers,
        smooth,
    })
}

fn parse_terrain_positionals(
    args: &[&str],
    kind: TerrainKind,
) -> Result<(f64, Option<String>, i32, usize), String> {
    match kind {
        TerrainKind::Height | TerrainKind::Heightmap => parse_height_positionals(args),
        TerrainKind::Flatten => parse_flatten_positionals(args),
    }
}

fn parse_height_positionals(args: &[&str]) -> Result<(f64, Option<String>, i32, usize), String> {
    let mut index = 0usize;
    let y_scale = if let Some(raw) = args.get(index) {
        if raw.parse::<f64>().is_ok() {
            index += 1;
            parse_f64_str(raw, "yscale")?
        } else {
            1.0
        }
    } else {
        1.0
    };
    let image = args
        .get(index)
        .filter(|raw| raw.parse::<i32>().is_err())
        .map(|raw| {
            index += 1;
            (*raw).to_string()
        })
        .filter(|raw| !raw.is_empty() && !raw.eq_ignore_ascii_case("none"));
    let rotation = if let Some(raw) = args.get(index) {
        if raw.parse::<i32>().is_ok() {
            index += 1;
            parse_i32_str(raw, "rotation")?
        } else {
            0
        }
    } else {
        0
    };
    Ok((y_scale, image, rotation, index))
}

fn parse_flatten_positionals(args: &[&str]) -> Result<(f64, Option<String>, i32, usize), String> {
    if let (Some(first), Some(second)) = (args.first(), args.get(1)) {
        if first.parse::<f64>().is_ok() && second.parse::<f64>().is_err() {
            let y_scale = parse_f64_str(first, "yscale")?;
            let image = Some((*second).to_string()).filter(|raw| !raw.eq_ignore_ascii_case("none"));
            let rotation = if let Some(raw) = args.get(2) {
                parse_i32_str(raw, "rotation")?
            } else {
                0
            };
            return Ok((y_scale, image, rotation, (3).min(args.len())));
        }
    }

    let mut index = 0usize;
    let image = args
        .get(index)
        .filter(|raw| raw.parse::<f64>().is_err() && raw.parse::<i32>().is_err())
        .map(|raw| {
            index += 1;
            (*raw).to_string()
        })
        .filter(|raw| !raw.is_empty() && !raw.eq_ignore_ascii_case("none"));
    let rotation = if let Some(raw) = args.get(index) {
        if raw.parse::<i32>().is_ok() {
            index += 1;
            parse_i32_str(raw, "rotation")?
        } else {
            0
        }
    } else {
        0
    };
    let y_scale = if let Some(raw) = args.get(index) {
        index += 1;
        parse_f64_str(raw, "yscale")?
    } else {
        1.0
    };
    Ok((y_scale, image, rotation, index))
}

fn bind(kind: BrushKind) -> ParsedBrushCommand {
    ParsedBrushCommand::Bind(BrushBinding::with_kind(kind, None))
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

fn parse_optional_i32_str(raw: Option<&str>, default: i32) -> Result<i32, String> {
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

fn parse_i32_str(raw: &str, name: &str) -> Result<i32, String> {
    raw.parse::<i32>()
        .map_err(|_| format!("Invalid {name} '{raw}'."))
}

fn parse_bool_str(raw: &str, name: &str) -> Result<bool, String> {
    match raw.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" | "on" => Ok(true),
        "false" | "f" | "no" | "n" | "0" | "off" => Ok(false),
        _ => Err(format!("Invalid {name} '{raw}'.")),
    }
}

fn parse_f64(raw: Option<&String>, name: &str) -> Result<f64, String> {
    raw.ok_or_else(|| format!("Expected {name}."))
        .and_then(|raw| {
            raw.parse::<f64>()
                .map_err(|_| format!("Invalid {name} '{raw}'."))
        })
}

fn parse_optional_f64_str(raw: Option<&str>, default: f64) -> Result<f64, String> {
    match raw {
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid number '{raw}'.")),
        None => Ok(default),
    }
}

fn parse_f64_str(raw: &str, name: &str) -> Result<f64, String> {
    raw.parse::<f64>()
        .map_err(|_| format!("Invalid {name} '{raw}'."))
}

fn parse_optional_radius_str(raw: Option<&str>, default: f64) -> Result<f64, String> {
    clamp_radius(match raw {
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid radius '{raw}'."))?,
        None => default,
    })
}

fn clamp_radius(radius: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err("Brush radius must be positive.".to_string());
    }
    Ok(radius.min(MAX_RADIUS))
}

fn clamp_zeroable_radius(radius: f64) -> Result<f64, String> {
    if !radius.is_finite() || radius < 0.0 {
        return Err("Brush radius must be zero or positive.".to_string());
    }
    Ok(radius.min(MAX_RADIUS))
}

fn tokenize(raw: &str) -> Vec<String> {
    raw.split_whitespace().map(str::to_string).collect()
}

fn held_item_key(player: &Player) -> Option<ToolBindingKey> {
    player
        .get_item_in_hand(Hand::Right)
        .map(|stack| ToolBindingKey {
            slot: player.get_selected_slot(),
            item: stack.get_registry_key(),
        })
}

fn send_brush_usage(sender: &CommandSender) {
    sender.send_error(TextComponent::text(
        "Usage: //brush <sphere|cylinder|set|clipboard|smooth|gravity|extinguish|splatter|raise|lower|erode|dilate|morph|snow|blendball|surface|overlay|scatter|scattercommand|height|flatten|spline|surfacespline|sweep|catenary|shatter|command|populateschematic|none|list|size|material|mask|range|tracemask|targetmask|target|vis|scroll> ...",
    ));
}

struct BrushInteractHandler;

impl EventHandler<PlayerInteractEvent> for BrushInteractHandler {
    fn handle(
        &self,
        _server: pumpkin_plugin_api::Server,
        mut data: pumpkin_plugin_api::events::EventData<PlayerInteractEvent>,
    ) -> pumpkin_plugin_api::events::EventData<PlayerInteractEvent> {
        if trigger_player_brush(&data.player, data.clicked_pos) {
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
        if trigger_player_brush(player, Some(data.block_pos)) {
            data.cancelled = true;
        }
        data
    }
}

fn trigger_player_brush(player: &Player, clicked: Option<BlockPos>) -> bool {
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

    let target = match player
        .as_entity()
        .raycast(binding.range, false)
        .map(|hit| hit.pos)
        .or(clicked)
    {
        Some(target) => target,
        None => {
            player.send_system_message(
                TextComponent::text(&format!(
                    "No target block in range ({:.0} blocks).",
                    binding.range
                )),
                true,
            );
            return true;
        }
    };
    let world = player.get_world();
    let started = std::time::Instant::now();
    let changed = match apply_brush(&key, &world, target, &binding) {
        Ok(changed) => changed,
        Err(message) => {
            player.send_system_message(TextComponent::text(&message), true);
            return true;
        }
    };
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

fn apply_brush(
    key: &str,
    world: &World,
    target: BlockPos,
    binding: &BrushBinding,
) -> Result<usize, String> {
    let pattern_ctx = PatternEvalContext::for_operation(target, key, world);
    match &binding.kind {
        BrushKind::Sphere {
            pattern,
            radius,
            hollow,
        } => {
            pattern.validate(&pattern_ctx)?;
            Ok(apply_pattern_positions(
                key,
                world,
                sphere_positions(target, *radius, *hollow),
                pattern,
                &pattern_ctx,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Cylinder {
            pattern,
            radius,
            height,
            hollow,
        } => {
            pattern.validate(&pattern_ctx)?;
            Ok(apply_pattern_positions(
                key,
                world,
                cylinder_positions(target, *radius, *height, *hollow),
                pattern,
                &pattern_ctx,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Cuboid { pattern, radius } => {
            pattern.validate(&pattern_ctx)?;
            Ok(apply_pattern_positions(
                key,
                world,
                cuboid_positions(target, *radius),
                pattern,
                &pattern_ctx,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Clipboard {
            skip_air,
            paste_at_origin,
            random_rotate,
        } => Ok(apply_clipboard(
            key,
            world,
            target,
            *skip_air,
            *paste_at_origin,
            *random_rotate,
            binding.mask.as_ref(),
        )),
        BrushKind::Smooth {
            radius,
            iterations,
            height_mask,
        } => Ok(apply_smooth(
            key,
            world,
            target,
            *radius,
            *iterations,
            height_mask.as_ref(),
            binding.mask.as_ref(),
        )),
        BrushKind::Gravity { radius, height } => Ok(apply_gravity(
            key,
            world,
            target,
            *radius,
            *height,
            binding.mask.as_ref(),
        )),
        BrushKind::Extinguish { radius } => Ok(apply_extinguish(
            key,
            world,
            target,
            *radius,
            binding.mask.as_ref(),
        )),
        BrushKind::Splatter {
            pattern,
            radius,
            points,
            recursion,
            solid,
        } => {
            pattern.validate(&pattern_ctx)?;
            Ok(apply_splatter(
                key,
                world,
                target,
                pattern,
                &pattern_ctx,
                *radius,
                *points as usize,
                *recursion,
                *solid,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Raise {
            shape,
            radius,
            lower,
        } => Ok(apply_raise_lower(
            key,
            world,
            target,
            *shape,
            *radius,
            *lower,
            binding.mask.as_ref(),
        )),
        BrushKind::Morph {
            radius,
            min_erode_faces,
            erode_iterations,
            min_dilate_faces,
            dilate_iterations,
        } => Ok(apply_morph(
            key,
            world,
            target,
            *radius,
            *min_erode_faces,
            *erode_iterations,
            *min_dilate_faces,
            *dilate_iterations,
            binding.mask.as_ref(),
        )),
        BrushKind::Snow {
            shape,
            radius,
            stack,
        } => Ok(apply_snow(
            key,
            world,
            target,
            *shape,
            *radius,
            *stack,
            binding.mask.as_ref(),
        )),
        BrushKind::Flatten { settings } => {
            settings.validate_runtime()?;
            Ok(apply_terrain_brush(
                key,
                world,
                target,
                settings,
                TerrainMode::Flatten,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Height { settings } | BrushKind::Heightmap { settings } => {
            settings.validate_runtime()?;
            Ok(apply_terrain_brush(
                key,
                world,
                target,
                settings,
                TerrainMode::RaiseLower,
                binding.mask.as_ref(),
            ))
        }
        BrushKind::Overlay { .. } => Err(
            "Overlay brushes are parsed and stored, but the surface-following edit path is not implemented yet."
                .to_string(),
        ),
        BrushKind::Surface { .. } => Err(
            "Surface brushes are parsed and stored, but the surface-following edit path is not implemented yet."
                .to_string(),
        ),
        BrushKind::BlendBall { .. } => Err(
            "Blendball brushes are parsed and stored, but the terrain blending edit path is not implemented yet."
                .to_string(),
        ),
        BrushKind::Scatter { .. } | BrushKind::ScatterOverlay { .. } => Err(
            "Scatter brushes are parsed and stored, but the scatter placement edit path is not implemented yet."
                .to_string(),
        ),
        BrushKind::ScatterCommand { .. } => Err(
            "Scatter command brushes are parsed and stored, but command execution brushes are not enabled yet."
                .to_string(),
        ),
        BrushKind::Spline { .. }
        | BrushKind::SurfaceSpline { .. }
        | BrushKind::Sweep { .. }
        | BrushKind::Catenary { .. } => Err(
            "Curve brushes are parsed and stored, but multi-click control point state is not implemented yet."
                .to_string(),
        ),
        BrushKind::Shatter { .. } => Err(
            "Shatter brushes are parsed and stored, but the fracture terrain edit path is not implemented yet."
                .to_string(),
        ),
        BrushKind::Command { .. } => Err(
            "Command brushes are parsed and stored, but command execution brushes are not enabled yet."
                .to_string(),
        ),
        BrushKind::PopulateSchematic { .. } => Err(
            "Populate schematic brushes are parsed and stored, but scatter schematic placement is not implemented yet."
                .to_string(),
        ),
    }
}

fn apply_pattern_positions(
    key: &str,
    world: &World,
    positions: Vec<BlockPos>,
    pattern: &BlockPattern,
    pattern_ctx: &PatternEvalContext,
    mask: Option<&BlockMask>,
) -> usize {
    let mut entry = EditEntry::default();
    let mut changed = 0usize;
    for batch in positions.chunks(batch_size()) {
        let mut changes = Vec::with_capacity(batch.len());
        for &pos in batch {
            let before = block_data::capture_block(world, pos);
            if mask.is_some_and(|mask| !mask.matches(before.state_id))
                || !passes_gmask(key, before.state_id)
            {
                continue;
            }
            let after = pattern.placement_at_with(pos, &before, pattern_ctx);
            if before == after {
                continue;
            }
            entry.push_change(pos, before, after.clone());
            changes.push((pos, after));
        }
        changed += changes.len();
        if !changes.is_empty() {
            block_data::apply_blocks(world, &changes, block_flags());
        }
    }
    debug_assert_eq!(changed, entry.changes.len());
    finalize_brush_history(key, entry)
}

fn apply_splatter(
    key: &str,
    world: &World,
    target: BlockPos,
    pattern: &BlockPattern,
    pattern_ctx: &PatternEvalContext,
    radius: f64,
    points: usize,
    recursion: u32,
    solid: bool,
    mask: Option<&BlockMask>,
) -> usize {
    let surface_hits = surface_hits_for_shape(
        world,
        target,
        Shape::Sphere,
        radius.ceil() as i32,
        MIN_BUILD_Y,
        MAX_BUILD_Y,
        None,
    );
    let growth = generate_splatter_hits(&surface_hits, points, recursion);
    let mut entry = EditEntry::default();
    let mut solid_cache = HashMap::<BlockPosKey, BlockPlacement>::new();

    for placement in growth {
        let pos = BlockPos {
            x: placement.hit.column.x,
            y: placement.hit.y,
            z: placement.hit.column.z,
        };
        let before = block_data::capture_block(world, pos);
        if mask.is_some_and(|mask| !mask.matches(before.state_id))
            || !passes_gmask(key, before.state_id)
        {
            continue;
        }
        let after = if solid {
            let seed_pos = BlockPos {
                x: placement.seed.column.x,
                y: placement.seed.y,
                z: placement.seed.column.z,
            };
            solid_cache
                .entry(seed_pos.into())
                .or_insert_with(|| {
                    let seed_before = block_data::capture_block(world, seed_pos);
                    pattern.placement_at_with(seed_pos, &seed_before, pattern_ctx)
                })
                .clone()
        } else {
            pattern.placement_at_with(pos, &before, pattern_ctx)
        };
        if before == after {
            continue;
        }
        entry.push_change(pos, before, after);
    }

    commit_entry(key, world, entry)
}

fn apply_clipboard(
    key: &str,
    world: &World,
    target: BlockPos,
    skip_air: bool,
    paste_at_origin: bool,
    random_rotate: bool,
    mask: Option<&BlockMask>,
) -> usize {
    let Some((buffer, clipboard_transform)) = clipboard::get_with_transform(key) else {
        return 0;
    };
    let transform = if random_rotate {
        clipboard_transform.combine(deterministic_clipboard_rotation(target))
    } else {
        clipboard_transform
    };
    let buffer = buffer.transformed(transform);
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
            if mask.is_some_and(|mask| !mask.matches(before))
                || !passes_gmask(key, before)
                || before == state
            {
                continue;
            }
            entry.push_state_change(pos, before, state);
            changes.push((pos, BlockPlacement::new(state)));
        }
        changed += changes.len();
        if !changes.is_empty() {
            block_data::apply_blocks(world, &changes, block_flags());
        }
    }
    debug_assert_eq!(changed, entry.changes.len());
    finalize_brush_history(key, entry)
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
    let original_hits = surface_hits_for_shape(
        world,
        target,
        Shape::Sphere,
        radius,
        target.y - radius,
        target.y + radius,
        height_mask,
    );
    let mut surface_hits = HashMap::<(i32, i32), SurfaceHit>::new();
    for hit in original_hits {
        heights.insert((hit.column.dx, hit.column.dz), hit.y);
        surface_hits.insert((hit.column.dx, hit.column.dz), hit);
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
        let Some(hit) = surface_hits.get(&(dx, dz)).copied() else {
            continue;
        };
        let x = hit.column.x;
        let z = hit.column.z;
        let old_y = hit.y;
        let top_state = hit.state;
        if new_y > old_y {
            for y in old_y + 1..=new_y.min(MAX_BUILD_Y) {
                push_change(
                    key,
                    world,
                    &mut entry,
                    BlockPos { x, y, z },
                    top_state,
                    mask,
                );
            }
        } else if new_y < old_y {
            for y in (new_y.max(MIN_BUILD_Y) + 1)..=old_y {
                push_change(key, world, &mut entry, BlockPos { x, y, z }, 0, mask);
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
                push_change(key, world, &mut entry, BlockPos { x, y, z }, after, None);
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
            push_change(key, world, &mut entry, pos, 0, None);
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
    for hit in surface_hits_for_shape(
        world,
        target,
        shape,
        radius,
        target.y - radius,
        target.y + radius,
        None,
    ) {
        let x = hit.column.x;
        let z = hit.column.z;
        let top_y = hit.y;
        let top_state = hit.state;
        if mask.is_some_and(|mask| !mask.matches(top_state)) {
            continue;
        }
        if lower {
            push_change(key, world, &mut entry, BlockPos { x, y: top_y, z }, 0, None);
        } else if top_y < MAX_BUILD_Y {
            push_change(
                key,
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
            push_change(key, world, &mut entry, pos, after, None);
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
    for hit in surface_hits_for_shape(
        world,
        target,
        shape,
        radius,
        target.y - radius,
        target.y + radius,
        None,
    ) {
        let x = hit.column.x;
        let z = hit.column.z;
        let top_y = hit.y;
        let top_state = hit.state;
        if mask.is_some_and(|mask| !mask.matches(top_state)) {
            continue;
        }
        let y = if stack && top_state == snow {
            top_y
        } else {
            top_y + 1
        };
        if y <= MAX_BUILD_Y {
            push_change(key, world, &mut entry, BlockPos { x, y, z }, snow, None);
        }
    }
    commit_entry(key, world, entry)
}

#[derive(Clone, Copy)]
enum TerrainMode {
    RaiseLower,
    Flatten,
}

fn apply_terrain_brush(
    key: &str,
    world: &World,
    target: BlockPos,
    settings: &TerrainBrushSettings,
    mode: TerrainMode,
    mask: Option<&BlockMask>,
) -> usize {
    let radius = settings.radius.ceil() as i32;
    let surface_hits = surface_hits_for_shape(
        world,
        target,
        Shape::Sphere,
        radius,
        MIN_BUILD_Y,
        MAX_BUILD_Y,
        None,
    );
    let mut heights = HashMap::<(i32, i32), i32>::new();
    let mut surface_by_column = HashMap::<(i32, i32), SurfaceHit>::new();

    for hit in surface_hits {
        if mask.is_some_and(|mask| !mask.matches(hit.state)) {
            continue;
        }
        let profile = terrain_profile_height(hit.column.dx, hit.column.dz, settings.radius);
        if profile <= 0.0 {
            continue;
        }
        let new_height = terrain_target_height(hit.y, target.y, profile, settings, mode);
        heights.insert((hit.column.dx, hit.column.dz), new_height);
        surface_by_column.insert((hit.column.dx, hit.column.dz), hit);
    }

    if settings.smooth {
        smooth_height_targets(&mut heights);
    }

    let mut entry = EditEntry::default();
    for ((dx, dz), new_y) in heights {
        let Some(hit) = surface_by_column.get(&(dx, dz)).copied() else {
            continue;
        };
        if new_y > hit.y {
            for y in hit.y + 1..=new_y.min(MAX_BUILD_Y) {
                push_change(
                    key,
                    world,
                    &mut entry,
                    BlockPos {
                        x: hit.column.x,
                        y,
                        z: hit.column.z,
                    },
                    hit.state,
                    None,
                );
            }
        } else if new_y < hit.y {
            for y in (new_y.max(MIN_BUILD_Y) + 1)..=hit.y {
                push_change(
                    key,
                    world,
                    &mut entry,
                    BlockPos {
                        x: hit.column.x,
                        y,
                        z: hit.column.z,
                    },
                    0,
                    None,
                );
            }
        }
    }
    commit_entry(key, world, entry)
}

fn push_change(
    key: &str,
    world: &World,
    entry: &mut EditEntry,
    pos: BlockPos,
    after: u16,
    mask: Option<&BlockMask>,
) {
    let before = world.get_block_state_id(pos);
    if mask.is_some_and(|mask| !mask.matches(before))
        || !passes_gmask(key, before)
        || before == after
    {
        return;
    }
    entry.push_state_change(pos, before, after);
}

fn commit_entry(key: &str, world: &World, entry: EditEntry) -> usize {
    for batch in entry.changes.chunks(batch_size()) {
        let changes: Vec<_> = batch
            .iter()
            .map(|change| (change.pos, change.after.clone()))
            .collect();
        block_data::apply_blocks(world, &changes, block_flags());
    }
    finalize_brush_history(key, entry)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BrushColumn {
    x: i32,
    z: i32,
    dx: i32,
    dz: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SurfaceHit {
    column: BrushColumn,
    y: i32,
    state: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SplatterGrowthHit {
    hit: SurfaceHit,
    seed: SurfaceHit,
}

fn shape_columns(center: BlockPos, shape: Shape, radius: i32) -> Vec<BrushColumn> {
    let mut columns = Vec::new();
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if matches!(shape, Shape::Sphere | Shape::Cylinder)
                && dx * dx + dz * dz > radius * radius
            {
                continue;
            }
            columns.push(BrushColumn {
                x: center.x + dx,
                z: center.z + dz,
                dx,
                dz,
            });
        }
    }
    columns
}

fn top_solid_in_column_by<F>(
    x: i32,
    z: i32,
    min_y: i32,
    max_y: i32,
    height_mask: Option<&BlockMask>,
    mut sample: F,
) -> Option<(i32, u16)>
where
    F: FnMut(BlockPos) -> u16,
{
    for y in (min_y.max(MIN_BUILD_Y)..=max_y.min(MAX_BUILD_Y)).rev() {
        let state = sample(BlockPos { x, y, z });
        if state != 0 && height_mask.is_none_or(|mask| mask.matches(state)) {
            return Some((y, state));
        }
    }
    None
}

fn collect_surface_hits_with<F>(
    columns: &[BrushColumn],
    min_y: i32,
    max_y: i32,
    height_mask: Option<&BlockMask>,
    mut sample: F,
) -> Vec<SurfaceHit>
where
    F: FnMut(BlockPos) -> u16,
{
    let mut hits = Vec::with_capacity(columns.len());
    for column in columns {
        if let Some((y, state)) =
            top_solid_in_column_by(column.x, column.z, min_y, max_y, height_mask, &mut sample)
        {
            hits.push(SurfaceHit {
                column: *column,
                y,
                state,
            });
        }
    }
    hits
}

fn surface_hits_for_shape(
    world: &World,
    center: BlockPos,
    shape: Shape,
    radius: i32,
    min_y: i32,
    max_y: i32,
    height_mask: Option<&BlockMask>,
) -> Vec<SurfaceHit> {
    let columns = shape_columns(center, shape, radius);
    collect_surface_hits_with(&columns, min_y, max_y, height_mask, |pos| {
        world.get_block_state_id(pos)
    })
}

#[allow(dead_code)]
fn top_solid_in_column(
    world: &World,
    x: i32,
    z: i32,
    min_y: i32,
    max_y: i32,
    height_mask: Option<&BlockMask>,
) -> Option<(i32, u16)> {
    top_solid_in_column_by(x, z, min_y, max_y, height_mask, |pos| {
        world.get_block_state_id(pos)
    })
}

fn select_spaced_positions(
    positions: &[BlockPos],
    count: usize,
    min_distance: i32,
) -> Vec<BlockPos> {
    let mut ordered = positions.to_vec();
    ordered.sort_by_key(|pos| (position_hash(*pos), pos.x, pos.y, pos.z));
    let min_distance2 = min_distance.saturating_mul(min_distance);
    let mut selected = Vec::with_capacity(count.min(ordered.len()));
    for pos in ordered {
        if selected.len() >= count {
            break;
        }
        if min_distance <= 0
            || selected
                .iter()
                .all(|other| position_distance2(*other, pos) >= min_distance2)
        {
            selected.push(pos);
        }
    }
    selected
}

#[allow(dead_code)]
fn scatter_surface_hits(
    surface_hits: &[SurfaceHit],
    count: usize,
    min_distance: i32,
) -> Vec<SurfaceHit> {
    let selected_positions = select_spaced_positions(
        &surface_hits
            .iter()
            .map(|hit| BlockPos {
                x: hit.column.x,
                y: hit.y,
                z: hit.column.z,
            })
            .collect::<Vec<_>>(),
        count,
        min_distance,
    );
    let mut by_position = HashMap::<BlockPosKey, SurfaceHit>::new();
    for hit in surface_hits {
        by_position.insert(BlockPosKey(hit.column.x, hit.y, hit.column.z), *hit);
    }
    selected_positions
        .into_iter()
        .filter_map(|pos| by_position.get(&pos.into()).copied())
        .collect()
}

fn generate_splatter_hits(
    surface_hits: &[SurfaceHit],
    points: usize,
    recursion: u32,
) -> Vec<SplatterGrowthHit> {
    let seeds = scatter_surface_hits(surface_hits, points, 1);
    let by_column: HashMap<(i32, i32), SurfaceHit> = surface_hits
        .iter()
        .map(|hit| ((hit.column.dx, hit.column.dz), *hit))
        .collect();
    let mut visited = HashMap::<(i32, i32), SurfaceHit>::new();
    let mut growth = Vec::new();

    for seed in seeds {
        if visited
            .insert((seed.column.dx, seed.column.dz), seed)
            .is_none()
        {
            growth.push(SplatterGrowthHit { hit: seed, seed });
        }
        let mut frontier = vec![seed];
        for depth in 0..recursion {
            let mut next = Vec::new();
            for current in frontier {
                let mut branches = 0usize;
                for (dx, dz) in ordered_splatter_neighbors(seed, current) {
                    if branches >= 2 {
                        break;
                    }
                    let Some(hit) = by_column.get(&(dx, dz)).copied() else {
                        continue;
                    };
                    if visited.contains_key(&(dx, dz))
                        || !splatter_branch_allowed(seed, hit, depth + 1)
                    {
                        continue;
                    }
                    visited.insert((dx, dz), seed);
                    growth.push(SplatterGrowthHit { hit, seed });
                    next.push(hit);
                    branches += 1;
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
    }

    growth
}

fn ordered_splatter_neighbors(seed: SurfaceHit, current: SurfaceHit) -> Vec<(i32, i32)> {
    let mut neighbors = Vec::with_capacity(8);
    for dz in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dz == 0 {
                continue;
            }
            neighbors.push((current.column.dx + dx, current.column.dz + dz));
        }
    }
    neighbors.sort_by_key(|(dx, dz)| {
        position_hash(BlockPos {
            x: seed.column.x + dx - current.column.dx,
            y: seed.y,
            z: seed.column.z + dz - current.column.dz,
        })
    });
    neighbors
}

fn splatter_branch_allowed(seed: SurfaceHit, hit: SurfaceHit, depth: u32) -> bool {
    let branch_pos = BlockPos {
        x: hit.column.x,
        y: hit.y + depth as i32,
        z: hit.column.z,
    };
    let seed_hash = position_hash(BlockPos {
        x: seed.column.x,
        y: seed.y,
        z: seed.column.z,
    });
    let mixed = position_hash(branch_pos) ^ seed_hash.rotate_left(depth % 31);
    mixed % 5 < 2
}

fn position_distance2(a: BlockPos, b: BlockPos) -> i32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dx * dx + dy * dy + dz * dz
}

fn terrain_profile_height(dx: i32, dz: i32, radius: f64) -> f64 {
    let distance2 = f64::from(dx * dx + dz * dz);
    let radius2 = radius * radius;
    if distance2 > radius2 {
        return 0.0;
    }
    (radius - distance2.sqrt()).max(0.0)
}

fn terrain_target_height(
    old_y: i32,
    target_y: i32,
    profile: f64,
    settings: &TerrainBrushSettings,
    mode: TerrainMode,
) -> i32 {
    let raw_height = match mode {
        TerrainMode::RaiseLower => f64::from(old_y) + settings.y_scale * profile,
        TerrainMode::Flatten => {
            let radius = settings.radius.max(1.0);
            let factor = profile.powf(settings.y_scale) / radius.powf(settings.y_scale);
            f64::from(old_y) + f64::from(target_y - old_y) * factor
        }
    };
    raw_height
        .round()
        .clamp(f64::from(MIN_BUILD_Y), f64::from(MAX_BUILD_Y)) as i32
}

fn smooth_height_targets(heights: &mut HashMap<(i32, i32), i32>) {
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
            *height = (f64::from(total) / f64::from(count)).round() as i32;
        }
    }
}

fn finalize_brush_history(key: &str, entry: EditEntry) -> usize {
    let changed = entry.changes.len();
    history::push(key, entry);
    changed
}

fn deterministic_clipboard_rotation(target: BlockPos) -> Transform {
    match position_hash(target) % 4 {
        0 => Transform::identity(),
        1 => Transform::rotate_y(90).expect("valid quarter rotation"),
        2 => Transform::rotate_y(180).expect("valid half rotation"),
        _ => Transform::rotate_y(270).expect("valid three-quarter rotation"),
    }
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

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

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
    fn parses_phase_one_brush_settings() {
        let ParsedBrushCommand::Setting(BrushSetting::Visualization(mode)) =
            parse_brush_command("vis 2").expect("valid vis command")
        else {
            panic!("expected vis setting");
        };
        assert_eq!(mode.value(), 2);

        let ParsedBrushCommand::Setting(BrushSetting::TargetMode(mode)) =
            parse_brush_command("target 3").expect("valid target mode")
        else {
            panic!("expected target setting");
        };
        assert_eq!(mode, BrushTargetMode::TargetFaceRange);

        let ParsedBrushCommand::Setting(BrushSetting::Scroll(BrushScrollAction::Pattern(patterns))) =
            parse_brush_command("scroll pattern stone dirt").expect("valid scroll command")
        else {
            panic!("expected scroll pattern setting");
        };
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn parses_phase_one_stub_brushes() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("scatter stone 5 8 2").expect("valid scatter")
        else {
            panic!("expected scatter bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Scatter {
                points: 8,
                distance: 2,
                ..
            }
        ));

        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("flatten 7 0.5 mymap 90 -r -l -s").expect("valid flatten")
        else {
            panic!("expected flatten bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Flatten {
                settings: TerrainBrushSettings {
                    radius,
                    y_scale: 0.5,
                    rotation: 90,
                    random_rotate: true,
                    layers: true,
                    smooth: false,
                    ..
                },
            } if (radius - 7.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn parses_clipboard_random_rotate_flag() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("clipboard -aro").expect("valid clipboard")
        else {
            panic!("expected clipboard bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Clipboard {
                skip_air: true,
                paste_at_origin: true,
                random_rotate: true,
            }
        ));
    }

    #[test]
    fn parses_splatter_points_recursion_and_solid() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("splatter stone 6 4 7 false").expect("valid splatter")
        else {
            panic!("expected splatter bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Splatter {
                radius,
                points: 4,
                recursion: 7,
                solid: false,
                ..
            } if (radius - 6.0).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn parses_height_and_flatten_argument_orders() {
        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("height 7 1.5 mymap 90").expect("valid height")
        else {
            panic!("expected height bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Height {
                settings: TerrainBrushSettings {
                    radius,
                    y_scale: 1.5,
                    rotation: 90,
                    image: Some(ref image),
                    ..
                },
            } if (radius - 7.0).abs() < f64::EPSILON && image == "mymap"
        ));

        let ParsedBrushCommand::Bind(binding) =
            parse_brush_command("flatten 7 mymap 90 1.5").expect("valid flatten")
        else {
            panic!("expected flatten bind");
        };
        assert!(matches!(
            binding.kind,
            BrushKind::Flatten {
                settings: TerrainBrushSettings {
                    radius,
                    y_scale: 1.5,
                    rotation: 90,
                    image: Some(ref image),
                    ..
                },
            } if (radius - 7.0).abs() < f64::EPSILON && image == "mymap"
        ));
    }

    #[test]
    fn collects_surface_hits_with_masked_sampling() {
        let stone = mapping::resolve_block("stone").expect("stone");
        let dirt = mapping::resolve_block("dirt").expect("dirt");
        let mask = BlockMask::parse("stone").expect("mask");
        let columns = vec![
            BrushColumn {
                x: 0,
                z: 0,
                dx: 0,
                dz: 0,
            },
            BrushColumn {
                x: 1,
                z: 0,
                dx: 1,
                dz: 0,
            },
        ];
        let hits = collect_surface_hits_with(&columns, -1, 3, Some(&mask), |pos| {
            match (pos.x, pos.y, pos.z) {
                (0, 2, 0) => dirt,
                (0, 1, 0) => stone,
                (1, 0, 0) => stone,
                _ => 0,
            }
        });
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].y, 1);
        assert_eq!(hits[0].state, stone);
        assert_eq!(hits[1].y, 0);
        assert_eq!(hits[1].state, stone);
    }

    #[test]
    fn select_spaced_positions_is_deterministic() {
        let positions = vec![at(0, 0, 0), at(1, 0, 0), at(4, 0, 0), at(10, 0, 0)];
        let first = select_spaced_positions(&positions, 3, 4);
        let second = select_spaced_positions(&positions, 3, 4);
        let coords = |points: &[BlockPos]| {
            points
                .iter()
                .map(|pos| (pos.x, pos.y, pos.z))
                .collect::<Vec<_>>()
        };
        assert_eq!(coords(&first), coords(&second));
        assert!(first.len() <= 3);
        for i in 0..first.len() {
            for j in i + 1..first.len() {
                assert!(position_distance2(first[i], first[j]) >= 16);
            }
        }
    }

    #[test]
    fn scatter_surface_hits_preserve_surface_metadata() {
        let hits = vec![
            SurfaceHit {
                column: BrushColumn {
                    x: 0,
                    z: 0,
                    dx: 0,
                    dz: 0,
                },
                y: 5,
                state: 1,
            },
            SurfaceHit {
                column: BrushColumn {
                    x: 2,
                    z: 0,
                    dx: 2,
                    dz: 0,
                },
                y: 7,
                state: 10,
            },
            SurfaceHit {
                column: BrushColumn {
                    x: 8,
                    z: 0,
                    dx: 8,
                    dz: 0,
                },
                y: 9,
                state: 1,
            },
        ];
        let selected = scatter_surface_hits(&hits, 2, 3);
        assert_eq!(selected.len(), 2);
        for hit in selected {
            assert!(hits.contains(&hit));
        }
    }

    #[test]
    fn generate_splatter_hits_is_deterministic() {
        let hits = vec![
            SurfaceHit {
                column: BrushColumn {
                    x: 0,
                    z: 0,
                    dx: 0,
                    dz: 0,
                },
                y: 5,
                state: 1,
            },
            SurfaceHit {
                column: BrushColumn {
                    x: 1,
                    z: 0,
                    dx: 1,
                    dz: 0,
                },
                y: 5,
                state: 1,
            },
            SurfaceHit {
                column: BrushColumn {
                    x: 1,
                    z: 1,
                    dx: 1,
                    dz: 1,
                },
                y: 5,
                state: 1,
            },
            SurfaceHit {
                column: BrushColumn {
                    x: 2,
                    z: 1,
                    dx: 2,
                    dz: 1,
                },
                y: 5,
                state: 1,
            },
        ];
        let first = generate_splatter_hits(&hits, 2, 3);
        let second = generate_splatter_hits(&hits, 2, 3);
        let coords = |items: &[SplatterGrowthHit]| {
            items
                .iter()
                .map(|item| {
                    (
                        item.hit.column.x,
                        item.hit.column.z,
                        item.seed.column.x,
                        item.seed.column.z,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(coords(&first), coords(&second));
        assert!(!first.is_empty());
    }

    #[test]
    fn terrain_profile_falls_off_from_center() {
        assert_eq!(terrain_profile_height(0, 0, 5.0), 5.0);
        assert!(terrain_profile_height(3, 4, 5.0) <= f64::EPSILON);
        assert!(terrain_profile_height(1, 0, 5.0) < 5.0);
        assert_eq!(terrain_profile_height(6, 0, 5.0), 0.0);
    }

    #[test]
    fn terrain_target_height_matches_mode_intent() {
        let settings = TerrainBrushSettings {
            radius: 5.0,
            image: None,
            rotation: 0,
            y_scale: 2.0,
            random_rotate: false,
            layers: false,
            smooth: true,
        };
        assert_eq!(
            terrain_target_height(10, 20, 3.0, &settings, TerrainMode::RaiseLower),
            16
        );
        let flatten = terrain_target_height(10, 20, 5.0, &settings, TerrainMode::Flatten);
        assert_eq!(flatten, 20);
        let halfway = terrain_target_height(10, 20, 2.5, &settings, TerrainMode::Flatten);
        assert!(halfway > 10 && halfway < 20);
    }

    #[test]
    fn terrain_settings_reject_unimplemented_sources() {
        let image = TerrainBrushSettings {
            radius: 5.0,
            image: Some("mymap".to_string()),
            rotation: 0,
            y_scale: 1.0,
            random_rotate: false,
            layers: false,
            smooth: true,
        };
        assert!(image.validate_runtime().is_err());

        let layers = TerrainBrushSettings {
            radius: 5.0,
            image: None,
            rotation: 0,
            y_scale: 1.0,
            random_rotate: false,
            layers: true,
            smooth: true,
        };
        assert!(layers.validate_runtime().is_err());
    }

    #[test]
    fn finalize_brush_history_records_undoable_edits() {
        let key = "finalize_brush_history_records_undoable_edits";
        history::clear(key);

        let mut entry = EditEntry::default();
        entry.push_state_change(at(1, 2, 3), 0, 1);

        assert_eq!(finalize_brush_history(key, entry), 1);
        let popped = history::undo(key).expect("brush history entry");
        assert_eq!(popped.changes.len(), 1);
        assert_eq!(popped.changes[0].pos.x, 1);

        history::clear(key);
    }

    #[test]
    fn finalize_brush_history_skips_empty_edits() {
        let key = "finalize_brush_history_skips_empty_edits";
        history::clear(key);

        assert_eq!(finalize_brush_history(key, EditEntry::default()), 0);
        assert!(history::undo(key).is_none());
    }

    #[test]
    fn deterministic_clipboard_rotation_is_stable() {
        let first = deterministic_clipboard_rotation(at(10, 64, -3));
        let second = deterministic_clipboard_rotation(at(10, 64, -3));
        assert_eq!(first, second);
        assert!(matches!(
            first,
            t if t == Transform::identity()
                || t == Transform::rotate_y(90).expect("rotation")
                || t == Transform::rotate_y(180).expect("rotation")
                || t == Transform::rotate_y(270).expect("rotation")
        ));
    }

    #[test]
    fn rebinding_preserves_phase_one_binding_state() {
        let existing = BrushBinding {
            kind: BrushKind::Extinguish { radius: 3 },
            mask: Some(BlockMask::parse("stone").expect("valid mask")),
            range: 42.0,
            trace_mask: Some(BlockMask::parse("dirt").expect("valid mask")),
            target_mask: Some(BlockMask::parse("grass_block").expect("valid mask")),
            target_mode: BrushTargetMode::TargetFaceRange,
            visualization: BrushVisualizationMode(2),
            scroll_action: BrushScrollAction::Range,
            transform: BrushTransformSettings {
                transform: Transform::rotate_y(90).expect("transform"),
                random_rotate: true,
                auto_rotate: false,
            },
        };
        let rebound = BrushBinding::with_kind(
            BrushKind::Sphere {
                pattern: BlockPattern::parse("dirt").expect("valid pattern"),
                radius: 5.0,
                hollow: false,
            },
            Some(&existing),
        );
        assert!(rebound.mask.is_some());
        assert_eq!(rebound.range, 42.0);
        assert!(rebound.trace_mask.is_some());
        assert!(rebound.target_mask.is_some());
        assert_eq!(rebound.target_mode, BrushTargetMode::TargetFaceRange);
        assert_eq!(rebound.visualization.value(), 2);
        assert!(matches!(rebound.scroll_action, BrushScrollAction::Range));
        assert!(rebound.transform.random_rotate);
        assert!(matches!(rebound.kind, BrushKind::Sphere { .. }));
    }

    #[test]
    fn sphere_hollow_has_fewer_blocks() {
        let solid = sphere_positions(at(0, 0, 0), 3.0, false);
        let hollow = sphere_positions(at(0, 0, 0), 3.0, true);
        assert!(hollow.len() < solid.len());
    }
}
