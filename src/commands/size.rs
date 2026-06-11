//! `//size` — show information about the current selection.
//!
//! Mirrors WorldEdit's `SelectionCommands#size`: reports the selection's
//! dimensions, volume, and corner coordinates.
//!
//! TODO(FAWE parity): WorldEdit's `//size -c` reports the clipboard's
//! dimensions and offset instead of the selection. Not implemented — `//size`
//! here always reports the current `//pos1`/`//pos2` selection.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandSender, ConsumedArgs},
    text::TextComponent,
};

use super::{command_names, require_selection};

pub fn register(context: &Context) {
    let size = Command::new(
        &command_names("size"),
        "Show information about your selection",
    )
    .execute(SizeCommand);
    context.register_command(size, "worldedit-rs:command.size");
}

/// Handler for `//size` — reports the selection's dimensions, volume, and corners.
struct SizeCommand;

impl pumpkin_plugin_api::commands::CommandHandler for SizeCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((_key, _world, region)) = require_selection(&sender) else {
            return Ok(0);
        };

        let dx = region.max.x - region.min.x + 1;
        let dy = region.max.y - region.min.y + 1;
        let dz = region.max.z - region.min.z + 1;

        sender.send_message(TextComponent::text(&format!(
            "Size: {dx} x {dy} x {dz} ({} blocks).",
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
        Ok(1)
    }
}
