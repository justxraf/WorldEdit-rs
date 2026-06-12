//! Command registration and shared helpers for WorldEdit-rs's `//` commands.
//!
//! Each command lives in its own module, mirroring how WorldEdit/FAWE splits
//! commands across `SelectionCommands`, `RegionCommands`, `ClipboardCommands`,
//! and `HistoryCommands`. [`register`] wires every command into the host.
//!
//! Commands use WorldEdit's documented permission node suffixes where Pumpkin
//! can attach a node directly to the registered [`Command`]. Pumpkin requires
//! plugin-owned permission nodes, so `worldedit.region.set` is registered as
//! `worldedit-rs:worldedit.region.set`. Internal `worldedit-rs:command.<name>`
//! nodes are kept only where WorldEdit documents no command permission, or
//! where a single Pumpkin command contains multiple WorldEdit subcommands that
//! need handler-level checks.

mod brush;
mod clearclipboard;
mod clearhistory;
mod copy;
mod count;
mod cut;
mod gmask;
mod paste;
mod pos;
mod redo;
mod replace;
mod schematic;
mod sel;
mod set;
mod shell;
mod size;
mod transform;
mod undo;
mod wand;

use pumpkin_plugin_api::{
    Context,
    command::CommandSender,
    common::BlockPos,
    logging::{self, LogLevel},
    permission::{Permission, PermissionDefault},
    text::TextComponent,
    world::BlockFlags,
};

use crate::{mask, selection};

const PERMISSION_NAMESPACE: &str = "worldedit-rs";

/// Register every `//` command and its permission node.
pub fn register(context: &Context) {
    for (node, description) in [
        (
            "worldedit-rs:command.brush",
            "Allows using the //brush and //br dispatcher.",
        ),
        ("worldedit.brush.sphere", "Allows binding the sphere brush."),
        (
            "worldedit.brush.cylinder",
            "Allows binding the cylinder brush.",
        ),
        ("worldedit.brush.set", "Allows binding the set brush."),
        (
            "worldedit.brush.clipboard",
            "Allows binding the clipboard brush.",
        ),
        ("worldedit.brush.smooth", "Allows binding the smooth brush."),
        (
            "worldedit.brush.gravity",
            "Allows binding the gravity brush.",
        ),
        ("worldedit.brush.ex", "Allows binding the extinguish brush."),
        (
            "worldedit.brush.splatter",
            "Allows binding the splatter brush.",
        ),
        ("worldedit.brush.raise", "Allows binding the raise brush."),
        ("worldedit.brush.lower", "Allows binding the lower brush."),
        (
            "worldedit.brush.morph",
            "Allows binding erode, dilate, and morph brushes.",
        ),
        ("worldedit.brush.snow", "Allows binding the snow brush."),
        (
            "worldedit.brush.options.size",
            "Allows changing a bound brush's size.",
        ),
        (
            "worldedit.brush.options.material",
            "Allows changing a bound brush's material.",
        ),
        (
            "worldedit.brush.options.mask",
            "Allows changing a bound brush's mask.",
        ),
        (
            "worldedit.brush.options.range",
            "Allows changing a bound brush's range.",
        ),
        (
            "worldedit.brush.options.tracemask",
            "Allows changing a bound brush's trace mask.",
        ),
        (
            "worldedit.selection.pos",
            "Allows setting selection points with //pos1 and //pos2.",
        ),
        (
            "worldedit.selection.hpos",
            "Allows setting selection points with //hpos1 and //hpos2.",
        ),
        (
            "worldedit.analysis.sel",
            "Allows clearing or changing the selection type with //sel.",
        ),
        (
            "worldedit.region.set",
            "Allows filling the selection with //set.",
        ),
        (
            "worldedit.region.replace",
            "Allows replacing blocks in the selection with //replace.",
        ),
        (
            "worldedit.clipboard.copy",
            "Allows copying the selection with //copy.",
        ),
        (
            "worldedit.clipboard.cut",
            "Allows cutting the selection with //cut.",
        ),
        (
            "worldedit.clipboard.paste",
            "Allows pasting the clipboard with //paste.",
        ),
        (
            "worldedit.history.undo",
            "Allows undoing your last edit with //undo.",
        ),
        (
            "worldedit.history.redo",
            "Allows redoing your last undone edit with //redo.",
        ),
        (
            "worldedit.selection.size",
            "Allows viewing selection info with //size.",
        ),
        (
            "worldedit.clipboard.clear",
            "Allows clearing your clipboard with //clearclipboard.",
        ),
        (
            "worldedit.history.clear",
            "Allows clearing your history with //clearhistory.",
        ),
        (
            "worldedit.selection.expand",
            "Allows expanding selections with //expand.",
        ),
        (
            "worldedit.selection.contract",
            "Allows contracting selections with //contract.",
        ),
        (
            "worldedit.selection.shift",
            "Allows shifting selections with //shift.",
        ),
        (
            "worldedit.selection.outset",
            "Allows outsetting selections with //outset.",
        ),
        (
            "worldedit.selection.inset",
            "Allows insetting selections with //inset.",
        ),
        (
            "worldedit.analysis.count",
            "Allows counting blocks with //count.",
        ),
        (
            "worldedit.region.walls",
            "Allows building selection walls with //walls.",
        ),
        (
            "worldedit.region.faces",
            "Allows building selection faces with //faces and //outline.",
        ),
        (
            "worldedit.wand",
            "Allows getting the selection wand with //wand.",
        ),
        (
            "worldedit.schematic.save",
            "Allows saving schematics with //schematic save.",
        ),
        (
            "worldedit.schematic.load",
            "Allows loading schematics with //schematic load.",
        ),
        (
            "worldedit.schematic.list",
            "Allows listing schematics with //schematic list.",
        ),
        (
            "worldedit-rs:command.schematic",
            "Allows using the //schematic dispatcher.",
        ),
        (
            "worldedit.global-mask",
            "Allows setting a global mask with //gmask.",
        ),
    ] {
        let node = permission_node(node);
        if let Err(e) = context.register_permission(&Permission {
            node: node.clone(),
            description: description.to_string(),
            default: PermissionDefault::Allow,
            children: Vec::new(),
        }) {
            logging::log(
                LogLevel::Warn,
                &format!("WorldEdit-rs: failed to register permission node {node}: {e}"),
            );
        }
    }

    pos::register(context);
    sel::register(context);
    set::register(context);
    replace::register(context);
    copy::register(context);
    cut::register(context);
    paste::register(context);
    undo::register(context);
    redo::register(context);
    size::register(context);
    clearclipboard::register(context);
    clearhistory::register(context);
    transform::register(context);
    count::register(context);
    shell::register(context);
    wand::register(context);
    schematic::register(context);
    brush::register(context);
    gmask::register(context);

    logging::log(
        LogLevel::Info,
        "WorldEdit-rs: //pos1, //pos2, //hpos1, //hpos2, //sel, //set, //replace, //copy, //cut, \
         //paste, //undo, //redo, //size, //clearclipboard, //clearhistory, //expand, //contract, \
         //shift, //outset, //inset, //count, //walls, //faces, //outline, //wand, //schematic \
         (//schem), //brush (//br), //gmask registered.",
    );
}

