//! `//rotate <y> [<x> [<z>]]` - rotate the clipboard's future pastes.
//!
//! Mirrors WorldEdit/FAWE: this does not mutate the clipboard's stored block
//! data. It composes a rotation onto the clipboard's pending [`Transform`],
//! applied lazily by `//paste` (see [`crate::transform`]). Repeated
//! `//rotate` calls accumulate. Angles must be multiples of 90 degrees;
//! positive Y angles rotate clockwise (e.g. `facing=north` -> `facing=east`).

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, Number},
    text::TextComponent,
};

use crate::{clipboard, transform::Transform};

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let z_arg =
        CommandNode::argument("z", &ArgumentType::Integer((None, None))).execute(RotateCommand);
    let x_arg =
        CommandNode::argument("x", &ArgumentType::Integer((None, None))).execute(RotateCommand);
    x_arg.then(z_arg);
    let y_arg =
        CommandNode::argument("y", &ArgumentType::Integer((None, None))).execute(RotateCommand);
    y_arg.then(x_arg);

    let command = Command::new(&command_names("rotate"), "Rotate the clipboard").execute(Usage);
    command.then(y_arg);
    context.register_command(command, "worldedit.clipboard.rotate");
}

struct Usage;

impl pumpkin_plugin_api::commands::CommandHandler for Usage {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        sender.send_error(TextComponent::text(
            "Usage: //rotate <y> [<x> [<z>]] (degrees, multiples of 90).",
        ));
        Ok(0)
    }
}

struct RotateCommand;

impl pumpkin_plugin_api::commands::CommandHandler for RotateCommand {
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

        let Some(y) = int_arg(&args, "y") else {
            sender.send_error(TextComponent::text(
                "Expected a Y-axis rotation in degrees.",
            ));
            return Ok(0);
        };
        let x = int_arg(&args, "x").unwrap_or(0);
        let z = int_arg(&args, "z").unwrap_or(0);

        let (Some(rot_y), Some(rot_x), Some(rot_z)) = (
            Transform::rotate_y(y),
            Transform::rotate_x(x),
            Transform::rotate_z(z),
        ) else {
            sender.send_error(TextComponent::text(
                "Rotation angles must be multiples of 90 degrees.",
            ));
            return Ok(0);
        };

        let transform = rot_y.combine(rot_x).combine(rot_z);
        clipboard::set_transform(&key, transform);

        sender.send_message(TextComponent::text(&format!(
            "Rotated the clipboard by ({y}, {x}, {z})."
        )));
        Ok(1)
    }
}

fn int_arg(args: &ConsumedArgs, name: &str) -> Option<i32> {
    match args.get_value(name) {
        Arg::Num(Ok(Number::Int32(n))) => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_steps_are_multiples_of_90() {
        assert!(Transform::rotate_y(90).is_some());
        assert!(Transform::rotate_y(45).is_none());
    }
}
