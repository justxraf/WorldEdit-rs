//! `//count <mask>` - count matching blocks in the current selection.
//!
//! `<mask>` accepts anything [`BlockMask::parse`] supports, including
//! comma-separated multi-block masks (`//count stone,andesite`) and `!`/
//! `#existing`/`#air`. Output is one line per distinct matched block plus a
//! total, matching WorldEdit/FAWE's `//count` format.

use std::collections::HashMap;

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::{mapping, pattern::BlockMask};

use super::{batch_size, command_names, require_selection};

pub fn register(context: &Context) {
    let mask_arg = CommandNode::argument("mask", &ArgumentType::String(StringType::SingleWord))
        .execute(CountCommand);
    let count = Command::new(&command_names("count"), "Count blocks in the selection");
    count.then(mask_arg);
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
        let raw_mask = match args.get_value("mask") {
            Arg::Simple(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block mask, got {other:?}"
                )));
                return Ok(0);
            }
        };
        let mask = match BlockMask::parse(&raw_mask) {
            Ok(mask) => mask,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let started = std::time::Instant::now();
        let mut counts: HashMap<u16, usize> = HashMap::new();
        region.for_each_batch(batch_size(), |batch| {
            for &pos in batch {
                accumulate(&mut counts, world.get_block_state_id(pos), &mask);
            }
        });

        let total: usize = counts.values().sum();
        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //count counted {total} blocks in {:?}.",
                started.elapsed()
            ),
        );

        if counts.is_empty() {
            sender.send_message(TextComponent::text("0 blocks matched."));
            return Ok(1);
        }

        let mut entries: Vec<(u16, usize)> = counts.into_iter().collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (state_id, count) in entries {
            let palette_key = mapping::palette_key_for_state_id(state_id);
            let message = TextComponent::text(&format!("{count} "));
            message.add_child(mapping::display_component(&palette_key, state_id));
            sender.send_message(message);
        }
        sender.send_message(TextComponent::text(&format!("Total: {total}")));
        Ok(1)
    }
}

/// Add one to `counts[state]` if `state` matches `mask`.
fn accumulate(counts: &mut HashMap<u16, usize>, state: u16, mask: &BlockMask) {
    if mask.matches(state) {
        *counts.entry(state).or_insert(0) += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_single_block_mask() {
        let mask = BlockMask::States(vec![1]);
        let mut counts = HashMap::new();
        for state in [1, 2, 1, 0, 1] {
            accumulate(&mut counts, state, &mask);
        }
        assert_eq!(counts.get(&1), Some(&3));
        assert_eq!(counts.get(&2), None);
    }

    #[test]
    fn accumulates_multi_block_mask_per_distinct_block() {
        let mask = BlockMask::parse("stone,dirt").unwrap();
        let mut counts = HashMap::new();
        for state in [1, 10, 1, 10, 10, 2] {
            accumulate(&mut counts, state, &mask);
        }
        assert_eq!(counts.get(&1), Some(&2));
        assert_eq!(counts.get(&10), Some(&3));
        assert_eq!(counts.get(&2), None);
        assert_eq!(counts.values().sum::<usize>(), 5);
    }
}
