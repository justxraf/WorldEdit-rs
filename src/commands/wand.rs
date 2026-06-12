//! `//wand` — give the player a selection wand, plus the click handlers that
//! set `//pos1`/`//pos2` from it.
//!
//! Mirrors WorldEdit's `ToolCommands#wand` and the default `WAND_ITEM`
//! (a wooden axe): left-click a block to set position 1, right-click a block
//! to set position 2. Left-click is cancelled via [`BlockBreakEvent`] (the
//! [`PlayerInteractEvent`] for `LeftClickBlock` doesn't gate breaking) so the
//! wand never breaks the targeted block. Right-click is *not* cancelled: a
//! wooden axe has no placement or right-click behaviour of its own, so the
//! interaction is already a no-op, and cancelling it triggers a Pumpkin host
//! bug where the revert `CBlockUpdate` sends the block's registry id instead
//! of its block-state id — for `grass_block` that id aliases `snowy=true`,
//! making the clicked block flash as "snowy grass" client-side.
//!
//! WorldEdit's wand item is configurable (`wand-item` in
//! `worldedit.properties`) and FAWE also supports `//wand -n` for a navigation
//! wand. Here the selection wand item is hard-coded to a wooden axe; `-n`
//! reports that the navigation wand is unavailable.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    events::{BlockBreakEvent, EventHandler, EventPriority, InteractAction, PlayerInteractEvent},
    player::{Hand, ItemStack},
    text::TextComponent,
};

use crate::selection;

use super::command_names;

/// Registry key of the item used as the selection wand.
///
/// Unlike block palette keys elsewhere in this plugin, [`ItemStack::get_registry_key`]
/// returns the *unnamespaced* key (e.g. `"wooden_axe"`, not `"minecraft:wooden_axe"`),
/// so this must match that form for [`is_wand`]'s comparison to succeed.
/// [`ItemStack::new`] accepts either form, so this also works for [`WandCommand`].
const WAND_ITEM: &str = "wooden_axe";

pub fn register(context: &Context) {
    let flags = CommandNode::argument("flags", &ArgumentType::String(StringType::Greedy))
        .execute(WandCommand);
    let wand_command =
        Command::new(&command_names("wand"), "Get the selection wand").execute(WandCommand);
    wand_command.then(flags);
    context.register_command(wand_command, "worldedit.wand");

    if let Err(e) = context.register_event_handler(WandInteractHandler, EventPriority::Normal, true)
    {
        pumpkin_plugin_api::logging::log(
            pumpkin_plugin_api::logging::LogLevel::Warn,
            &format!("WorldEdit-rs: failed to register wand interact handler: {e}"),
        );
    }

    if let Err(e) = context.register_event_handler(WandBreakHandler, EventPriority::Normal, true) {
        pumpkin_plugin_api::logging::log(
            pumpkin_plugin_api::logging::LogLevel::Warn,
            &format!("WorldEdit-rs: failed to register wand block-break handler: {e}"),
        );
    }
}

/// Handler for `//wand` — gives the player a wooden axe to use as a selection
/// tool.
struct WandCommand;

impl pumpkin_plugin_api::commands::CommandHandler for WandCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        let Some(player) = sender.as_player() else {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        };
        if let Err(message) = parse_wand_flags(&args) {
            sender.send_error(TextComponent::text(&message));
            return Ok(0);
        }

        player.set_item_in_hand(Hand::Right, Some(ItemStack::new(WAND_ITEM, 1)));
        sender.send_message(TextComponent::text(
            "Left-click a block to set position 1, right-click to set position 2.",
        ));
        Ok(1)
    }
}

fn parse_wand_flags(args: &ConsumedArgs) -> Result<(), String> {
    let raw = match args.get_value("flags") {
        Arg::Simple(raw) | Arg::Msg(raw) => raw,
        _ => return Ok(()),
    };
    for token in raw.split_whitespace() {
        let Some(flags) = token.strip_prefix('-') else {
            return Err(format!("Unexpected wand argument '{token}'."));
        };
        for flag in flags.chars() {
            match flag {
                'n' => {
                    return Err(
                        "Navigation wand support is not implemented yet; use //wand for the selection wand."
                            .to_string(),
                    );
                }
                _ => return Err(format!("Unknown wand flag '-{flag}'.")),
            }
        }
    }
    Ok(())
}

/// Returns whether `stack` is the selection wand.
fn is_wand(stack: &ItemStack) -> bool {
    stack.get_registry_key() == WAND_ITEM
}

/// Sets `//pos2` when the player right-clicks a block while holding the wand.
///
/// Left-click is handled by [`WandBreakHandler`] instead, since this event
/// doesn't gate block breaking.
struct WandInteractHandler;

impl EventHandler<PlayerInteractEvent> for WandInteractHandler {
    fn handle(
        &self,
        _server: pumpkin_plugin_api::Server,
        data: pumpkin_plugin_api::events::EventData<PlayerInteractEvent>,
    ) -> pumpkin_plugin_api::events::EventData<PlayerInteractEvent> {
        let Some(pos) = data.clicked_pos else {
            return data;
        };
        if !matches!(data.action, InteractAction::RightClickBlock) {
            return data;
        }

        let holding_wand = data
            .player
            .get_item_in_hand(Hand::Right)
            .is_some_and(|stack| is_wand(&stack));
        if !holding_wand {
            return data;
        }

        let key = data.player.get_name();
        selection::with_selection_mut(&key, |sel| sel.pos2 = Some(pos));
        data.player.send_system_message(
            TextComponent::text(&format!(
                "Position 2 set to ({}, {}, {}).",
                pos.x, pos.y, pos.z
            )),
            false,
        );

        data
    }
}

/// Sets `//pos1` and prevents the block from breaking when the player
/// left-clicks it while holding the wand.
struct WandBreakHandler;

impl EventHandler<BlockBreakEvent> for WandBreakHandler {
    fn handle(
        &self,
        _server: pumpkin_plugin_api::Server,
        mut data: pumpkin_plugin_api::events::EventData<BlockBreakEvent>,
    ) -> pumpkin_plugin_api::events::EventData<BlockBreakEvent> {
        let Some(player) = &data.player else {
            return data;
        };

        let holding_wand = player
            .get_item_in_hand(Hand::Right)
            .is_some_and(|stack| is_wand(&stack));
        if !holding_wand {
            return data;
        }

        let pos = data.block_pos;
        let key = player.get_name();
        selection::with_selection_mut(&key, |sel| sel.pos1 = Some(pos));
        player.send_system_message(
            TextComponent::text(&format!(
                "Position 1 set to ({}, {}, {}).",
                pos.x, pos.y, pos.z
            )),
            false,
        );

        data.cancelled = true;
        data
    }
}
