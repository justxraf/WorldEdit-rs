//! Command registration and shared helpers for WorldEdit-rs's `//` commands.
//!
//! Each command lives in its own module, mirroring how WorldEdit/FAWE splits
//! commands across `SelectionCommands`, `RegionCommands`, `ClipboardCommands`,
//! and `HistoryCommands`. [`register`] wires every command into the host.
//!
//! TODO(FAWE parity): WorldEdit registers commands under a `/we` (or `//`)
//! dispatcher with per-command permission nodes like
//! `worldedit.region.set` / `worldedit.history.undo`. Pumpkin's command API
//! takes one permission node per registered [`Command`], so each command here
//! gets its own `worldedit-rs:command.<name>` node (registered in [`register`])
//! rather than the finer-grained sub-permissions FAWE offers (e.g. per-mask
//! or per-pattern permissions). Not implemented — see the TODO list at the
//! bottom of this file.

mod copy;
mod cut;
mod paste;
mod pos;
mod redo;
mod replace;
mod sel;
mod set;
mod size;
mod undo;

use pumpkin_plugin_api::{
    Context,
    command::CommandSender,
    common::BlockPos,
    logging::{self, LogLevel},
    permission::{Permission, PermissionDefault},
    text::TextComponent,
    world::BlockFlags,
};

use crate::selection;

/// Register every `//` command and its permission node.
pub fn register(context: &Context) {
    for (node, description) in [
        (
            "worldedit-rs:command.pos",
            "Allows setting selection points with //pos1 and //pos2.",
        ),
        (
            "worldedit-rs:command.sel",
            "Allows clearing the selection with //sel.",
        ),
        (
            "worldedit-rs:command.set",
            "Allows filling the selection with //set.",
        ),
        (
            "worldedit-rs:command.replace",
            "Allows replacing blocks in the selection with //replace.",
        ),
        (
            "worldedit-rs:command.copy",
            "Allows copying the selection with //copy.",
        ),
        (
            "worldedit-rs:command.cut",
            "Allows cutting the selection with //cut.",
        ),
        (
            "worldedit-rs:command.paste",
            "Allows pasting the clipboard with //paste.",
        ),
        (
            "worldedit-rs:command.undo",
            "Allows undoing your last edit with //undo.",
        ),
        (
            "worldedit-rs:command.redo",
            "Allows redoing your last undone edit with //redo.",
        ),
        (
            "worldedit-rs:command.size",
            "Allows viewing selection info with //size.",
        ),
    ] {
        if let Err(e) = context.register_permission(&Permission {
            node: node.to_string(),
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

    logging::log(
        LogLevel::Info,
        "WorldEdit-rs: //pos1, //pos2, //sel, //set, //replace, //copy, //cut, //paste, //undo, \
         //redo, //size registered.",
    );
}

/// Resolve a player's name from the command sender, used as the key for
/// per-player selection, clipboard, and history state.
pub fn player_key(sender: &CommandSender) -> Option<String> {
    sender.as_player().map(|_| sender.get_name())
}

/// Names to register a `//<name>` command under: the bare literal (e.g.
/// `pos1`, which is what `//pos1` resolves to after the client and dispatcher
/// each strip one leading `/`) plus a `/`-prefixed alias (e.g. `/pos1`) so
/// the literal also appears in the suggestion graph for `//` tab-completion.
pub fn command_names(name: &str) -> Vec<String> {
    vec![name.to_string(), format!("/{name}")]
}

/// Common setup shared by every region command: requires a player, a position,
/// a world, and a completed selection. Returns `(player_key, world, region)`.
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
/// per-block callbacks/physics — keeps large `//set`/`//replace`/`//paste`
/// operations quiet and fast.
///
/// TODO(FAWE parity): FAWE exposes this as the "side effects" / `-n` (no
/// physics) toggle on commands like `//set` and `//paste` (see `//update` and
/// `SideEffectSet`). Here it's always-on and not configurable per command.
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

/// Batch size for `set_block_states`/`get_block_state_id` loops — keeps a
/// single region operation from building one enormous change list (a whole
/// region collected into one `Vec` can overflow the plugin's 32-bit wasm
/// linear memory).
pub fn batch_size() -> usize {
    1 << 16 // 65,536
}

// ---------------------------------------------------------------------------
// TODO(FAWE parity): commands not yet implemented.
//
// Selection (SelectionCommands):
// - //hpos1, //hpos2 — set a position to the block the player is looking at
//   (needs a block-trace/raycast API from the host; not currently exposed).
// - //expand, //contract, //shift — resize/move the selection by an amount.
// - //count, //distr — analyze blocks in the selection by mask.
// - Non-cuboid selections (polygon, cylinder, sphere, convex). Only the
//   axis-aligned cuboid `Region` in `crate::selection` is supported.
//
// Region edits (RegionCommands):
// - //walls, //faces/outline, //overlay, //center, //hollow, //naturalize.
// - //line, //curve, //move, //stack, //regen, //deform, //smooth.
// - Masks (`-m`) and complex patterns (e.g. `50%stone,50%dirt`,
//   `#existing`). //set and //replace currently take one literal block name
//   or numeric state id each, resolved via `crate::mapping::resolve_block`.
//
// Clipboard (ClipboardCommands):
// - //rotate, //flip, //clearclipboard.
// - Entity/biome copy (`-e`, `-b` flags) — only block states are captured.
// - `-a` (skip air) flag for //paste; current //paste always overwrites with
//   the captured block, including air ("stamp" semantics).
// - Schematic load/save (`//schem load|save`) — `crate::schem_paste` and
//   `crate::schematic` already implement `.schem` parsing/pasting and just
//   need a command + clipboard integration.
//
// History (HistoryCommands):
// - //clearhistory.
// - `//undo [times] [player]` / `//redo [times] [player]` — only an optional
//   `times` count for the invoking player is implemented; the `player`
//   argument (for operators undoing others' edits) is not.
// - Disk-backed history for very large edits (see `crate::history`).
//
// General:
// - Per-command fine-grained permissions beyond one node per command.
// - A "//" wand (selection tool) bound to an item, and tool-based commands
//   (`//brush`, etc.) — entirely out of scope for this plugin so far.
// ---------------------------------------------------------------------------
