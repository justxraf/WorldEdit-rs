//! `//replace [from] <to>` - replace blocks in the current selection.
//!
//! Mirrors WorldEdit/FAWE's default mask behavior for the one-argument form:
//! `//replace <to>` replaces every non-air block. The two-argument form
//! `//replace <from> <to>` replaces one literal source block state. The target
//! accepts this plugin's supported pattern subset.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{
    history,
    history::EditEntry,
    mapping,
    pattern::{BlockPattern, PatternEvalContext},
};

use super::{batch_size, block_flags, command_names, require_selection};

pub fn register(context: &Context) {
    let to_arg = CommandNode::argument("to", &ArgumentType::String(StringType::Greedy))
        .execute(ReplaceCommand);
    let from_or_to_arg =
        CommandNode::argument("from_or_to", &ArgumentType::String(StringType::SingleWord))
            .execute(ReplaceCommand);
    from_or_to_arg.then(to_arg);
    let replace_command =
        Command::new(&command_names("replace"), "Replace blocks in the selection");
    replace_command.then(from_or_to_arg);
    context.register_command(replace_command, "worldedit.region.replace");
}

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

        let first = match args.get_value("from_or_to") {
            Arg::Simple(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block name or pattern, got {other:?}"
                )));
                return Ok(0);
            }
        };
        let (from, raw_to) = match args.get_value("to") {
            Arg::Simple(to) | Arg::Msg(to) => (Some(first), to),
            _ => (None, first),
        };

        let from_id = if let Some(from) = &from {
            let Some(from_id) = mapping::resolve_block(from) else {
                sender.send_error(TextComponent::text(&format!("Unknown block '{from}'.")));
                return Ok(0);
            };
            Some(from_id)
        } else {
            None
        };
        let to = match BlockPattern::parse(&raw_to) {
            Ok(pattern) => pattern,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };
        let pattern_ctx = PatternEvalContext::for_player(region.min, &key);
        if let Err(message) = to.validate(&pattern_ctx) {
            sender.send_error(TextComponent::text(&message));
            return Ok(0);
        }

        let started = std::time::Instant::now();
        let mut replaced = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = world.get_block_state_id(pos);
                if should_replace(before, from_id) {
                    let after = to.state_at_with(pos, before, &pattern_ctx);
                    if before == after {
                        continue;
                    }
                    entry.changes.push((pos, before, after));
                    changes.push(BlockChange { pos, state: after });
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
                "WorldEdit-rs: //replace replaced {replaced} blocks in {:?}.",
                started.elapsed()
            ),
        );
        let message = TextComponent::text(&format!("Replaced {replaced} blocks of "));
        if let (Some(from), Some(from_id)) = (&from, from_id) {
            message.add_child(mapping::display_component(from, from_id));
        } else {
            message.add_text("non-air");
        }
        message.add_text(" with ");
        if let Some((input, state_id)) = to.literal_display() {
            message.add_child(mapping::display_component(input, state_id));
        } else {
            message.add_text(to.description());
        }
        message.add_text(".");
        sender.send_message(message);
        Ok(1)
    }
}

fn should_replace(before: u16, from_id: Option<u16>) -> bool {
    match from_id {
        Some(from_id) => before == from_id,
        None => before != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_arg_replace_targets_non_air() {
        assert!(should_replace(1, None));
        assert!(!should_replace(0, None));
    }

    #[test]
    fn two_arg_replace_targets_only_source_state() {
        assert!(should_replace(1, Some(1)));
        assert!(!should_replace(3, Some(1)));
    }
}
