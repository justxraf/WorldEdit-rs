//! `//pos1`, `//pos2`, `//hpos1`, and `//hpos2`.
//!
//! Mirrors WorldEdit/FAWE's cuboid selection point commands. `//pos1` and
//! `//pos2` accept optional explicit coordinates, otherwise they use the
//! player's current block position. `//hpos1` and `//hpos2` use the block the
//! player is looking at.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType},
    common::BlockPos,
    text::TextComponent,
};

use crate::selection;

use super::{player_key, sender_block_pos};

const HPOS_MAX_DISTANCE: f64 = 300.0;

pub fn register(context: &Context) {
    let pos1_coord =
        CommandNode::argument("coordinates", &ArgumentType::BlockPos).execute(Pos1Command);
    let pos1 = Command::new(
        &[
            "pos1".to_string(),
            "/pos1".to_string(),
            "1".to_string(),
            "/1".to_string(),
        ],
        "Set selection point 1",
    )
    .execute(Pos1Command);
    pos1.then(pos1_coord);
    context.register_command(pos1, "worldedit.selection.pos");

    let pos2_coord =
        CommandNode::argument("coordinates", &ArgumentType::BlockPos).execute(Pos2Command);
    let pos2 = Command::new(
        &[
            "pos2".to_string(),
            "/pos2".to_string(),
            "2".to_string(),
            "/2".to_string(),
        ],
        "Set selection point 2",
    )
    .execute(Pos2Command);
    pos2.then(pos2_coord);
    context.register_command(pos2, "worldedit.selection.pos");

    let hpos1 = Command::new(
        &["hpos1".to_string(), "/hpos1".to_string()],
        "Set selection point 1 to the block you are looking at",
    )
    .execute(HPos1Command);
    context.register_command(hpos1, "worldedit.selection.hpos");

    let hpos2 = Command::new(
        &["hpos2".to_string(), "/hpos2".to_string()],
        "Set selection point 2 to the block you are looking at",
    )
    .execute(HPos2Command);
    context.register_command(hpos2, "worldedit.selection.hpos");
}

fn explicit_pos(args: &ConsumedArgs) -> Option<BlockPos> {
    match args.get_value("coordinates") {
        Arg::BlockPos(pos) => Some(pos),
        _ => None,
    }
}

fn sender_or_explicit_pos(sender: &CommandSender, args: &ConsumedArgs) -> Result<BlockPos, ()> {
    if let Some(pos) = explicit_pos(args) {
        Ok(pos)
    } else {
        sender_block_pos(sender)
    }
}

fn target_block_pos(sender: &CommandSender) -> Result<BlockPos, ()> {
    let Some(player) = sender.as_player() else {
        sender.send_error(TextComponent::text("Only players can use this command."));
        return Err(());
    };
    let Some(hit) = player.as_entity().raycast(HPOS_MAX_DISTANCE, false) else {
        sender.send_error(TextComponent::text("No block in sight."));
        return Err(());
    };
    Ok(hit.pos)
}

fn set_point(
    sender: &CommandSender,
    point: u8,
    pos: BlockPos,
) -> std::result::Result<i32, CommandError> {
    let Some(key) = player_key(sender) else {
        return Ok(0);
    };
    selection::with_selection_mut(&key, |sel| match point {
        1 => sel.pos1 = Some(pos),
        2 => sel.pos2 = Some(pos),
        _ => {}
    });
    sender.send_message(TextComponent::text(&format!(
        "Position {point} set to ({}, {}, {}).",
        pos.x, pos.y, pos.z
    )));
    Ok(1)
}

struct Pos1Command;

impl pumpkin_plugin_api::commands::CommandHandler for Pos1Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Ok(pos) = sender_or_explicit_pos(&sender, &args) else {
            return Ok(0);
        };
        set_point(&sender, 1, pos)
    }
}

struct Pos2Command;

impl pumpkin_plugin_api::commands::CommandHandler for Pos2Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Ok(pos) = sender_or_explicit_pos(&sender, &args) else {
            return Ok(0);
        };
        set_point(&sender, 2, pos)
    }
}

struct HPos1Command;

impl pumpkin_plugin_api::commands::CommandHandler for HPos1Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok(pos) = target_block_pos(&sender) else {
            return Ok(0);
        };
        set_point(&sender, 1, pos)
    }
}

struct HPos2Command;

impl pumpkin_plugin_api::commands::CommandHandler for HPos2Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok(pos) = target_block_pos(&sender) else {
            return Ok(0);
        };
        set_point(&sender, 2, pos)
    }
}
