//! `//paste` — paste the player's clipboard at their current position.
//!
//! Mirrors WorldEdit's `ClipboardCommands#paste` "stamp" behaviour: every
//! captured cell (including air) overwrites the destination, anchored at the
//! player's current position using the same relative offsets recorded by
//! `//copy`/`//cut`.
//!
//! TODO(FAWE parity): WorldEdit's `//paste` supports `-a` (skip air), `-o`
//! (paste at the original copy origin instead of your current position), `-s`
//! (select the pasted region afterwards), `-n` (select without pasting), and
//! entity/biome pasting (`-e`/`-b`). None of these flags are implemented;
//! `//paste` here always stamps every captured cell at the player's position.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    common::BlockPos,
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{clipboard, history, history::EditEntry};

use super::{batch_size, block_flags, command_names, player_key, sender_block_pos};

pub fn register(context: &Context) {
    let paste_command = Command::new(
        &command_names("paste"),
        "Paste your clipboard at your position",
    )
    .execute(PasteCommand);
    context.register_command(paste_command, "worldedit-rs:command.paste");
}

/// Handler for `//paste` — pastes the player's clipboard at their current position.
struct PasteCommand;

impl pumpkin_plugin_api::commands::CommandHandler for PasteCommand {
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
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        let Some(world) = sender.world() else {
            sender.send_error(TextComponent::text("Could not determine your world."));
            return Ok(0);
        };
        let Ok(pos) = sender_block_pos(&sender) else {
            return Ok(0);
        };

        let Some(buffer) = clipboard::get(&key) else {
            sender.send_error(TextComponent::text(
                "Your clipboard is empty. Use //copy first.",
            ));
            return Ok(0);
        };

        let started = std::time::Instant::now();
        let mut placed = 0usize;
        let mut entry = EditEntry::default();
        for batch in buffer.blocks.chunks(batch_size()) {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &(offset, state) in batch {
                let target = BlockPos {
                    x: pos.x + offset.0,
                    y: pos.y + offset.1,
                    z: pos.z + offset.2,
                };
                let before = world.get_block_state_id(target);
                if before == state {
                    continue;
                }
                entry.changes.push((target, before, state));
                changes.push(BlockChange { pos: target, state });
            }
            placed += changes.len();
            if !changes.is_empty() {
                world.set_block_states(&changes, block_flags());
            }
        }
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //paste placed {placed} blocks in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!("Pasted {placed} blocks.")));
        Ok(1)
    }
}
