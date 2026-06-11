//! `//copy [-c] [-m <mask>]` - copy the current selection into the clipboard.
//!
//! Mirrors the block-state portion of FAWE's `ClipboardCommands#copy`: the
//! clipboard origin is the player's current placement position by default, `-c`
//! anchors it to the horizontal center of the selection at the minimum Y level,
//! and `-m <mask>` copies only matching literal block states while turning
//! excluded cells into air. Entity and biome flags are rejected because this
//! clipboard stores block states only.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    common::BlockPos,
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::{clipboard, pattern::BlockMask, selection::Region};

use super::{command_names, require_selection, sender_block_pos};

pub fn register(context: &Context) {
    let flags_arg = CommandNode::argument("flags", &ArgumentType::String(StringType::Greedy))
        .execute(CopyCommand);
    let copy_command = Command::new(
        &command_names("copy"),
        "Copy the selection to your clipboard",
    )
    .execute(CopyCommand);
    copy_command.then(flags_arg);
    context.register_command(copy_command, "worldedit.clipboard.copy");
}

#[derive(Default, Debug, PartialEq, Eq)]
struct CopyOptions {
    center_origin: bool,
    mask: Option<BlockMask>,
}

fn parse_options(raw: &str) -> Result<CopyOptions, String> {
    let mut options = CopyOptions::default();
    let mut tokens = raw.split_whitespace();

    while let Some(token) = tokens.next() {
        let Some(flags) = token.strip_prefix('-') else {
            return Err(format!("Unexpected copy argument '{token}'."));
        };
        if flags.is_empty() {
            return Err("Empty copy flag.".to_string());
        }

        for flag in flags.chars() {
            match flag {
                'c' => options.center_origin = true,
                'm' => {
                    let Some(mask) = tokens.next() else {
                        return Err("Copy flag '-m' requires a mask.".to_string());
                    };
                    options.mask = Some(BlockMask::parse(mask)?);
                }
                'e' => {
                    return Err(
                        "Copy flag '-e' needs entity clipboard support that is not implemented yet."
                            .to_string(),
                    );
                }
                'b' => {
                    return Err(
                        "Copy flag '-b' needs biome clipboard support that is not implemented yet."
                            .to_string(),
                    );
                }
                _ => return Err(format!("Unknown copy flag '-{flag}'.")),
            }
        }
    }

    Ok(options)
}

fn options_from_args(args: &ConsumedArgs) -> Result<CopyOptions, String> {
    match args.get_value("flags") {
        Arg::Simple(raw) | Arg::Msg(raw) => parse_options(&raw),
        _ => Ok(CopyOptions::default()),
    }
}

/// Handler for `//copy` - copies the current selection into the player's clipboard.
struct CopyCommand;

impl pumpkin_plugin_api::commands::CommandHandler for CopyCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Ok((key, world, region)) = require_selection(&sender) else {
            return Ok(0);
        };
        let options = match options_from_args(&args) {
            Ok(options) => options,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let origin = if options.center_origin {
            center_origin(region)
        } else {
            let Ok(origin) = sender_block_pos(&sender) else {
                return Ok(0);
            };
            origin
        };

        let started = std::time::Instant::now();
        let buffer = if let Some(mask) = options.mask.as_ref() {
            clipboard::capture_filtered(&world, &region, origin, |state| mask.matches(state))
        } else {
            clipboard::capture(&world, &region, origin)
        };
        let blocks = buffer.blocks.len();
        clipboard::set(&key, buffer);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //copy captured {blocks} blocks in {:?}.",
                started.elapsed()
            ),
        );
        let origin_note = if options.center_origin {
            "centered on the selection"
        } else {
            "relative to your position"
        };
        sender.send_message(TextComponent::text(&format!(
            "Copied {blocks} blocks to your clipboard ({origin_note})."
        )));
        Ok(1)
    }
}

fn center_origin(region: Region) -> BlockPos {
    BlockPos {
        x: region.min.x + (region.max.x - region.min.x) / 2,
        y: region.min.y,
        z: region.min.z + (region.max.z - region.min.z) / 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_plugin_api::common::BlockPos;

    #[test]
    fn parse_copy_options_accepts_center_and_mask() {
        let options = parse_options("-c -m stone").unwrap();
        assert!(options.center_origin);
        assert!(options.mask.unwrap().matches(1));
    }

    #[test]
    fn parse_copy_options_rejects_entity_and_biome_flags() {
        assert!(parse_options("-e").unwrap_err().contains("entity"));
        assert!(parse_options("-b").unwrap_err().contains("biome"));
    }

    #[test]
    fn center_origin_uses_floor_center_and_min_y() {
        let region = Region::new(
            BlockPos { x: -2, y: 5, z: 0 },
            BlockPos { x: -1, y: 9, z: 3 },
        );
        let origin = center_origin(region);
        assert_eq!((origin.x, origin.y, origin.z), (-2, 5, 1));
    }
}
