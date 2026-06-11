//! `//set <block>` — fill the current selection with one block.
//!
//! Mirrors WorldEdit's `RegionCommands#set`, minus pattern support: the
//! argument is a single block name or numeric global state id resolved via
//! [`crate::mapping::resolve_block`], not a full WorldEdit pattern.
//!
//! TODO(FAWE parity): `//set` in WorldEdit takes a `Pattern`, which can be a
//! single block, a weighted random mix (`50%stone,50%dirt`), a `#clipboard`
//! / `#existing` pattern, etc. Only a single literal block name or id is
//! supported here.

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
    let block_arg = CommandNode::argument("block", &ArgumentType::String(StringType::SingleWord))
        .execute(SetCommand);
    let set_command = Command::new(&command_names("set"), "Fill the selection with a block");
    set_command.then(block_arg);
    context.register_command(set_command, "worldedit-rs:command.set");
}

/// Handler for `//set <block>` — fills the current selection with one block.
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
        let mut placed = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes: Vec<BlockChange> = Vec::with_capacity(batch.len());
            for &pos in batch {
                let before = world.get_block_state_id(pos);
                if before == state_id {
                    continue;
                }
                entry.changes.push((pos, before, state_id));
                changes.push(BlockChange {
                    pos,
                    state: state_id,
                });
            }
            placed += changes.len();
            if !changes.is_empty() {
                world.set_block_states(&changes, block_flags());
            }
        });
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //set {block} filled {placed} blocks in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Set {placed} blocks to '{block}'."
        )));
        Ok(1)
    }
}
