use std::array;

use pumpkin_plugin_api::{
    block_entity::{BlockEntityType, DyeColor, SignText},
    common::BlockPos,
    data_components::DataComponent,
    player::ItemStack,
    world::{BlockChange, BlockFlags, World},
};

use crate::mapping;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockPlacement {
    pub state_id: u16,
    pub block_entity: Option<BlockEntityData>,
}

impl BlockPlacement {
    pub fn new(state_id: u16) -> Self {
        Self {
            state_id,
            block_entity: None,
        }
    }
}

impl Default for BlockPlacement {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockEntityData {
    Sign(SignBlockData),
    Chest(ChestBlockData),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChestBlockData {
    pub items: Vec<Option<ItemStackData>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemStackData {
    pub registry_key: String,
    pub count: u8,
    pub components: Vec<ItemComponentData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemComponentData {
    pub component: DataComponent,
    pub value: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignBlockData {
    pub front: SignFace,
    pub back: SignFace,
    pub waxed: bool,
}

impl Default for SignBlockData {
    fn default() -> Self {
        Self {
            front: SignFace::default(),
            back: SignFace::default(),
            waxed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignFace {
    pub messages: [String; 4],
    pub color: SignColor,
    pub glowing: bool,
}

impl SignFace {
    pub fn from_lines(lines: &[String]) -> Self {
        let mut face = Self::default();
        for (index, line) in lines.iter().take(4).enumerate() {
            face.messages[index] = line.clone();
        }
        face
    }
}

impl Default for SignFace {
    fn default() -> Self {
        Self {
            messages: array::from_fn(|_| String::new()),
            color: SignColor::White,
            glowing: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignColor {
    White,
    Orange,
    Magenta,
    LightBlue,
    Yellow,
    Lime,
    Pink,
    Gray,
    LightGray,
    Cyan,
    Purple,
    Blue,
    Brown,
    Green,
    Red,
    Black,
}

impl SignColor {
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "white" => Some(Self::White),
            "orange" => Some(Self::Orange),
            "magenta" => Some(Self::Magenta),
            "light_blue" | "light-blue" | "lightblue" => Some(Self::LightBlue),
            "yellow" => Some(Self::Yellow),
            "lime" => Some(Self::Lime),
            "pink" => Some(Self::Pink),
            "gray" | "grey" => Some(Self::Gray),
            "light_gray" | "light-grey" | "lightgray" | "lightgrey" => Some(Self::LightGray),
            "cyan" => Some(Self::Cyan),
            "purple" => Some(Self::Purple),
            "blue" => Some(Self::Blue),
            "brown" => Some(Self::Brown),
            "green" => Some(Self::Green),
            "red" => Some(Self::Red),
            "black" => Some(Self::Black),
            _ => None,
        }
    }
}

impl From<DyeColor> for SignColor {
    fn from(value: DyeColor) -> Self {
        match value {
            DyeColor::White => Self::White,
            DyeColor::Orange => Self::Orange,
            DyeColor::Magenta => Self::Magenta,
            DyeColor::LightBlue => Self::LightBlue,
            DyeColor::Yellow => Self::Yellow,
            DyeColor::Lime => Self::Lime,
            DyeColor::Pink => Self::Pink,
            DyeColor::Gray => Self::Gray,
            DyeColor::LightGray => Self::LightGray,
            DyeColor::Cyan => Self::Cyan,
            DyeColor::Purple => Self::Purple,
            DyeColor::Blue => Self::Blue,
            DyeColor::Brown => Self::Brown,
            DyeColor::Green => Self::Green,
            DyeColor::Red => Self::Red,
            DyeColor::Black => Self::Black,
        }
    }
}

impl From<SignColor> for DyeColor {
    fn from(value: SignColor) -> Self {
        match value {
            SignColor::White => Self::White,
            SignColor::Orange => Self::Orange,
            SignColor::Magenta => Self::Magenta,
            SignColor::LightBlue => Self::LightBlue,
            SignColor::Yellow => Self::Yellow,
            SignColor::Lime => Self::Lime,
            SignColor::Pink => Self::Pink,
            SignColor::Gray => Self::Gray,
            SignColor::LightGray => Self::LightGray,
            SignColor::Cyan => Self::Cyan,
            SignColor::Purple => Self::Purple,
            SignColor::Blue => Self::Blue,
            SignColor::Brown => Self::Brown,
            SignColor::Green => Self::Green,
            SignColor::Red => Self::Red,
            SignColor::Black => Self::Black,
        }
    }
}

pub fn capture_block_with_state(world: &World, pos: BlockPos, state_id: u16) -> BlockPlacement {
    let block_entity = if mapping::state_has_block_entity(state_id) {
        match world.get_block_entity(pos) {
            Some(BlockEntityType::SignBlockEntity(sign)) => {
                Some(BlockEntityData::Sign(SignBlockData {
                    front: face_from_text(sign.get_front_text()),
                    back: face_from_text(sign.get_back_text()),
                    waxed: sign.is_waxed(),
                }))
            }
            Some(BlockEntityType::ChestBlockEntity(chest)) => {
                let items = (0..chest.size())
                    .map(|slot| {
                        chest.get_item(slot).map(|item| ItemStackData {
                            registry_key: item.get_registry_key(),
                            count: item.get_count(),
                            components: item
                                .get_components()
                                .into_iter()
                                .map(|component| ItemComponentData {
                                    component: component.component,
                                    value: component.value,
                                })
                                .collect(),
                        })
                    })
                    .collect();
                Some(BlockEntityData::Chest(ChestBlockData { items }))
            }
            _ => None,
        }
    } else {
        None
    };
    BlockPlacement {
        state_id,
        block_entity,
    }
}

pub fn apply_block(world: &World, pos: BlockPos, placement: &BlockPlacement, flags: BlockFlags) {
    let flags = flags_for_state(placement.state_id, flags);
    world.set_block_state(pos, placement.state_id, flags);
    apply_block_entity(world, pos, placement);
}

fn flags_for_state(state_id: u16, flags: BlockFlags) -> BlockFlags {
    if mapping::state_has_block_entity(state_id) {
        flags
    } else {
        flags | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
    }
}

pub fn apply_blocks(world: &World, changes: &[(BlockPos, BlockPlacement)], flags: BlockFlags) {
    let mut regular_states = Vec::with_capacity(changes.len());
    let mut block_entity_states = Vec::new();
    for (pos, placement) in changes {
        let change = BlockChange {
            pos: *pos,
            state: placement.state_id,
        };
        if mapping::state_has_block_entity(placement.state_id) {
            block_entity_states.push(change);
        } else {
            regular_states.push(change);
        }
    }

    if !regular_states.is_empty() {
        world.set_block_states(
            &regular_states,
            flags | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK,
        );
    }
    if !block_entity_states.is_empty() {
        world.set_block_states(&block_entity_states, flags);
    }

    for (pos, placement) in changes {
        apply_block_entity(world, *pos, placement);
    }
}

fn apply_block_entity(world: &World, pos: BlockPos, placement: &BlockPlacement) {
    match &placement.block_entity {
        Some(BlockEntityData::Sign(data)) => {
            if let Some(BlockEntityType::SignBlockEntity(sign)) = world.get_block_entity(pos) {
                let front = text_from_face(&data.front);
                let back = text_from_face(&data.back);
                sign.set_front_text(&front);
                sign.set_back_text(&back);
                sign.set_waxed(data.waxed);
            }
        }
        Some(BlockEntityData::Chest(data)) => {
            if let Some(BlockEntityType::ChestBlockEntity(chest)) = world.get_block_entity(pos) {
                for (slot, item) in data.items.iter().enumerate() {
                    let item = item.as_ref().map(|data| {
                        let item = ItemStack::new(&data.registry_key, data.count);
                        for component in &data.components {
                            item.set_component(component.component, &component.value);
                        }
                        item
                    });
                    chest.set_item(slot as u32, item);
                }
            }
        }
        None => {}
    }
}

fn face_from_text(text: SignText) -> SignFace {
    let mut messages = array::from_fn(|_| String::new());
    for (index, message) in text.messages.into_iter().take(4).enumerate() {
        messages[index] = message;
    }
    SignFace {
        messages,
        color: text.color.into(),
        glowing: text.has_glowing_text,
    }
}

fn text_from_face(face: &SignFace) -> SignText {
    SignText {
        messages: face.messages.iter().cloned().collect(),
        color: face.color.into(),
        has_glowing_text: face.glowing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_callbacks_run_only_for_block_entity_states() {
        let base = BlockFlags::SKIP_DROPS | BlockFlags::FORCE_STATE;
        let door = mapping::state_id_for("minecraft:oak_door").unwrap();
        let chest = mapping::state_id_for("minecraft:chest").unwrap();

        assert_eq!(
            flags_for_state(door, base),
            base | BlockFlags::SKIP_BLOCK_ADDED_CALLBACK
        );
        assert_eq!(flags_for_state(chest, base), base);
    }
}
