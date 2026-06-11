//! `//schematic save|load|list` (alias `//schem`) — save the clipboard to a
//! `.schem` file, load one back into the clipboard, or list saved files.
//!
//! Mirrors WorldEdit's `SchematicCommands`. Files live in
//! `<data folder>/schematics/<name>.schem`, gzip-compressed Sponge v2 NBT
//! (see [`crate::schematic`]). `<name>` may include safe subdirectories under
//! the schematics folder.
//!
//! FAWE also supports multiple formats (`mcedit`, `structure`) and a
//! configurable schematics folder. This plugin currently reads and writes
//! Sponge `.schem` only.

use std::path::{Component, Path, PathBuf};

use pumpkin_plugin_api::{
    Context,
    command::{Command, CommandError, CommandNode, CommandSender, ConsumedArgs},
    command_wit::{Arg, ArgumentType, StringType},
    logging::{self, LogLevel},
    text::TextComponent,
};

use crate::{clipboard, schematic};

use super::{player_key, require_permission};

const SCHEMATIC_SAVE_PERMISSION: &str = "worldedit.schematic.save";
const SCHEMATIC_LOAD_PERMISSION: &str = "worldedit.schematic.load";
const SCHEMATIC_LIST_PERMISSION: &str = "worldedit.schematic.list";

pub fn register(context: &Context) {
    let data_folder = context.get_data_folder();

    let name_arg_save =
        CommandNode::argument("name", &ArgumentType::String(StringType::SingleWord)).execute(
            SaveCommand {
                data_folder: data_folder.clone(),
            },
        );
    let save = CommandNode::literal("save").execute(Usage);
    save.then(name_arg_save);

    let name_arg_load =
        CommandNode::argument("name", &ArgumentType::String(StringType::SingleWord)).execute(
            LoadCommand {
                data_folder: data_folder.clone(),
            },
        );
    let load = CommandNode::literal("load").execute(Usage);
    load.then(name_arg_load);

    let list = CommandNode::literal("list").execute(ListCommand {
        data_folder: data_folder.clone(),
    });

    let command = Command::new(
        &[
            "schematic".to_string(),
            "/schematic".to_string(),
            "schem".to_string(),
            "/schem".to_string(),
        ],
        "Save, load, or list schematics",
    )
    .execute(Usage);
    command.then(save);
    command.then(load);
    command.then(list);

    context.register_command(command, "worldedit-rs:command.schematic");
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
            "Usage: //schematic save <name>, //schematic load <name>, or //schematic list.",
        ));
        Ok(0)
    }
}

/// Validate a user-supplied schematic name and turn it into a `.schem` path
/// inside `data_folder/schematics/`.
///
/// Rejects names containing path separators, `..`, or that are empty, so a
/// player can't read or write outside the schematics folder.
fn schem_path(data_folder: &str, name: &str) -> Result<PathBuf, String> {
    if name.trim().is_empty() || name.contains('\0') {
        return Err(format!("Invalid schematic name '{name}'."));
    }

    let root = Path::new(data_folder).join("schematics");
    let mut relative = PathBuf::new();
    for component in Path::new(name).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            _ => return Err(format!("Invalid schematic name '{name}'.")),
        }
    }

    if relative.components().next().is_none() {
        return Err(format!("Invalid schematic name '{name}'."));
    }
    if relative.extension().and_then(|e| e.to_str()) != Some("schem") {
        relative.set_extension("schem");
    }

    Ok(root.join(relative))
}

/// Handler for `//schematic save <name>`.
struct SaveCommand {
    data_folder: String,
}

impl pumpkin_plugin_api::commands::CommandHandler for SaveCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if require_permission(&sender, &server, SCHEMATIC_SAVE_PERMISSION).is_err() {
            return Ok(0);
        }
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        let name = match args.get_value("name") {
            Arg::Simple(s) => s,
            _ => {
                sender.send_error(TextComponent::text("Expected a schematic name."));
                return Ok(0);
            }
        };

        let path = match schem_path(&self.data_folder, &name) {
            Ok(path) => path,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let Some(buffer) = clipboard::get(&key) else {
            sender.send_error(TextComponent::text(
                "Your clipboard is empty. Use //copy first.",
            ));
            return Ok(0);
        };
        let Some((width, height, length, offset, blocks)) = buffer.to_schematic_blocks() else {
            sender.send_error(TextComponent::text("Your clipboard is empty."));
            return Ok(0);
        };

        let bytes = match schematic::write(width, height, length, offset, &blocks) {
            Ok(bytes) => bytes,
            Err(e) => {
                sender.send_error(TextComponent::text(&format!(
                    "Failed to encode schematic: {e}"
                )));
                return Ok(0);
            }
        };

        if let Some(parent) = path.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            sender.send_error(TextComponent::text(&format!(
                "Failed to create schematics folder: {e}"
            )));
            return Ok(0);
        }

        if let Err(e) = std::fs::write(&path, &bytes) {
            sender.send_error(TextComponent::text(&format!(
                "Failed to write schematic: {e}"
            )));
            return Ok(0);
        }

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //schematic save wrote {} ({width}x{height}x{length}, {} bytes).",
                path.display(),
                bytes.len()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Saved schematic '{name}' ({width}x{height}x{length})."
        )));
        Ok(1)
    }
}

