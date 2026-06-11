//! `//copy` — copy the current selection into the player's clipboard.
//!
//! Mirrors WorldEdit's `ClipboardCommands#copy`. The clipboard origin is the
//! player's current position, so a later `//paste` is relative to wherever
//! the player is standing then (matching WorldEdit's "paste relative to where
//! you copied from" behaviour when you don't move).
//!
//! TODO(FAWE parity): WorldEdit's `//copy` supports `-e` (entities), `-b`
//! (biomes), and `-m <mask>` (only copy blocks matching a mask). Only block
//! states are captured here, unconditionally, via [`crate::clipboard::capture`].

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::clipboard;

use super::{command_names, require_selection, sender_block_pos};

pub fn register(context: &Context) {
    let copy_command = Command::new(
        &command_names("copy"),
        "Copy the selection to your clipboard",
    )
    .execute(CopyCommand);
    context.register_command(copy_command, "worldedit-rs:command.copy");
}

/// Handler for `//copy` — copies the current selection into the player's clipboard.
struct CopyCommand;

impl pumpkin_plugin_api::commands::CommandHandler for CopyCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        let Ok(origin) = sender_block_pos(&sender) else {
            return Ok(0);
        };

        let started = std::time::Instant::now();
        let buffer = clipboard::capture(&world, &region, origin);
        let blocks = buffer.blocks.len();
        clipboard::set(&key, buffer);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //copy captured {blocks} blocks in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Copied {blocks} blocks to your clipboard (relative to your position)."
        )));
        Ok(1)
    }
}
