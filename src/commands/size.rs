//! `//size [-c]` - show information about the current selection or clipboard.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    text::TextComponent,
};

use crate::{clipboard, selection::Region};

use super::{command_names, player_key, require_selection};

pub fn register(context: &Context) {
    let flags = CommandNode::argument("flags", &ArgumentType::String(StringType::Greedy))
        .execute(SizeCommand);
    let size = Command::new(
        &command_names("size"),
        "Show information about your selection",
    )
    .execute(SizeCommand);
    size.then(flags);
    context.register_command(size, "worldedit.selection.size");
}

struct SizeCommand;

impl pumpkin_plugin_api::commands::CommandHandler for SizeCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if wants_clipboard(&args)? {
            let Some(key) = player_key(&sender) else {
                sender.send_error(TextComponent::text("Only players can use this command."));
                return Ok(0);
            };
            let Some(buffer) = clipboard::get(&key) else {
                sender.send_error(TextComponent::text("Your clipboard is empty."));
                return Ok(0);
            };
            let Some(region) = buffer.bounds(true) else {
                sender.send_error(TextComponent::text("Your clipboard is empty."));
                return Ok(0);
            };
            send_region_size(&sender, "Clipboard", region);
            sender.send_message(TextComponent::text(&format!(
                "Origin: ({}, {}, {})",
                buffer.origin.x, buffer.origin.y, buffer.origin.z
            )));
            return Ok(1);
        }

        let Ok((_key, _world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        send_region_size(&sender, "Size", region);
        Ok(1)
    }
}

fn wants_clipboard(args: &ConsumedArgs) -> std::result::Result<bool, CommandError> {
    match args.get_value("flags") {
        Arg::Simple(raw) | Arg::Msg(raw) => parse_size_flags(&raw)
            .map_err(|message| CommandError::CommandFailed(TextComponent::text(&message))),
        _ => Ok(false),
    }
}

fn parse_size_flags(raw: &str) -> Result<bool, String> {
    let mut clipboard = false;
    for token in raw.split_whitespace() {
        let Some(rest) = token.strip_prefix('-') else {
            return Err(format!("Unexpected size argument '{token}'."));
        };
        for flag in rest.chars() {
            match flag {
                'c' => clipboard = true,
                _ => return Err(format!("Unknown size flag '-{flag}'.")),
            }
        }
    }
    Ok(clipboard)
}

fn send_region_size(sender: &CommandSender, label: &str, region: Region) {
    let dx = region.max.x - region.min.x + 1;
    let dy = region.max.y - region.min.y + 1;
    let dz = region.max.z - region.min.z + 1;

    sender.send_message(TextComponent::text(&format!(
        "{label}: {dx} x {dy} x {dz} ({} blocks).",
        region.volume()
    )));
    sender.send_message(TextComponent::text(&format!(
        "Min: ({}, {}, {})",
        region.min.x, region.min.y, region.min.z
    )));
    sender.send_message(TextComponent::text(&format!(
        "Max: ({}, {}, {})",
        region.max.x, region.max.y, region.max.z
    )));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_flags_parse_clipboard() {
        assert!(parse_size_flags("-c").unwrap());
        assert!(parse_size_flags("-cc").unwrap());
        assert!(!parse_size_flags("").unwrap());
        assert!(parse_size_flags("-x").is_err());
    }
}
