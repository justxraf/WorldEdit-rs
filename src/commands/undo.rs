//! `//undo [times]` — undo the player's last edit(s).
//!
//! Mirrors WorldEdit's `HistoryCommands#undo`.
//!
//! TODO(FAWE parity): WorldEdit's `//undo [times] [player]` accepts an
//! optional `player` argument so operators can undo another player's edits.
//! Not implemented — only the invoking player's own history is addressable.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, Number},
    logging::{self, LogLevel},
    text::TextComponent,
    world::{BlockChange, World},
};

use crate::history::{self, EditEntry};

use super::{block_flags, command_names, player_key};

/// Upper bound on `//undo <times>` / `//redo <times>` so a typo can't request
/// an absurd number of pops (each is still capped by the history stack size
/// in `crate::history`, but this keeps the argument itself sane).
const MAX_TIMES: i32 = 64;

pub fn register(context: &Context) {
    // `//undo <times>` — undo `times` edits.
    let times_arg =
        CommandNode::argument("times", &ArgumentType::Integer((Some(1), Some(MAX_TIMES))))
            .execute(UndoCommand);

    // Bare `//undo` — undo once. Both forms share one command tree, since
    // registering two `Command`s under the same name would have the second
    // overwrite the first in the dispatcher.
    let undo = Command::new(&command_names("undo"), "Undo your last edit").execute(UndoCommand);
    undo.then(times_arg);
    context.register_command(undo, "worldedit-rs:command.undo");
}

/// Handler for `//undo` and `//undo <times>`.
struct UndoCommand;

impl pumpkin_plugin_api::commands::CommandHandler for UndoCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
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

        let times = match args.get_value("times") {
            Arg::Num(Ok(Number::Int32(n))) => n,
            Arg::Num(Err(_)) => {
                sender.send_error(TextComponent::text(&format!(
                    "Undo count must be between 1 and {MAX_TIMES}."
                )));
                return Ok(0);
            }
            // No `times` argument was provided — the bare `//undo` form.
            _ => 1,
        };

        let mut undone = 0;
        for _ in 0..times {
            let Some(entry) = history::undo(&key) else {
                break;
            };
            apply_undo(&world, &entry);
            undone += 1;
        }

        if undone == 0 {
            sender.send_error(TextComponent::text("Nothing left to undo."));
            return Ok(0);
        }

        logging::log(
            LogLevel::Info,
            &format!("WorldEdit-rs: //undo reverted {undone} edit(s) for {key}."),
        );
        sender.send_message(TextComponent::text(&format!("Undid {undone} edit(s).")));
        Ok(1)
    }
}

/// Restore every block in `entry` to its `before` state.
pub(super) fn apply_undo(world: &World, entry: &EditEntry) {
    let changes: Vec<BlockChange> = entry
        .changes
        .iter()
        .map(|&(pos, before, _after)| BlockChange { pos, state: before })
        .collect();
    if !changes.is_empty() {
        world.set_block_states(&changes, block_flags());
    }
}
