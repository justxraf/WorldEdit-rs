//! `//sel [-d] [selector]` - clear or choose the current selection type.
//!
//! FAWE supports many selector shapes. WorldEdit-rs currently stores cuboids
//! only, so no-arg `//sel`/`//desel`/`//deselect` clears the selection and
//! `//sel cuboid` acknowledges the available selector. Other selector names
//! return a clear unsupported message.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    text::TextComponent,
};

use crate::selection;

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let selector = CommandNode::argument("selector", &ArgumentType::String(StringType::Greedy))
        .execute(SelCommand);

    let sel = Command::new(&command_names("sel"), "Clear or change your selection type")
        .execute(SelCommand);
    sel.then(selector);
    context.register_command(sel, "worldedit.analysis.sel");

    let desel =
        Command::new(&command_names("desel"), "Clear your current selection").execute(SelCommand);
    context.register_command(desel, "worldedit.analysis.sel");

    let deselect = Command::new(&command_names("deselect"), "Clear your current selection")
        .execute(SelCommand);
    context.register_command(deselect, "worldedit.analysis.sel");
}

struct SelCommand;

impl pumpkin_plugin_api::commands::CommandHandler for SelCommand {
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

        let raw = match args.get_value("selector") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            _ => String::new(),
        };
        let request = match parse_selector_request(&raw) {
            Ok(request) => request,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        match request {
            SelectorRequest::Clear => {
                selection::with_selection_mut(&key, |sel| {
                    sel.pos1 = None;
                    sel.pos2 = None;
                });
                sender.send_message(TextComponent::text("Selection cleared."));
            }
            SelectorRequest::Cuboid { default } => {
                if default {
                    sender.send_message(TextComponent::text(
                        "Default selection type set to cuboid for this session.",
                    ));
                } else {
                    sender.send_message(TextComponent::text("Selection type set to cuboid."));
                }
            }
        }
        Ok(1)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SelectorRequest {
    Clear,
    Cuboid { default: bool },
}

fn parse_selector_request(raw: &str) -> Result<SelectorRequest, String> {
    let mut default = false;
    let mut selector = None;

    for token in raw.split_whitespace() {
        if token == "-d" {
            default = true;
        } else if selector.replace(token).is_some() {
            return Err("Usage: //sel [-d] [cuboid].".to_string());
        }
    }

    let Some(selector) = selector else {
        return if default {
            Err("Usage: //sel [-d] [cuboid].".to_string())
        } else {
            Ok(SelectorRequest::Clear)
        };
    };

    match selector.to_ascii_lowercase().as_str() {
        "cuboid" | "cube" => Ok(SelectorRequest::Cuboid { default }),
        other => Err(format!(
            "Selection type '{other}' is not supported yet; only cuboid is available."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_selector_clears() {
        assert_eq!(parse_selector_request("").unwrap(), SelectorRequest::Clear);
    }

    #[test]
    fn cuboid_selector_is_accepted() {
        assert_eq!(
            parse_selector_request("-d cuboid").unwrap(),
            SelectorRequest::Cuboid { default: true }
        );
    }

    #[test]
    fn unsupported_selector_is_rejected() {
        assert!(
            parse_selector_request("poly")
                .unwrap_err()
                .contains("only cuboid")
        );
    }
}
