//! Cuboid shell commands: `//walls`, `//faces`, and `//outline`.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
    world::BlockChange,
};

use crate::{
    history::{self, EditEntry},
    mapping,
    pattern::BlockPattern,
    selection::Region,
};

use super::{batch_size, block_flags, command_names, require_selection};

pub fn register(context: &Context) {
    register_one(
        context,
        "walls",
        "Build the walls of the selection",
        ShellKind::Walls,
        "worldedit.region.walls",
    );
    register_one(
        context,
        "faces",
        "Build the outer shell of the selection",
        ShellKind::Faces,
        "worldedit.region.faces",
    );
    register_one(
        context,
        "outline",
        "Build the outer shell of the selection",
        ShellKind::Faces,
        "worldedit.region.faces",
    );
}

fn register_one(
    context: &Context,
    name: &str,
    description: &str,
    kind: ShellKind,
    permission: &str,
) {
    let block = CommandNode::argument("block", &ArgumentType::String(StringType::Greedy))
        .execute(ShellCommand { kind });
    let command = Command::new(&command_names(name), description);
    command.then(block);
    context.register_command(command, permission);
}

#[derive(Clone, Copy)]
struct ShellCommand {
    kind: ShellKind,
}

impl pumpkin_plugin_api::commands::CommandHandler for ShellCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        let raw_pattern = match args.get_value("block") {
            Arg::Simple(s) | Arg::Msg(s) => s,
            other => {
                sender.send_error(TextComponent::text(&format!(
                    "Expected a block pattern, got {other:?}"
                )));
                return Ok(0);
            }
        };
        let pattern = match BlockPattern::parse(&raw_pattern) {
            Ok(pattern) => pattern,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let started = std::time::Instant::now();
        let mut placed = 0usize;
        let mut entry = EditEntry::default();
        region.for_each_batch(batch_size(), |batch| {
            let mut changes = Vec::with_capacity(batch.len());
            for &pos in batch {
                if !self.kind.includes(region, pos.x, pos.y, pos.z) {
                    continue;
                }
                let before = world.get_block_state_id(pos);
                let state_id = pattern.state_at(pos, before);
                if before == state_id {
                    continue;
                }
                entry.changes.push((pos, before, state_id));
                changes.push(BlockChange {
                    pos,
                    state: state_id,
                });
            }
            placed += changes.len();
            if !changes.is_empty() {
                world.set_block_states(&changes, block_flags());
            }
        });
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: shell command placed {placed} blocks in {:?}.",
                started.elapsed()
            ),
        );
        let message = TextComponent::text(&format!("{placed} block(s) set to "));
        if let Some((input, state_id)) = pattern.literal_display() {
            message.add_child(mapping::display_component(input, state_id));
        } else {
            message.add_text(pattern.description());
        }
        message.add_text(".");
        sender.send_message(message);
        Ok(1)
    }
}

#[derive(Clone, Copy)]
enum ShellKind {
    Walls,
    Faces,
}

impl ShellKind {
    fn includes(self, region: Region, x: i32, y: i32, z: i32) -> bool {
        let side = x == region.min.x || x == region.max.x || z == region.min.z || z == region.max.z;
        match self {
            Self::Walls => side,
            Self::Faces => side || y == region.min.y || y == region.max.y,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_plugin_api::common::BlockPos;

    fn region() -> Region {
        Region::new(BlockPos { x: 0, y: 0, z: 0 }, BlockPos { x: 2, y: 2, z: 2 })
    }

    #[test]
    fn walls_include_vertical_sides_but_not_floor_center() {
        let r = region();
        assert!(ShellKind::Walls.includes(r, 0, 1, 1));
        assert!(ShellKind::Walls.includes(r, 1, 1, 2));
        assert!(!ShellKind::Walls.includes(r, 1, 0, 1));
        assert!(!ShellKind::Walls.includes(r, 1, 1, 1));
    }

    #[test]
    fn faces_include_floor_ceiling_and_sides() {
        let r = region();
        assert!(ShellKind::Faces.includes(r, 1, 0, 1));
        assert!(ShellKind::Faces.includes(r, 1, 2, 1));
        assert!(ShellKind::Faces.includes(r, 2, 1, 1));
        assert!(!ShellKind::Faces.includes(r, 1, 1, 1));
    }
}
