//! `//sel` — clear the current selection.
//!
//! In WorldEdit, `//sel` (and `//desel`/`//deselect`) chooses a region
//! selector, with no arguments resetting/clearing the current selection. Only
//! the cuboid selector exists here (see [`crate::selection::Region`]), so
//! `//sel` simply clears both selection points.
//!
//! TODO(FAWE parity): WorldEdit's `//sel <type>` (cuboid, extend, poly,
//! convex, ellipsoid, sphere, cyl) switches selection *shape*. Only cuboid is
//! supported, so the `<type>` argument isn't accepted.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

use crate::selection;

use super::{command_names, player_key};

pub fn register(context: &Context) {
    let sel =
        Command::new(&command_names("sel"), "Clear your current selection").execute(SelCommand);
    context.register_command(sel, "worldedit-rs:command.sel");

    let desel =
        Command::new(&command_names("desel"), "Clear your current selection").execute(SelCommand);
    context.register_command(desel, "worldedit-rs:command.sel");
}

/// Handler for `//sel` / `//desel` — clears both selection points.
struct SelCommand;

impl pumpkin_plugin_api::commands::CommandHandler for SelCommand {
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
        selection::with_selection_mut(&key, |sel| {
            sel.pos1 = None;
            sel.pos2 = None;
        });
        sender.send_message(TextComponent::text("Selection cleared."));
        Ok(1)
    }
}
