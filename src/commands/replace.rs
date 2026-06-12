//! `//replace [from] <to>` - replace blocks in the current selection.
//!
//! Mirrors WorldEdit/FAWE's default mask behavior for the one-argument form:
//! `//replace <to>` replaces every non-air block (`!#air`). The two-argument
//! form `//replace <from> <to>` parses `from` as a [`BlockMask`], so
//! comma-separated multi-block masks like `//replace stone,andesite glass`
//! work as in WorldEdit/FAWE. The target accepts this plugin's supported
//! pattern subset. The player's global mask (`//gmask`) is applied on top of
//! `from`.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::{
    block_data, history,
    history::EditEntry,
    mapping,
    pattern::{BlockMask, BlockPattern, PatternEvalContext},
};

use super::{batch_size, block_flags, command_names, passes_gmask, require_selection};

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

        let from_mask = match &from {
            Some(from) => match BlockMask::parse(from) {
                Ok(mask) => mask,
                Err(message) => {
                    sender.send_error(TextComponent::text(&message));
                    return Ok(0);
                }
            },
            None => BlockMask::Not(Box::new(BlockMask::Air)),
        };
        let to = match BlockPattern::parse(&raw_to) {
            Ok(pattern) => pattern,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };
        let pattern_ctx = PatternEvalContext::for_operation(region.min, &key, &world);
        if let Err(message) = to.validate(&pattern_ctx) {
            sender.send_error(TextComponent::text(&message));
            return Ok(0);
        }

        let started = std::time::Instant::now();
        let mut replaced = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = block_data::capture_block(&world, pos);
                if from_mask.matches(before.state_id) && passes_gmask(&key, before.state_id) {
                    let after = to.placement_at_with(pos, &before, &pattern_ctx);
                    if before == after {
                        continue;
                    }
                    entry.push_change(pos, before, after.clone());
                    changes.push((pos, after));
                }
            }
            if !changes.is_empty() {
                replaced += changes.len();
                block_data::apply_blocks(&world, &changes, block_flags());
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
        match (&from, &from_mask) {
            (Some(from), BlockMask::States(states)) if states.len() == 1 => {
                message.add_child(mapping::display_component(from, states[0]))
            }
            (Some(from), _) => message.add_text(from),
            (None, _) => message.add_text("non-air"),
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_arg_replace_targets_non_air() {
        let mask = BlockMask::Not(Box::new(BlockMask::Air));
        assert!(mask.matches(1));
        assert!(!mask.matches(0));
    }

    #[test]
    fn two_arg_replace_targets_only_source_state() {
        let mask = BlockMask::parse("stone").unwrap();
        assert!(mask.matches(1));
        assert!(!mask.matches(3));
    }

    #[test]
    fn multi_block_from_matches_any_listed_block() {
        let mask = BlockMask::parse("stone,dirt").unwrap();
        assert!(mask.matches(1));
        assert!(mask.matches(10));
        assert!(!mask.matches(470));
    }
}
