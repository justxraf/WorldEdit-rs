//! `//redo [times] [player]` - redo edit(s).
//!
//! Mirrors WorldEdit/FAWE's `HistoryCommands#redo` shape. History is still
//! stored per player key in memory, so the optional `player` argument selects
//! another key from that map.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, Number, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
    world::{BlockChange, World},
};

use crate::history::{self, EditEntry};

use super::{block_flags, command_names, player_key};

/// Upper bound on `//undo <times>` / `//redo <times>`.
const MAX_TIMES: i32 = 64;

pub fn register(context: &Context) {
    let player_after_times =
        CommandNode::argument("player", &ArgumentType::String(StringType::SingleWord))
            .execute(RedoCommand);
    let times_arg =
        CommandNode::argument("times", &ArgumentType::Integer((Some(1), Some(MAX_TIMES))))
            .execute(RedoCommand);
    times_arg.then(player_after_times);

    let player_arg = CommandNode::argument("player", &ArgumentType::String(StringType::SingleWord))
        .execute(RedoCommand);

    let redo =
        Command::new(&command_names("redo"), "Redo your last undone edit").execute(RedoCommand);
    redo.then(times_arg);
    redo.then(player_arg);
    context.register_command(redo, "worldedit.history.redo");
}

/// Handler for `//redo`, `//redo <times>`, and `//redo <times> <player>`.
struct RedoCommand;

impl pumpkin_plugin_api::commands::CommandHandler for RedoCommand {
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
        let Some(sender_key) = player_key(&sender) else {
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
                    "Redo count must be between 1 and {MAX_TIMES}."
                )));
                return Ok(0);
            }
            _ => 1,
        };
        let key = match args.get_value("player") {
            Arg::Simple(player) if !player.is_empty() => player,
            _ => sender_key,
        };

        let mut redone = 0;
        for _ in 0..times {
            let Some(entry) = history::redo(&key) else {
                break;
            };
            apply_redo(&world, &entry);
            redone += 1;
        }

        if redone == 0 {
            sender.send_error(TextComponent::text("Nothing left to redo."));
            return Ok(0);
        }

        logging::log(
            LogLevel::Info,
            &format!("WorldEdit-rs: //redo reapplied {redone} edit(s) for {key}."),
        );
        sender.send_message(TextComponent::text(&format!(
            "Redid {redone} edit(s) for {key}."
        )));
        Ok(1)
    }
}

/// Reapply every block in `entry` to its `after` state.
fn apply_redo(world: &World, entry: &EditEntry) {
    let changes: Vec<BlockChange> = entry
        .changes
        .iter()
        .map(|&(pos, _before, after)| BlockChange { pos, state: after })
        .collect();
    if !changes.is_empty() {
        world.set_block_states(&changes, block_flags());
    }
}
