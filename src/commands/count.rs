//! `//count <block>` - count matching blocks in the current selection.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::mapping;

use super::{batch_size, command_names, require_selection};

pub fn register(context: &Context) {
    let block = CommandNode::argument("block", &ArgumentType::String(StringType::SingleWord))
        .execute(CountCommand);
    let count = Command::new(&command_names("count"), "Count blocks in the selection");
    count.then(block);
    context.register_command(count, "worldedit.analysis.count");
}

struct CountCommand;

impl pumpkin_plugin_api::commands::CommandHandler for CountCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((_key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        let block = match args.get_value("block") {
            Arg::Simple(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block name, got {other:?}"
                )));
                return Ok(0);
            }
        };
        let Some(state_id) = mapping::resolve_block(&block) else {
            sender.send_error(TextComponent::text(&format!("Unknown block '{block}'.")));
            return Ok(0);
        };

        let started = std::time::Instant::now();
        let mut count = 0usize;
        region.for_each_batch(batch_size(), |batch| {
            for &pos in batch {
                if matches_count(world.get_block_state_id(pos), state_id) {
                    count += 1;
                }
            }
        });

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //count counted {count} blocks in {:?}.",
                started.elapsed()
            ),
        );
        let message = TextComponent::text(&format!("{count} block(s) matched "));
        message.add_child(mapping::display_component(&block, state_id));
        message.add_text(".");
        sender.send_message(message);
        Ok(1)
    }
}

fn matches_count(state: u16, wanted: u16) -> bool {
    state == wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_count_is_exact_state_match() {
        assert!(matches_count(1, 1));
        assert!(!matches_count(1, 2));
    }
}
