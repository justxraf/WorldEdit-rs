//! Build script that compiles Pumpkin's block registry into a fast, embedded
//! `block name + properties -> global state id` lookup table.
//!
//! ## Where the data comes from
//! Pumpkin ships a `blocks.json` (the same one in the `Pumpkin/assets` folder).
//! Drop a copy at `assets/blocks.json` next to this build script. At build time we
//! parse it and emit `$OUT_DIR/block_map.rs`, which `mapping.rs` includes.
//!
//! ## What gets emitted
//! For every block we record, keyed by its full namespaced name (e.g.
//! `minecraft:oak_log`):
//! - `default_id` — used when a schematic entry has no properties, or when a
//!   property combination can't be matched.
//! - `variants` — explicit `(property-string, state-id)` pairs when the dump
//!   provides per-state properties (mojang report style).
//!
//! If `assets/blocks.json` is absent the emitted table is empty and `mapping.rs`
//! falls back to its small hand-written table — enough to test the pipeline.

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

#[derive(Deserialize)]
struct RawBlock {
    name: String,
    #[serde(default)]
    default_state_id: u16,
    #[serde(default)]
    states: Vec<RawState>,
}

#[derive(Deserialize)]
struct RawState {
    id: u16,
    /// Present in mojang-style "blocks.json" reports; maps property name -> value.
    #[serde(default)]
    properties: Option<BTreeMap<String, String>>,
    #[serde(default)]
    default: bool,
}

#[derive(Deserialize)]
struct RawTags {
    #[serde(default)]
    block: BTreeMap<String, Vec<String>>,
}

fn main() {
    println!("cargo:rerun-if-changed=assets/blocks.json");
    println!("cargo:rerun-if-changed=assets/block_tags.json");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../pumpkin/Pumpkin/assets/tags/26_1_tags.json");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let block_map_dest = Path::new(&out_dir).join("block_map.rs");
    let block_tag_dest = Path::new(&out_dir).join("block_tags.rs");

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/blocks.json");
    let tag_assets = candidate_tag_assets();

    let generated_blocks = match fs::read_to_string(&assets) {
        Ok(text) => match generate_from_json(&text) {
            Ok(code) => code,
            Err(e) => {
                println!(
                    "cargo:warning=islands: failed to parse assets/blocks.json ({e}); using fallback block table"
                );
                empty_table()
            }
        },
        Err(_) => {
            println!(
                "cargo:warning=islands: assets/blocks.json not found; using small fallback block \
                 table. Drop Pumpkin's blocks.json there for full schematic support."
            );
            empty_table()
        }
    };

    let generated_tags = match read_tag_assets(&tag_assets) {
        Ok(text) => match generate_tag_table(&text) {
            Ok(code) => code,
            Err(e) => {
                println!(
                    "cargo:warning=islands: failed to parse block tags ({}: {e}); using empty block-tag table",
                    tag_assets.display()
                );
                empty_tag_table()
            }
        },
        Err(_) => {
            println!(
                "cargo:warning=islands: block tag dump not found; using empty block-tag table. \
                 Add assets/block_tags.json or keep Pumpkin checked out at ../pumpkin/Pumpkin \
                 for real ##tag support."
            );
            empty_tag_table()
        }
    };

    fs::write(&block_map_dest, generated_blocks).expect("failed to write block_map.rs");
    fs::write(&block_tag_dest, generated_tags).expect("failed to write block_tags.rs");
}

fn empty_table() -> String {
    "pub static GENERATED_BLOCKS: &[GeneratedBlock] = &[];\n\
     pub static MAX_STATE_ID: u16 = 0;\n"
        .to_string()
}

fn empty_tag_table() -> String {
    "pub static GENERATED_BLOCK_TAGS: &[GeneratedBlockTag] = &[];\n".to_string()
}

/// Two shapes are supported:
/// 1. A bare array of blocks: `[ {name, states, ...}, ... ]`
/// 2. An object: `{ "blocks": [ ... ] }` or a mojang map `{ "minecraft:stone": {...} }`
fn generate_from_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;

    let blocks: Vec<RawBlock> = if let Some(arr) = value.as_array() {
        serde_json::from_value(serde_json::Value::Array(arr.clone())).map_err(|e| e.to_string())?
    } else if let Some(arr) = value.get("blocks").and_then(|b| b.as_array()) {
        serde_json::from_value(serde_json::Value::Array(arr.clone())).map_err(|e| e.to_string())?
    } else if let Some(obj) = value.as_object() {
        obj.iter()
            .filter_map(|(name, body)| {
                let states: Vec<RawState> = body
                    .get("states")
                    .and_then(|s| serde_json::from_value(s.clone()).ok())
                    .unwrap_or_default();
                if states.is_empty() {
                    return None;
                }
                let default_state_id = states
                    .iter()
                    .find(|s| s.default)
                    .or_else(|| states.first())
                    .map(|s| s.id)
                    .unwrap_or(0);
                Some(RawBlock {
                    name: name.clone(),
                    default_state_id,
                    states,
                })
            })
            .collect()
    } else {
        return Err("unrecognised blocks.json shape".to_string());
    };

    let mut entries = String::new();
    let mut count = 0usize;
    let mut max_state_id = 0u16;

    for block in &blocks {
        if block.states.is_empty() {
            continue;
        }
        let name = full_name(&block.name);
        let first = block.states.iter().map(|s| s.id).min().unwrap_or(0);
        max_state_id = max_state_id.max(block.states.iter().map(|s| s.id).max().unwrap_or(0));
        let default = if block.default_state_id != 0 || block.states.iter().any(|s| s.id == 0) {
            block.default_state_id
        } else {
            block
                .states
                .iter()
                .find(|s| s.default)
                .map(|s| s.id)
                .unwrap_or(first)
        };

        let mut variants = String::new();
        for state in &block.states {
            if let Some(props) = &state.properties
                && !props.is_empty()
            {
                // BTreeMap iterates keys sorted, matching mapping.rs::canonical_props.
                let key = props
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                variants.push_str(&format!("({key:?}, {}), ", state.id));
            }
        }

        entries.push_str(&format!(
            "    GeneratedBlock {{ name: {name:?}, default_id: {default}, \
             variants: &[{variants}] }},\n"
        ));
        count += 1;
    }

    let header = format!(
        "// AUTO-GENERATED by build.rs from assets/blocks.json. Do not edit.\n\
         // {count} blocks.\n\
         pub static GENERATED_BLOCKS: &[GeneratedBlock] = &[\n{entries}];\n\
         pub static MAX_STATE_ID: u16 = {max_state_id};\n"
    );
    Ok(header)
}

fn full_name(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    }
}

fn candidate_tag_assets() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let local = manifest_dir.join("assets/block_tags.json");
    if local.exists() {
        return local;
    }
    manifest_dir.join("../pumpkin/Pumpkin/assets/tags/26_1_tags.json")
}

fn read_tag_assets(path: &Path) -> Result<String, std::io::Error> {
    fs::read_to_string(path)
}

fn generate_tag_table(text: &str) -> Result<String, String> {
    let raw: RawTags = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut entries = String::new();

    for (tag, blocks) in raw.block {
        let members = blocks
            .into_iter()
            .map(|block| format!("{block:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        entries.push_str(&format!(
            "    GeneratedBlockTag {{ name: {tag:?}, blocks: &[{members}] }},\n"
        ));
    }

    Ok(format!(
        "// AUTO-GENERATED by build.rs from a block-tag dump. Do not edit.\n\
         pub static GENERATED_BLOCK_TAGS: &[GeneratedBlockTag] = &[\n{entries}];\n"
    ))
}
