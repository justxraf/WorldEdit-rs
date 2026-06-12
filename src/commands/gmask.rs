//! `//gmask [mask]` - set or clear the player's global mask.
//!
//! Mirrors WorldEdit/FAWE's `GeneralCommands#gmask`: once set, the mask is
//! layered on top of every edit command (`//set`, `//replace`, `//cut`'s
//! leave-fill, `//paste`, shell commands, and brushes) via
//! [`super::passes_gmask`]. Running `//gmask` with no argument clears it.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    text::TextComponent,
};

use crate::{mask, pattern::BlockMask};

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let mask_arg = CommandNode::argument("mask", &ArgumentType::String(StringType::Greedy))
        .execute(GmaskCommand);
    let gmask_command = Command::new(
        &command_names("gmask"),
        "Set or clear your global mask, applied to every edit",
    )
    .execute(GmaskCommand);
    gmask_command.then(mask_arg);
    context.register_command(gmask_command, "worldedit.global-mask");
}

struct GmaskCommand;

impl pumpkin_plugin_api::commands::CommandHandler for GmaskCommand {
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
            sender.send_error(TextComponent::text("Could not determine your identity."));
            return Ok(0);
        };

        let raw = match args.get_value("mask") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => String::new(),
        };

        if raw.trim().is_empty() {
            mask::clear(&key);
            sender.send_message(TextComponent::text("Global mask cleared."));
            return Ok(1);
        }

        let parsed = match BlockMask::parse(&raw) {
            Ok(mask) => mask,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };
        mask::set(&key, parsed);
        sender.send_message(TextComponent::text("Global mask set."));
        Ok(1)
    }
}
