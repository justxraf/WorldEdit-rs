//! WorldEdit-rs — a small WorldEdit-style editing plugin for Pumpkin.
//!
//! Provides a per-player two-point selection (`//pos1`, `//pos2`) and basic
//! region operations (`//set`, `//replace`, `//copy`, `//cut`, `//paste`,
//! `//undo`, `//redo`, `//size`).
//!
//! Modules:
//! - [`mapping`]     — palette name -> Pumpkin global state id.
//! - [`schematic`]   — `.schem` (Sponge v2/v3) NBT parsing (used by clipboard paste).
//! - [`schem_paste`] — placing a parsed schematic into the world at a centre.
//! - [`selection`]   — per-player pos1/pos2 selection state and region helpers.
//! - [`clipboard`]   — per-player copy/paste buffer.
//! - [`transform`]   — block position and state rotation/flip transformations.
//! - [`history`]     — per-player undo/redo stack.
//! - [`mask`]        — per-player global mask (`//gmask`).
//! - [`commands`]    — `//` command registration and handlers.

mod block_data;
mod clipboard;
mod commands;
mod history;
mod mapping;
mod mask;
mod pattern;
mod schem_paste;
mod schematic;
mod selection;
mod simplex_noise;
mod transform;

use pumpkin_plugin_api::{
    Context, Plugin, PluginMetadata, Result,
    logging::{self, LogLevel},
    permissions,
};

struct WorldEditPlugin;

impl Plugin for WorldEditPlugin {
    fn new() -> Self {
        WorldEditPlugin
    }

    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            name: "worldedit-rs".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            authors: vec!["justxraf".to_string()],
            description: "A small WorldEdit-style editing plugin for Pumpkin.".to_string(),
            dependencies: vec![],
            permissions: vec![
                permissions::FS_READ_DATA.to_string(),
                permissions::FS_WRITE_DATA.to_string(),
            ],
        }
    }

    fn on_load(&mut self, context: Context) -> Result<()> {
        logging::log(LogLevel::Info, "WorldEdit-rs: loading.");

        if !mapping::has_full_registry() {
            logging::log(
                LogLevel::Warn,
                "WorldEdit-rs: full block registry not embedded (assets/blocks.json missing). \
                 Only a small set of blocks will map correctly. Drop Pumpkin's blocks.json \
                 into assets/ and rebuild for full support.",
            );
        }

        commands::register(&context);

        Ok(())
    }
}

pumpkin_plugin_api::register_plugin!(WorldEditPlugin);
