//! `//cut [leave]` - copy the current selection, then replace it.
//!
//! Mirrors WorldEdit/FAWE's block-only `ClipboardCommands#cut`: the optional
//! leave pattern defaults to air and accepts this plugin's supported pattern
//! subset.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{
    clipboard, history,
    history::EditEntry,
    pattern::{BlockPattern, PatternEvalContext},
};

use super::{batch_size, block_flags, command_names, require_selection, sender_block_pos};

const AIR_STATE_ID: u16 = 0;

pub fn register(context: &Context) {
    let leave_arg = CommandNode::argument("leave", &ArgumentType::String(StringType::Greedy))
        .execute(CutCommand);
    let cut_command = Command::new(&command_names("cut"), "Cut the selection to your clipboard")
        .execute(CutCommand);
    cut_command.then(leave_arg);
    context.register_command(cut_command, "worldedit.clipboard.cut");
}

struct CutCommand;

impl pumpkin_plugin_api::commands::CommandHandler for CutCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        let Ok(origin) = sender_block_pos(&sender) else {
            return Ok(0);
        };
        let leave_pattern = match args.get_value("leave") {
            Arg::Simple(block) | Arg::Msg(block) => match BlockPattern::parse(&block) {
                Ok(pattern) => pattern,
                Err(message) => {
                    sender.send_error(TextComponent::text(&message));
                    return Ok(0);
                }
            },
            _ => BlockPattern::Literal {
                input: "minecraft:air".to_string(),
                state_id: AIR_STATE_ID,
            },
        };
        let leave_name = leave_pattern.description().to_string();

        let started = std::time::Instant::now();
        let buffer = clipboard::capture(&world, &region, origin);
        let copied = buffer.blocks.len();
        clipboard::set(&key, buffer);
        let pattern_ctx = PatternEvalContext::for_player(region.min, &key);
        if let Err(message) = leave_pattern.validate(&pattern_ctx) {
            sender.send_error(TextComponent::text(&message));
            return Ok(0);
        }

        let mut cleared = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = world.get_block_state_id(pos);
                let leave_state = leave_pattern.state_at_with(pos, before, &pattern_ctx);
                if before == leave_state {
                    continue;
                }
                entry.changes.push((pos, before, leave_state));
                changes.push(BlockChange {
                    pos,
                    state: leave_state,
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
                "WorldEdit-rs: //cut copied {copied} blocks and replaced {cleared} in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Cut {copied} blocks to your clipboard ({cleared} set to {leave_name})."
        )));
        Ok(1)
    }
}
