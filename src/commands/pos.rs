//! `//pos1` and `//pos2` — set selection corners to the player's position.
//!
//! Mirrors WorldEdit's `SelectionCommands#pos1`/`pos2`.
//!
//! TODO(FAWE parity): WorldEdit's `//pos1`/`//pos2` accept an optional
//! `coordinates` argument to set the position explicitly instead of using the
//! player's current position. Not implemented — only the no-argument form is
//! registered here.
//!
//! TODO(FAWE parity): `//hpos1`/`//hpos2` (set from the block the player is
//! looking at) require a block-trace/raycast API the host doesn't currently
//! expose.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

use crate::selection;

use super::{command_names, player_key, sender_block_pos};

pub fn register(context: &Context) {
    let pos1 = Command::new(
        &command_names("pos1"),
        "Set selection point 1 to your position",
    )
    .execute(Pos1Command);
    context.register_command(pos1, "worldedit-rs:command.pos");

    let pos2 = Command::new(
        &command_names("pos2"),
        "Set selection point 2 to your position",
    )
    .execute(Pos2Command);
    context.register_command(pos2, "worldedit-rs:command.pos");
}

/// Handler for `//pos1` — sets selection point 1 to the player's position.
struct Pos1Command;

impl pumpkin_plugin_api::commands::CommandHandler for Pos1Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Ok(pos) = sender_block_pos(&sender) else {
            return Ok(0);
        };
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        selection::with_selection_mut(&key, |sel| sel.pos1 = Some(pos));
        sender.send_message(TextComponent::text(&format!(
            "Position 1 set to ({}, {}, {}).",
            pos.x, pos.y, pos.z
        )));
        Ok(1)
    }
}

/// Handler for `//pos2` — sets selection point 2 to the player's position.
struct Pos2Command;

impl pumpkin_plugin_api::commands::CommandHandler for Pos2Command {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Ok(pos) = sender_block_pos(&sender) else {
            return Ok(0);
        };
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        selection::with_selection_mut(&key, |sel| sel.pos2 = Some(pos));
        sender.send_message(TextComponent::text(&format!(
            "Position 2 set to ({}, {}, {}).",
            pos.x, pos.y, pos.z
        )));
        Ok(1)
    }
}