/// Handler for `//schematic load <name>`.
struct LoadCommand {
    data_folder: String,
}

impl pumpkin_plugin_api::commands::CommandHandler for LoadCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if require_permission(&sender, &server, SCHEMATIC_LOAD_PERMISSION).is_err() {
            return Ok(0);
        }
        if sender.as_player().is_none() {
            sender.send_error(TextComponent::text("Only players can use this command."));
            return Ok(0);
        }
        let Some(key) = player_key(&sender) else {
            return Ok(0);
        };
        let name = match args.get_value("name") {
            Arg::Simple(s) => s,
            _ => {
                sender.send_error(TextComponent::text("Expected a schematic name."));
                return Ok(0);
            }
        };

        let path = match schem_path(&self.data_folder, &name) {
            Ok(path) => path,
            Err(message) => {
                sender.send_error(TextComponent::text(&message));
                return Ok(0);
            }
        };

        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                sender.send_error(TextComponent::text(&format!(
                    "Failed to read schematic '{name}': {e}"
                )));
                return Ok(0);
            }
        };

        let parsed = match schematic::parse(&bytes) {
            Ok(parsed) => parsed,
            Err(e) => {
                sender.send_error(TextComponent::text(&format!(
                    "Failed to parse schematic '{name}': {e}"
                )));
                return Ok(0);
            }
        };

        let (width, height, length) = (parsed.width, parsed.height, parsed.length);
        let buffer = clipboard::from_schematic(&parsed);
        let blocks = buffer.blocks.len();
        clipboard::set(&key, buffer);

        logging::log(
            LogLevel::Info,
            &format!(
                "WorldEdit-rs: //schematic load read {} ({width}x{height}x{length}, {blocks} blocks).",
                path.display()
            ),
        );
        sender.send_message(TextComponent::text(&format!(
            "Loaded schematic '{name}' ({width}x{height}x{length}) into your clipboard."
        )));
        Ok(1)
    }
}

/// Handler for `//schematic list`.
struct ListCommand {
    data_folder: String,
}

impl pumpkin_plugin_api::commands::CommandHandler for ListCommand {
    fn handle(
        &self,
        sender: CommandSender,
        server: pumpkin_plugin_api::Server,
        _args: ConsumedArgs,
    ) -> std::result::Result<i32, CommandError> {
        if require_permission(&sender, &server, SCHEMATIC_LIST_PERMISSION).is_err() {
            return Ok(0);
        }
        let dir = Path::new(&self.data_folder).join("schematics");

        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                sender.send_message(TextComponent::text("No schematics saved yet."));
                return Ok(1);
            }
            Err(e) => {
                sender.send_error(TextComponent::text(&format!(
                    "Failed to list schematics: {e}"
                )));
                return Ok(0);
            }
        };

        let mut names = Vec::new();
        for entry in entries.filter_map(|entry| entry.ok()) {
            collect_schematic_names(&dir, &entry.path(), &mut names);
        }

        if names.is_empty() {
            sender.send_message(TextComponent::text("No schematics saved yet."));
            return Ok(1);
        }

        names.sort();
        sender.send_message(TextComponent::text(&format!(
            "Schematics ({}): {}",
            names.len(),
            names.join(", ")
        )));
        Ok(1)
    }
}

fn collect_schematic_names(root: &Path, path: &Path, names: &mut Vec<String>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.filter_map(|entry| entry.ok()) {
            collect_schematic_names(root, &entry.path(), names);
        }
        return;
    }

    if path.extension().and_then(|e| e.to_str()) != Some("schem") {
        return;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return;
    };
    let name = relative.with_extension("");
    let normalized = name
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if !normalized.is_empty() {
        names.push(normalized);
    }
}
