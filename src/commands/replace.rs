//! `//replace <from> <to>` — replace one block type with another within the
//! current selection.
//!
//! Mirrors WorldEdit's `RegionCommands#replace` (aliases `/re`, `/rep`), minus
//! mask/pattern support: both arguments are single block names or numeric
//! global state ids resolved via [`crate::mapping::resolve_block`].
//!
//! TODO(FAWE parity): WorldEdit's `//replace [from] <to>` makes `from` an
//! optional `Mask` (defaulting to "anything that isn't air") and `to` a full
//! `Pattern`. Here both are required, single literal block names or ids.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{history, history::EditEntry, mapping};

use super::{batch_size, block_flags, command_names, require_selection};

pub fn register(context: &Context) {
    let to_arg = CommandNode::argument("to", &ArgumentType::String(StringType::SingleWord))
        .execute(ReplaceCommand);
    let from_arg = CommandNode::argument("from", &ArgumentType::String(StringType::SingleWord));
    from_arg.then(to_arg);
    let replace_command =
        Command::new(&command_names("replace"), "Replace blocks in the selection");
    replace_command.then(from_arg);
    context.register_command(replace_command, "worldedit-rs:command.replace");
}

/// Handler for `//replace <from> <to>` — replaces matching blocks within the
/// current selection.
struct ReplaceCommand;

impl pumpkin_plugin_api::commands::CommandHandler for ReplaceCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };

        let from = match args.get_value("from") {
            Arg::Simple(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block name, got {other:?}"
                )));
                return Ok(0);
            }
        };
        let to = match args.get_value("to") {
            Arg::Simple(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block name, got {other:?}"
                )));
                return Ok(0);
            }
        };

        let Some(from_id) = mapping::resolve_block(&from) else {
            sender.send_error(TextComponent::text(&format!("Unknown block '{from}'.")));
            return Ok(0);
        };
        let Some(to_id) = mapping::resolve_block(&to) else {
            sender.send_error(TextComponent::text(&format!("Unknown block '{to}'.")));
            return Ok(0);
        };

        let started = std::time::Instant::now();
        let mut replaced = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = world.get_block_state_id(pos);
                if before == from_id && before != to_id {
                    entry.changes.push((pos, before, to_id));
                    changes.push(BlockChange { pos, state: to_id });
                }
            }
            if !changes.is_empty() {
                replaced += changes.len();
                world.set_block_states(&changes, block_flags());
            }
        });
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //replace {from} {to} replaced {replaced} blocks in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Replaced {replaced} blocks of '{from}' with '{to}'."
        )));
        Ok(1)
    }
}
