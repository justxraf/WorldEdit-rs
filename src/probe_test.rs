use pumpkin_plugin_api::events::{
    EventHandler, EventPriority, InteractAction, PlayerInteractEvent,
};
use pumpkin_plugin_api::{
    Server,
    common::BlockPos,
    player::{Hand, ItemStack, Player},
    text::TextComponent,
};

#[allow(dead_code)]
struct Probe;

impl EventHandler<PlayerInteractEvent> for Probe {
    fn handle(
        &self,
        _server: Server,
        mut data: pumpkin_plugin_api::events::EventData<PlayerInteractEvent>,
    ) -> pumpkin_plugin_api::events::EventData<PlayerInteractEvent> {
        match data.action {
            InteractAction::LeftClickBlock => {}
            InteractAction::RightClickBlock => {}
            InteractAction::LeftClickAir => {}
            InteractAction::RightClickAir => {}
        }
        let _pos: Option<BlockPos> = data.clicked_pos;
        let _block: &String = &data.block;
        let player: &Player = &data.player;
        let stack: Option<ItemStack> = player.get_item_in_hand(Hand::Main);
        let _ = stack;
        player.send_message(&TextComponent::text("hi"));
        data.cancelled = true;
        data
    }
}
