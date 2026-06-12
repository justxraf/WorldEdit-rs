//! Selection transform commands: `//expand`, `//contract`, `//shift`,
//! `//outset`, and `//inset`.

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, Number, StringType},
    text::TextComponent,
};

use crate::selection::{self, Direction, Region};

use super::{MAX_BUILD_Y, MIN_BUILD_Y, command_names, player_key};

const MAX_AMOUNT: i32 = 1_000_000;

pub fn register(context: &Context) {
    register_expand(context);
    register_contract(context);
    register_shift(context);
    register_outset(context);
    register_inset(context);
}

fn register_expand(context: &Context) {
    let vert = CommandNode::literal("vert").execute(VertExpandCommand);
    let direction =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(ExpandCommand);
    let reverse_direction =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(ExpandCommand);
    let reverse = CommandNode::argument(
        "reverse",
        &ArgumentType::Integer((Some(-MAX_AMOUNT), Some(MAX_AMOUNT))),
    )
    .execute(ExpandCommand);
    reverse.then(reverse_direction);
    let amount = CommandNode::argument(
        "amount",
        &ArgumentType::Integer((Some(-MAX_AMOUNT), Some(MAX_AMOUNT))),
    )
    .execute(ExpandCommand);
    amount.then(direction);
    amount.then(reverse);
    let command = Command::new(&command_names("expand"), "Expand your selection").execute(Usage);
    command.then(vert);
    command.then(amount);
    context.register_command(command, "worldedit.selection.expand");
}

fn register_contract(context: &Context) {
    let direction =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(ContractCommand);
    let reverse_direction =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(ContractCommand);
    let reverse = CommandNode::argument(
        "reverse",
        &ArgumentType::Integer((Some(-MAX_AMOUNT), Some(MAX_AMOUNT))),
    )
    .execute(ContractCommand);
    reverse.then(reverse_direction);
    let amount = CommandNode::argument(
        "amount",
        &ArgumentType::Integer((Some(-MAX_AMOUNT), Some(MAX_AMOUNT))),
    )
    .execute(ContractCommand);
    amount.then(direction);
    amount.then(reverse);
    let command =
        Command::new(&command_names("contract"), "Contract your selection").execute(Usage);
    command.then(amount);
    context.register_command(command, "worldedit.selection.contract");
}

fn register_shift(context: &Context) {
    let direction =
        CommandNode::argument("direction", &ArgumentType::String(StringType::SingleWord))
            .execute(ShiftCommand);
    let amount = CommandNode::argument(
        "amount",
        &ArgumentType::Integer((Some(-MAX_AMOUNT), Some(MAX_AMOUNT))),
    )
    .execute(ShiftCommand);
    amount.then(direction);
    let command = Command::new(&command_names("shift"), "Shift your selection").execute(Usage);
    command.then(amount);
    context.register_command(command, "worldedit.selection.shift");
}

fn register_outset(context: &Context) {
    let axes = CommandNode::argument("axes", &ArgumentType::String(StringType::SingleWord))
        .execute(OutsetCommand);
    let amount = CommandNode::argument(
        "amount",
        &ArgumentType::Integer((Some(0), Some(MAX_AMOUNT))),
    )
    .execute(OutsetCommand);
    amount.then(axes);
    let command = Command::new(&command_names("outset"), "Outset your selection").execute(Usage);
    command.then(amount);
    context.register_command(command, "worldedit.selection.outset");
}

fn register_inset(context: &Context) {
    let axes = CommandNode::argument("axes", &ArgumentType::String(StringType::SingleWord))
        .execute(InsetCommand);
    let amount = CommandNode::argument(
        "amount",
        &ArgumentType::Integer((Some(0), Some(MAX_AMOUNT))),
    )
    .execute(InsetCommand);
    amount.then(axes);
    let command = Command::new(&command_names("inset"), "Inset your selection").execute(Usage);
    command.then(amount);
    context.register_command(command, "worldedit.selection.inset");
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
            "Usage: //expand <amount> [reverse] [direction], //expand vert, //contract <amount> [reverse] [direction], //shift <amount> [direction], //outset <amount> [h|v|hv], or //inset <amount> [h|v|hv].",
        ));
        Ok(0)
    }
}

struct ExpandCommand;
struct VertExpandCommand;
struct ContractCommand;
struct ShiftCommand;
struct OutsetCommand;
struct InsetCommand;

