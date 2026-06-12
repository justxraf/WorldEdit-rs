//! `//set <pattern>` - fill the current selection with a block pattern.
//!
//! Mirrors the block-state subset of WorldEdit/FAWE's `RegionCommands#set`:
//! literal blocks, `#existing`, and simple weighted mixes such as
//! `50%stone,50%dirt`. The FAWE `-n` side-effect flag is accepted for command
//! compatibility; bulk edits already use quiet block flags. Clipboard and
//! expression-backed patterns still require FAWE's full pattern engine.

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
    pattern::{BlockPattern, PatternEvalContext},
};

use super::{batch_size, block_flags, command_names, passes_gmask, require_selection};

pub fn register(context: &Context) {
    let pattern_arg = CommandNode::argument("pattern", &ArgumentType::String(StringType::Greedy))
        .execute(SetCommand);
    let set_command = Command::new(&command_names("set"), "Fill the selection with a pattern");
    set_command.then(pattern_arg);
    context.register_command(set_command, "worldedit.region.set");
}

/// Handler for `//set <pattern>` - fills the current selection with a pattern.
struct SetCommand;

impl pumpkin_plugin_api::commands::CommandHandler for SetCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };

        let raw_pattern = match args.get_value("pattern") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block pattern, got {other:?}"
                )));
                return Ok(0);
            }
        };

        let raw_pattern = match parse_set_args(&raw_pattern) {
            Ok(raw_pattern) => raw_pattern,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };
        let pattern = match BlockPattern::parse(raw_pattern) {
            Ok(pattern) => pattern,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };
        let pattern_ctx = PatternEvalContext::for_operation(region.min, &key, &world);
        if let Err(message) = pattern.validate(&pattern_ctx) {
            sender.send_error(TextComponent::text(&message));
            return Ok(0);
        }

        let started = std::time::Instant::now();
        let mut placed = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = block_data::capture_block(&world, pos);
                if !passes_gmask(&key, before.state_id) {
                    continue;
                }
                let after = pattern.placement_at_with(pos, &before, &pattern_ctx);
                if before == after {
                    continue;
                }
                entry.push_change(pos, before, after.clone());
                changes.push((pos, after));
            }
            placed += changes.len();
            if !changes.is_empty() {
                block_data::apply_blocks(&world, &changes, block_flags());
            }
        });
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //set {} filled {placed} blocks in {:?}.",
                pattern.description(),
                started.elapsed()
            ),
        );
        let message = TextComponent::text(&format!("Set {placed} blocks to "));
        if let Some((input, state_id)) = pattern.literal_display() {
            message.add_child(mapping::display_component(input, state_id));
        } else {
            message.add_text(pattern.description());
        }
        message.add_text(".");
        sender.send_message(message);
        Ok(1)
    }
}

fn parse_set_args(raw: &str) -> Result<&str, String> {
    let mut rest = raw.trim();
    while let Some(flags) = rest.strip_prefix('-') {
        let (flag_token, after) = match flags.find(char::is_whitespace) {
            Some(index) => (&flags[..index], &flags[index..]),
            None => (flags, ""),
        };
        if flag_token.is_empty() {
            return Err("Empty set flag.".to_string());
        }
        for flag in flag_token.chars() {
            match flag {
                'n' => {}
                _ => return Err(format!("Unknown set flag '-{flag}'.")),
            }
        }
        rest = after.trim_start();
    }
    if rest.is_empty() {
        Err("Expected a block pattern.".to_string())
    } else {
        Ok(rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_args_accept_side_effect_flag() {
        assert_eq!(parse_set_args("-n stone").unwrap(), "stone");
        assert_eq!(
            parse_set_args("-nn 50%stone,50%dirt").unwrap(),
            "50%stone,50%dirt"
        );
    }

    #[test]
    fn set_args_reject_unknown_flag() {
        assert!(parse_set_args("-x stone").unwrap_err().contains("Unknown"));
    }
}
