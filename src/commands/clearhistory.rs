//! `//clearhistory` - clear the player's undo and redo history.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

use crate::history;

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let clear = Command::new(&command_names("clearhistory"), "Clear your edit history")
        .execute(ClearHistoryCommand);
    context.register_command(clear, "worldedit.history.clear");
}

struct ClearHistoryCommand;

impl pumpkin_plugin_api::commands::CommandHandler for ClearHistoryCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        history::clear(&key);
        sender.send_message(TextComponent::text("History cleared."));
        Ok(1)
    }
}
