//! `//paste` - paste the player's clipboard.
//!
//! Mirrors WorldEdit/FAWE's core block clipboard behavior. By default every
//! captured cell, including air, overwrites the destination. Supported flags
//! are `-a` (skip air), `-o` (paste at the original copy origin), `-s` (select
//! the pasted region), and `-n` (select only, without writing blocks).
//! Non-block entities and biome flags are intentionally rejected; supported
//! block entities are carried with their block states.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    common::BlockPos,
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::{
    block_data::{self, BlockPlacement},
    clipboard, history,
    history::EditEntry,
    mapping, selection,
};

use super::{batch_size, block_flags, command_names, passes_gmask, player_key, sender_block_pos};

pub fn register(context: &Context) {
    let flags_arg = CommandNode::argument("flags", &ArgumentType::String(StringType::Greedy))
        .execute(PasteCommand);
    let paste_command = Command::new(
        &command_names("paste"),
        "Paste your clipboard at your position",
    )
    .execute(PasteCommand);
    paste_command.then(flags_arg);
    context.register_command(paste_command, "worldedit.clipboard.paste");
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct PasteFlags {
    skip_air: bool,
    original_origin: bool,
    select: bool,
    select_only: bool,
    ignore_structure_void: bool,
}

fn parse_flags(raw: &str) -> Result<PasteFlags, String> {
    let mut flags = PasteFlags::default();
    for token in raw.split_whitespace() {
        let Some(rest) = token.strip_prefix('-') else {
            return Err(format!("Unexpected paste argument '{token}'."));
        };
        if rest.is_empty() {
            return Err("Empty paste flag.".to_string());
        }
        for flag in rest.chars() {
            match flag {
                'a' => flags.skip_air = true,
                'o' => flags.original_origin = true,
                's' => flags.select = true,
                'n' => {
                    flags.select_only = true;
                    flags.select = true;
                }
                'v' => flags.ignore_structure_void = true,
                'x' => {
                    return Err(
                        "Paste flag '-x' needs entity removal support that is not implemented yet."
                            .to_string(),
                    );
                }
                'b' | 'e' | 'm' => {
                    return Err(format!(
                        "Paste flag '-{flag}' needs entity, biome, or mask support that is not implemented yet."
                    ));
                }
                _ => return Err(format!("Unknown paste flag '-{flag}'.")),
            }
        }
    }
    Ok(flags)
}

fn flags_from_args(args: &ConsumedArgs) -> Result<PasteFlags, String> {
    match args.get_value("flags") {
        Arg::Simple(raw) | Arg::Msg(raw) => parse_flags(&raw),
        _ => Ok(PasteFlags::default()),
    }
}

/// Handler for `//paste`.
struct PasteCommand;

impl pumpkin_plugin_api::commands::CommandHandler for PasteCommand {
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
        let Some(world) = sender.world() else {
            sender.send_error(TextComponent::text("Could not determine your world."));
            return Ok(0);
        };
        let Ok(pos) = sender_block_pos(&sender) else {
            return Ok(0);
        };

        let Some((buffer, transform)) = clipboard::get_with_transform(&key) else {
            sender.send_error(TextComponent::text(
                "Your clipboard is empty. Use //copy first.",
            ));
            return Ok(0);
        };
        let buffer = buffer.transformed(transform);
        let flags = match flags_from_args(&args) {
            Ok(flags) => flags,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let paste_origin = if flags.original_origin {
            buffer.origin
        } else {
            pos
        };
        let target_region = buffer.target_region(paste_origin, !flags.skip_air);

        if flags.select {
            if let Some(region) = target_region {
                selection::set_region(&key, region);
            } else {
                sender.send_error(TextComponent::text("Clipboard has no blocks to select."));
                return Ok(0);
            }
        }

        if flags.select_only {
            sender.send_message(TextComponent::text("Selected the clipboard paste region."));
            return Ok(1);
        }

        let started = std::time::Instant::now();
        let mut placed = 0usize;
        let mut entry = EditEntry::default();
        let structure_void = flags
            .ignore_structure_void
            .then(|| mapping::resolve_block("minecraft:structure_void"))
            .flatten();
        for batch in buffer.blocks.chunks(batch_size()) {
            let mut changes: Vec<(BlockPos, BlockPlacement)> = Vec::with_capacity(batch.len());
            for &(offset, state) in batch {
                if flags.skip_air && state == 0 {
                    continue;
                }
                if structure_void == Some(state) {
                    continue;
                }
                let target = BlockPos {
                    x: paste_origin.x + offset.0,
                    y: paste_origin.y + offset.1,
                    z: paste_origin.z + offset.2,
                };
                let before = block_data::capture_block(&world, target);
                if !passes_gmask(&key, before.state_id) {
                    continue;
                }
                let placement = BlockPlacement {
                    state_id: state,
                    block_entity: buffer.block_entity_at(offset).cloned(),
                };
                if before == placement {
                    continue;
                }
                entry.push_change(target, before, placement.clone());
                changes.push((target, placement));
            }
            placed += changes.len();
            if !changes.is_empty() {
                block_data::apply_blocks(&world, &changes, block_flags());
            }
        }
        history::push(&key, entry);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //paste placed {placed} blocks in {:?}.",
                started.elapsed()
            ),
        );
        sender.send_message(TextComponent::text(&format!("Pasted {placed} blocks.")));
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_paste_flags_accepts_grouped_flags() {
        assert_eq!(
            parse_flags("-aso").unwrap(),
            PasteFlags {
                skip_air: true,
                original_origin: true,
                select: true,
                select_only: false,
                ignore_structure_void: false,
            }
        );
    }

    #[test]
    fn parse_paste_flags_select_only_implies_select() {
        let flags = parse_flags("-n").unwrap();
        assert!(flags.select_only);
        assert!(flags.select);
    }

    #[test]
    fn parse_paste_flags_rejects_unsupported_entity_biome_flags() {
        assert!(parse_flags("-e").unwrap_err().contains("not implemented"));
        assert!(parse_flags("-x").unwrap_err().contains("entity removal"));
        assert!(parse_flags("stone").is_err());
    }

    #[test]
    fn parse_paste_flags_accepts_structure_void_skip() {
        assert!(parse_flags("-v").unwrap().ignore_structure_void);
    }
}
