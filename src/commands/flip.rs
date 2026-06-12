//! `//flip [direction]` - mirror the clipboard's future pastes across an axis.
//!
//! Mirrors WorldEdit/FAWE: this does not mutate the clipboard's stored block
//! data. It composes a single-axis mirror onto the clipboard's pending
//! [`Transform`], applied lazily by `//paste` (see [`crate::transform`]).
//! Repeated `//flip` calls accumulate (two flips on the same axis cancel
//! out). `direction` defaults to the player's facing direction; north/south
//! mirror across Z, east/west mirror across X, and up/down mirror across Y.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    text::TextComponent,
};

use crate::{clipboard, selection::Direction, transform::Transform};

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let direction_arg =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(FlipCommand);
    let command = Command::new(&command_names("flip"), "Flip the clipboard").execute(FlipCommand);
    command.then(direction_arg);
    context.register_command(command, "worldedit.clipboard.flip");
}

struct FlipCommand;

impl pumpkin_plugin_api::commands::CommandHandler for FlipCommand {
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
        if clipboard::get_with_transform(&key).is_none() {
            sender.send_error(TextComponent::text(
                "Your clipboard is empty. Use //copy first.",
            ));
            return Ok(0);
        }

        let direction = match args.get_value("direction") {
            Arg::Simple(raw) => match Direction::parse(&raw) {
                Some(direction) => direction,
                None => {
                    sender.send_error(TextComponent::text(&format!(
                        "Unknown direction '{raw}'. Use north/south/east/west/up/down."
                    )));
                    return Ok(0);
                }
            },
            _ => {
                let Some(player) = sender.as_player() else {
                    sender.send_error(TextComponent::text("A direction is required from console."));
                    return Ok(0);
                };
                Direction::from_yaw_pitch(player.get_yaw(), player.get_pitch())
            }
        };

        let (transform, axis) = match direction {
            Direction::North | Direction::South => (Transform::flip_axis_z(), "Z"),
            Direction::East | Direction::West => (Transform::flip_axis_x(), "X"),
            Direction::Up | Direction::Down => (Transform::flip_axis_y(), "Y"),
        };
        clipboard::set_transform(&key, transform);

        sender.send_message(TextComponent::text(&format!(
            "Flipped the clipboard along the {axis} axis."
        )));
        Ok(1)
    }
}
