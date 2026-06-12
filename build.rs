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
    map_color: u8,
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
    #[serde(default)]
    opacity: u8,
    #[serde(default)]
    collision_shapes: Vec<u16>,
    #[serde(default)]
    outline_shapes: Vec<u16>,
}

#[derive(Deserialize)]
struct RawTags {
    #[serde(default)]
    block: BTreeMap<String, Vec<String>>,
}

const MAP_COLOR_BASES: [u32; 62] = [
    0, 8_368_696, 16_247_203, 13_092_807, 16_711_680, 10_526_975, 10_987_431, 31_744, 16_777_215,
    10_791_096, 9_923_917, 7_368_816, 4_210_943, 9_402_184, 16_776_437, 14_188_339, 11_685_080,
    6_724_056, 15_066_419, 8_375_321, 15_892_389, 5_000_268, 10_066_329, 5_013_401, 8_339_378,
    3_361_970, 6_704_179, 6_717_235, 10_040_115, 1_644_825, 16_445_005, 6_085_589, 4_882_687,
    55_610, 8_476_209, 7_340_544, 13_742_497, 10_441_252, 9_787_244, 7_367_818, 12_223_780,
    6_780_213, 10_505_550, 3_746_083, 8_874_850, 5_725_276, 8_014_168, 4_996_700, 4_993_571,
    5_001_770, 9_321_518, 2_430_480, 12_398_641, 9_715_553, 6_035_741, 1_474_182, 3_837_580,
    5_647_422, 1_356_933, 6_579_300, 14_200_723, 8_365_974,
];

#[derive(Clone)]
struct ColorEntry {
    block_index: u16,
    state_id: u16,
    color: u32,
    intensity: u16,
    priority: u32,
    name: String,
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
     pub static MAX_STATE_ID: u16 = 0;\n\
     pub static STATE_TO_BLOCK_INDEX: &[u16] = &[];\n\
     pub static COLOR_BLOCKS: &[GeneratedColorBlock] = &[];\n"
        .to_string()
}

fn empty_tag_table() -> String {
    "pub static GENERATED_BLOCK_TAGS: &[GeneratedBlockTag] = &[];\n".to_string()
}

fn argb_from_map_color(index: u8) -> u32 {
    MAP_COLOR_BASES
        .get(index as usize)
        .copied()
        .map_or(0, |rgb| (255u32 << 24) | rgb)
}

fn is_full_cube(state: &RawState) -> bool {
    state.collision_shapes.as_slice() == [0] && state.outline_shapes.as_slice() == [0]
}

fn is_color_eligible(block: &RawBlock, state: &RawState) -> bool {
    if block.map_color == 0 || state.opacity != 15 || !is_full_cube(state) {
        return false;
    }

    let name = block.name.as_str();
    if name.contains("shulker") {
        return false;
    }

    !matches!(
        name,
        "slime_block" | "honey_block" | "spawner" | "mob_spawner"
    )
}

fn color_block_priority(name: &str) -> u32 {
    let mut score = 100u32;

    if name.ends_with("_wool") {
        score = score.saturating_sub(50);
    }
    if name.ends_with("_concrete") {
        score = score.saturating_sub(45);
    }
    if name.ends_with("_terracotta") {
        score = score.saturating_sub(35);
    }
    if name.ends_with("_concrete_powder") {
        score = score.saturating_sub(30);
    }
    if name.ends_with("_planks") {
        score = score.saturating_sub(25);
    }
    if matches!(
        name,
        "stone"
            | "andesite"
            | "polished_andesite"
            | "diorite"
            | "polished_diorite"
            | "granite"
            | "polished_granite"
            | "deepslate"
            | "cobbled_deepslate"
            | "tuff"
            | "sand"
            | "red_sand"
            | "dirt"
            | "coarse_dirt"
            | "grass_block"
            | "podzol"
            | "mycelium"
            | "snow_block"
            | "obsidian"
            | "basalt"
            | "polished_basalt"
    ) {
        score = score.saturating_sub(20);
    }
    if name.ends_with("_log") || name.ends_with("_wood") {
        score += 15;
    }
    if name.contains("ore")
        || name.contains("redstone")
        || name.contains("command")
        || name.contains("lodestone")
        || name.contains("piston")
        || name.contains("bookshelf")
        || name.contains("crafting")
        || name.contains("loom")
        || name.contains("pumpkin")
        || name.contains("tnt")
        || name.contains("mushroom")
        || name.contains("wart")
        || name.contains("heart")
    {
        score += 40;
    }

    score
}

fn color_intensity(color: u32) -> u16 {
    let red = ((color >> 16) & 0xFF) as u16;
    let green = ((color >> 8) & 0xFF) as u16;
    let blue = (color & 0xFF) as u16;
    2 * red + 4 * green + 3 * blue
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
                    map_color: body
                        .get("map_color")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        .unwrap_or(0),
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
    let mut state_to_block = Vec::<u16>::new();
    let mut color_entries = Vec::<ColorEntry>::new();

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
        let default_state = block
            .states
            .iter()
            .find(|state| state.id == default)
            .or_else(|| block.states.iter().find(|state| state.default))
            .unwrap_or(&block.states[0]);
        let palette_color = if is_color_eligible(block, default_state) {
            argb_from_map_color(block.map_color)
        } else {
            0
        };
        let block_index = count as u16;

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

            let index = state.id as usize;
            if state_to_block.len() <= index {
                state_to_block.resize(index + 1, u16::MAX);
            }
            state_to_block[index] = block_index;
        }

        entries.push_str(&format!(
            "    GeneratedBlock {{ name: {name:?}, default_id: {default}, \
             variants: &[{variants}], palette_color: {palette_color} }},\n"
        ));
        if palette_color != 0 {
            color_entries.push(ColorEntry {
                block_index,
                state_id: default,
                color: palette_color,
                intensity: color_intensity(palette_color),
                priority: color_block_priority(&block.name),
                name: block.name.clone(),
            });
        }
        count += 1;
    }

    color_entries.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.state_id.cmp(&right.state_id))
    });

    let state_to_block_entries = state_to_block
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let color_block_entries = color_entries
        .iter()
        .map(|entry| {
            format!(
                "    GeneratedColorBlock {{ block_index: {}, state_id: {}, color: {}, intensity: {} }},\n",
                entry.block_index, entry.state_id, entry.color, entry.intensity
            )
        })
        .collect::<String>();

    let header = format!(
        "// AUTO-GENERATED by build.rs from assets/blocks.json. Do not edit.\n\
         // {count} blocks.\n\
         pub static GENERATED_BLOCKS: &[GeneratedBlock] = &[\n{entries}];\n\
         pub static MAX_STATE_ID: u16 = {max_state_id};\n\
         pub static STATE_TO_BLOCK_INDEX: &[u16] = &[{state_to_block_entries}];\n\
         pub static COLOR_BLOCKS: &[GeneratedColorBlock] = &[\n{color_block_entries}];\n"
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
