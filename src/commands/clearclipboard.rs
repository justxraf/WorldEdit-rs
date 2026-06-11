//! `/clearclipboard` and `//clearclipboard` - clear the player's clipboard.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

use crate::clipboard;

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let clear = Command::new(&command_names("clearclipboard"), "Clear your clipboard")
        .execute(ClearClipboardCommand);
    context.register_command(clear, "worldedit.clipboard.clear");
}

struct ClearClipboardCommand;

impl pumpkin_plugin_api::commands::CommandHandler for ClearClipboardCommand {
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
        if clipboard::clear(&key) {
            sender.send_message(TextComponent::text("Clipboard cleared."));
            Ok(1)
        } else {
            sender.send_error(TextComponent::text("Your clipboard is already empty."));
            Ok(0)
        }
    }
}