/// Resolve a player's name from the command sender, used as the key for
/// per-player selection, clipboard, and history state.
pub fn player_key(sender: &CommandSender) -> Option<String> {
    sender.as_player().map(|_| sender.get_name())
}

/// Names to register a `//<name>` command under: the bare literal plus a
/// `/`-prefixed alias for `//` tab-completion.
pub fn command_names(name: &str) -> Vec<String> {
    vec![name.to_string(), format!("/{name}")]
}

/// Pumpkin requires plugin-owned permission nodes. Command registration already
/// prefixes permission strings without a namespace, but explicit permission
/// registration and handler-level checks need to do the same work here.
pub fn permission_node(node: &str) -> String {
    if node.contains(':') {
        node.to_string()
    } else {
        format!("{PERMISSION_NAMESPACE}:{node}")
    }
}

/// Enforce a subcommand permission from inside a handler.
///
/// Pumpkin can attach only one permission to each registered command tree, so
/// nested command trees such as `//schematic save|load|list` use this extra
/// check to keep their WorldEdit permission nodes distinct.
pub fn require_permission(
    sender: &CommandSender,
    server: &pumpkin_plugin_api::Server,
    node: &str,
) -> std::result::Result<(), ()> {
    if sender.has_permission(server, &permission_node(node)) {
        Ok(())
    } else {
        sender.send_error(TextComponent::text(
            "You do not have permission to use this command.",
        ));
        Err(())
    }
}

/// Common setup shared by every region command: requires a player, a world,
/// and a completed selection. Returns `(player_key, world, region)`.
pub fn require_selection(
    sender: &CommandSender,
) -> std::result::Result<(String, pumpkin_plugin_api::world::World, selection::Region), ()> {
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
    let region = selection::with_selection(&key, |sel| sel.region());
    match region {
        Some(region) => Ok((key, world, region)),
        None => {
            sender.send_error(TextComponent::text("Set both //pos1 and //pos2 first."));
            Err(())
        }
    }
}

/// Block-update flags for bulk edits: force the state, skip drops, and skip
/// per-block callbacks/physics to keep large operations quiet and fast.
///
/// FAWE exposes this as the "side effects" / `-n` toggle on commands like
/// `//set` and `//paste`. WorldEdit-rs currently uses quiet bulk-edit flags
/// globally; `//set -n` is accepted for command compatibility.
pub fn block_flags() -> BlockFlags {
    BlockFlags::SKIP_DROPS
        | BlockFlags::FORCE_STATE
        | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
        | BlockFlags::SKIP_BLOCK_ENTITY_REPLACED_CALLBACK
}

/// Sender's current block position, or an error message if unavailable.
pub fn sender_block_pos(sender: &CommandSender) -> std::result::Result<BlockPos, ()> {
    match sender.position() {
        Some((x, y, z)) => Ok(BlockPos {
            x: x.floor() as i32,
            y: y.floor() as i32,
            z: z.floor() as i32,
        }),
        None => {
            sender.send_error(TextComponent::text("Could not determine your position."));
            Err(())
        }
    }
}

/// Batch size for `set_block_states`/`get_block_state_id` loops. This keeps a
/// single region operation from building one enormous change list in wasm
/// linear memory.
pub fn batch_size() -> usize {
    1 << 16 // 65,536
}

/// `true` if `before` passes the player's global mask (`//gmask`), or if no
/// global mask is set.
///
/// Every edit path (`//set`, `//replace`, `//cut`'s leave-fill, `//paste`,
/// shell commands, and brushes) should skip a position - not add it to its
/// change list - when this returns `false`, layering `//gmask` on top of
/// whatever mask the command itself applies.
pub fn passes_gmask(key: &str, before: u16) -> bool {
    mask::passes(key, before)
}