impl pumpkin_plugin_api::commands::CommandHandler for ExpandCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        transform_selection(&sender, &args, Transform::Expand)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for VertExpandCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        expand_vert_selection(&sender)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for ContractCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        transform_selection(&sender, &args, Transform::Contract)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for ShiftCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        transform_selection(&sender, &args, Transform::Shift)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for OutsetCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        transform_selection(&sender, &args, Transform::Outset)
    }
}

impl pumpkin_plugin_api::commands::CommandHandler for InsetCommand {
    fn handle(
        &self,
        sender: CommandSender,
        _server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        transform_selection(&sender, &args, Transform::Inset)
    }
}

#[derive(Clone, Copy)]
enum Transform {
    Expand,
    Contract,
    Shift,
    Outset,
    Inset,
}

fn transform_selection(
    sender: &CommandSender,
    args: &ConsumedArgs,
    transform: Transform,
) -> std::result::Result<i32, CommandError> {
    if sender.as_player().is_none() {
        sender.send_error(TextComponent::text("Only players can use this command."));
        return Ok(0);
    }
    let Some(key) = player_key(sender) else {
        return Ok(0);
    };
    let Some(region) = selection::with_selection(&key, |sel| sel.region()) else {
        sender.send_error(TextComponent::text("Set both //pos1 and //pos2 first."));
        return Ok(0);
    };
    let Some(amount) = int_arg(args, "amount") else {
        sender.send_error(TextComponent::text("Expected an amount."));
        return Ok(0);
    };
    let directions = match transform {
        Transform::Outset | Transform::Inset => match axes_arg(args) {
            Ok(directions) => directions,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        },
        _ => match direction_arg(args, sender) {
            Ok(directions) => directions,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        },
    };
    let reverse = int_arg(args, "reverse").unwrap_or(0);

    let changed = apply_transform(region, transform, amount, reverse, &directions);
    let Some(changed) = changed else {
        sender.send_error(TextComponent::text(
            "That would invert or overflow your selection.",
        ));
        return Ok(0);
    };
    selection::set_region(&key, changed);

    let verb = match transform {
        Transform::Expand => "Expanded",
        Transform::Contract => "Contracted",
        Transform::Shift => "Shifted",
        Transform::Outset => "Outset",
        Transform::Inset => "Inset",
    };
    sender.send_message(TextComponent::text(&format!(
        "{verb} selection to ({}, {}, {}) -> ({}, {}, {}).",
        changed.min.x, changed.min.y, changed.min.z, changed.max.x, changed.max.y, changed.max.z
    )));
    Ok(1)
}

fn expand_vert_selection(sender: &CommandSender) -> std::result::Result<i32, CommandError> {
    if sender.as_player().is_none() {
        sender.send_error(TextComponent::text("Only players can use this command."));
        return Ok(0);
    }
    let Some(key) = player_key(sender) else {
        return Ok(0);
    };
    let Some(region) = selection::with_selection(&key, |sel| sel.region()) else {
        sender.send_error(TextComponent::text("Set both //pos1 and //pos2 first."));
        return Ok(0);
    };

    let mut changed = region;
    changed.min.y = MIN_BUILD_Y;
    changed.max.y = MAX_BUILD_Y;
    selection::set_region(&key, changed);
    sender.send_message(TextComponent::text(&format!(
        "Expanded selection to ({}, {}, {}) -> ({}, {}, {}).",
        changed.min.x, changed.min.y, changed.min.z, changed.max.x, changed.max.y, changed.max.z
    )));
    Ok(1)
}

fn apply_transform(
    region: Region,
    transform: Transform,
    amount: i32,
    reverse: i32,
    directions: &[Direction],
) -> Option<Region> {
    match transform {
        Transform::Expand => {
            let mut out = region;
            for &direction in directions {
                out = out
                    .expanded(amount, direction)?
                    .expanded(reverse, direction.opposite())?;
            }
            Some(out)
        }
        Transform::Contract => {
            let mut out = region;
            for &direction in directions {
                out = out
                    .contracted(amount, direction)?
                    .contracted(reverse, direction.opposite())?;
            }
            Some(out)
        }
        Transform::Shift => {
            let mut out = region;
            for &direction in directions {
                out = out.shifted(amount, direction)?;
            }
            Some(out)
        }
        Transform::Outset => {
            let mut out = region;
            for &direction in directions {
                out = out.expanded(amount, direction)?;
            }
            Some(out)
        }
        Transform::Inset => {
            let mut out = region;
            for &direction in directions {
                out = out.expanded(-amount, direction)?;
            }
            Some(out)
        }
    }
}

