//! `//cut` — copy the current selection into the clipboard, then clear it to
//! air.
//!
//! Mirrors WorldEdit's `ClipboardCommands#cut`, with the `leavePattern`
//! hard-coded to air.
//!
//! TODO(FAWE parity): WorldEdit's `//cut [leavePattern]` lets you specify what
//! replaces the cut region (default air), plus the same `-e`/`-b`/`-m` flags
//! as `//copy`. Only "replace with air" is implemented here.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{clipboard, history, history::EditEntry};

use super::{batch_size, block_flags, command_names, require_selection, sender_block_pos};

/// Air's global block-state id.
const AIR_STATE_ID: u16 = 0;

pub fn register(context: &Context) {
    let cut_command = Command::new(
        &command_names("cut"),
        "Cut the selection to your clipboard, replacing it with air",
    )
    .execute(CutCommand);
    context.register_command(cut_command, "worldedit-rs:command.cut");
}

/// Handler for `//cut` — copies the selection, then fills it with air.
struct CutCommand;

impl pumpkin_plugin_api::commands::CommandHandler for CutCommand {
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
        let copied = buffer.blocks.len();
        clipboard::set(&key, buffer);

        let mut cleared = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = world.get_block_state_id(pos);
                if before == AIR_STATE_ID {
                    continue;
                }
                entry.changes.push((pos, before, AIR_STATE_ID));
                changes.push(BlockChange {
                    pos,
                    state: AIR_STATE_ID,
                });
            }
            cleared += changes.len();
            if !changes.is_empty() {
                world.set_block_states(&changes, block_flags());
            }
        });
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //cut copied {copied} blocks and cleared {cleared} in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Cut {copied} blocks to your clipboard ({cleared} set to air)."
        )));
        Ok(1)
    }
}