fn int_arg(args: &ConsumedArgs, name: &str) -> Option<i32> {
    match args.get_value(name) {
        Arg::Num(Ok(Number::Int32(n))) => Some(n),
        _ => None,
    }
}

fn direction_arg(args: &ConsumedArgs, sender: &CommandSender) -> Result<Vec<Direction>, String> {
    match args.get_value("direction") {
        Arg::Simple(raw) => parse_direction_list(&raw),
        _ => {
            let Some(player) = sender.as_player() else {
                return Err("A direction is required from console.".to_string());
            };
            Ok(vec![Direction::from_yaw_pitch(
                player.get_yaw(),
                player.get_pitch(),
            )])
        }
    }
}

fn parse_direction_list(raw: &str) -> Result<Vec<Direction>, String> {
    let mut directions = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.eq_ignore_ascii_case("vert") || part.eq_ignore_ascii_case("vertical") {
            directions.extend([Direction::Up, Direction::Down]);
            continue;
        }
        let Some(direction) = Direction::parse(part) else {
            return Err(format!(
                "Unknown direction '{part}'. Use north/south/east/west/up/down or comma-separated directions."
            ));
        };
        directions.push(direction);
    }
    if directions.is_empty() {
        Err("Expected a direction.".to_string())
    } else {
        Ok(directions)
    }
}

fn axes_arg(args: &ConsumedArgs) -> Result<Vec<Direction>, String> {
    match args.get_value("axes") {
        Arg::Simple(raw) => {
            let raw = raw.to_ascii_lowercase();
            let horizontal = raw.contains('h');
            let vertical = raw.contains('v');
            if !horizontal && !vertical {
                return Err("Axes must be h, v, or hv.".to_string());
            }
            let mut directions = Vec::with_capacity(6);
            if horizontal {
                directions.extend([
                    Direction::North,
                    Direction::South,
                    Direction::West,
                    Direction::East,
                ]);
            }
            if vertical {
                directions.extend([Direction::Up, Direction::Down]);
            }
            Ok(directions)
        }
        _ => Ok(vec![
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
            Direction::Up,
            Direction::Down,
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_plugin_api::common::BlockPos;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn expand_with_reverse_grows_both_sides() {
        let r = Region::new(at(0, 0, 0), at(2, 2, 2));
        let out = apply_transform(r, Transform::Expand, 2, 1, &[Direction::East]).unwrap();
        assert_eq!((out.min.x, out.max.x), (-1, 4));
    }

    #[test]
    fn contract_down_shrinks_from_top() {
        let r = Region::new(at(0, 0, 0), at(2, 4, 2));
        let out = apply_transform(r, Transform::Contract, 2, 0, &[Direction::Down]).unwrap();
        assert_eq!((out.min.y, out.max.y), (0, 2));
    }

    #[test]
    fn shift_preserves_dimensions() {
        let r = Region::new(at(0, 0, 0), at(2, 4, 2));
        let out = apply_transform(r, Transform::Shift, 3, 0, &[Direction::South]).unwrap();
        assert_eq!((out.min.z, out.max.z), (3, 5));
        assert_eq!(out.volume(), r.volume());
    }

    #[test]
    fn expand_accepts_multiple_directions() {
        let r = Region::new(at(0, 0, 0), at(2, 2, 2));
        let out = apply_transform(
            r,
            Transform::Expand,
            2,
            0,
            &[Direction::East, Direction::North],
        )
        .unwrap();
        assert_eq!((out.min.z, out.max.x), (-2, 4));
    }

    #[test]
    fn outset_and_inset_touch_all_requested_faces() {
        let r = Region::new(at(0, 0, 0), at(4, 4, 4));
        let dirs = [
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ];
        let out = apply_transform(r, Transform::Outset, 1, 0, &dirs).unwrap();
        assert_eq!((out.min.x, out.max.x, out.min.z, out.max.z), (-1, 5, -1, 5));
        let inset = apply_transform(r, Transform::Inset, 1, 0, &dirs).unwrap();
        assert_eq!(
            (inset.min.x, inset.max.x, inset.min.z, inset.max.z),
            (1, 3, 1, 3)
        );
    }
}
