//! Maps a Sponge schematic palette entry (e.g. `"minecraft:oak_log[axis=x]"`)
//! or a raw numeric global state id (e.g. `"1"`) to a Pumpkin global
//! block-state id (the `u16` that [`world.set-block-state`](crate) wants).
//!
//! Also provides [`display_component`], which turns a block name or state id
//! back into a [`TextComponent`] suitable for chat messages — a translatable
//! `block.minecraft.<name>` component (rendered in each client's own
//! language) where possible, falling back to a humanized name like
//! "Grass Block" otherwise.
//!
//! ## How it resolves a name
//! 1. Split the palette string into a base name and a sorted property list.
//! 2. Look the base name up in the generated table ([`GENERATED_BLOCKS`], built by
//!    `build.rs` from `assets/blocks.json`).
//!    - If the table carries explicit per-state property variants, match the exact
//!      property string and use that id.
//!    - Otherwise fall back to the block's `default_id`.
//! 3. If the base name isn't in the generated table, try the small hand-written
//!    [`FALLBACK`] table so the pipeline still works without the big JSON.
//! 4. If nothing matches, return `None` and the caller decides (skip / use air).
//!
//! ## How it resolves a numeric id
//! Commands like `//set` and `//replace` also accept a bare integer, treated
//! as a global state id directly (see [`resolve_block`]). It's accepted as
//! long as it fits in the `0..=MAX_STATE_ID` range emitted by `build.rs`
//! (or `0..=4095`, a generous guess, when the full registry isn't embedded).

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{OnceLock, RwLock},
};

use pumpkin_plugin_api::text::TextComponent;
use serde::Deserialize;

/// One block as emitted by `build.rs`. `variants` is `(property-string, state-id)`
/// where `property-string` is the canonical `k=v,k=v` form with keys sorted.
pub struct GeneratedBlock {
    pub name: &'static str,
    pub default_id: u16,
    pub variants: &'static [(&'static str, u16)],
    pub palette_color: u32,
}

pub struct GeneratedColorBlock {
    pub block_index: u16,
    pub state_id: u16,
    pub color: u32,
    pub intensity: u16,
}

pub struct GeneratedBlockTag {
    pub name: &'static str,
    pub blocks: &'static [&'static str],
}

#[derive(Deserialize)]
struct RuntimeBlockTagsFile {
    #[serde(default)]
    block: BTreeMap<String, Vec<String>>,
}

// Pulls in `pub static GENERATED_BLOCKS: &[GeneratedBlock]`.
include!(concat!(env!("OUT_DIR"), "/block_map.rs"));
include!(concat!(env!("OUT_DIR"), "/block_tags.rs"));

static RUNTIME_BLOCK_TAGS: OnceLock<RwLock<BTreeMap<String, Vec<String>>>> = OnceLock::new();

/// Minimal hand-written table so Stage-1 testing works before `blocks.json` is
/// vendored. These ids are the vanilla 1.21 flattened global state ids, which is
/// the numbering Pumpkin uses. Treat them as best-effort: the generated table
/// always wins when present.
///

static FALLBACK: &[(&str, u16)] = &[
    ("minecraft:air", 0),
    ("minecraft:stone", 1),
    ("minecraft:granite", 2),
    ("minecraft:dirt", 10),
    ("minecraft:grass_block", 9), // default snowy=false
    ("minecraft:cobblestone", 14),
    ("minecraft:oak_planks", 15),
    ("minecraft:bedrock", 79),
    ("minecraft:sand", 112),
    ("minecraft:glass", 470),
];

/// Resolve a palette key like `"minecraft:oak_log[axis=x]"` to a state id.
///
/// Returns `None` if the block name is unknown. A partial property list takes
/// the default state's value for every unnamed property (WorldEdit semantics);
/// an unknown property name or value degrades to the block's default state
/// rather than failing.
pub fn state_id_for(palette_key: &str) -> Option<u16> {
    let (name, props) = split_key(palette_key);
    let name = normalize(name);

    if let Some(block) = find_generated(&name) {
        if !props.is_empty() && !block.variants.is_empty() {
            let wanted = canonical_props(&props);
            if let Some((_, id)) = block.variants.iter().find(|(k, _)| *k == wanted) {
                return Some(*id);
            }
            if let Some(id) = merge_with_default_props(block, &props) {
                return Some(id);
            }
        }
        // No properties, no variant data, or no usable match: use the default.
        return Some(block.default_id);
    }

    // Fallback table is name-only (ignores properties).
    FALLBACK.iter().find(|(n, _)| *n == name).map(|(_, id)| *id)
}

/// Overlay `props` onto `block`'s default-state property string and look the
/// merged combination up in `block.variants`, so a partial key like
/// `oak_sign[rotation=12]` resolves with default values for the properties it
/// doesn't name. `None` when the default state has no variant entry or the
/// merged combination doesn't exist (misspelled property name or value).
fn merge_with_default_props(block: &GeneratedBlock, props: &[&str]) -> Option<u16> {
    let (default_props, _) = block
        .variants
        .iter()
        .find(|&&(_, id)| id == block.default_id)?;
    // BTreeMap iterates keys sorted, so the merged string is canonical.
    let mut merged: BTreeMap<&str, &str> = default_props
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .collect();
    for kv in props {
        let (key, value) = kv.split_once('=')?;
        let key = key.trim();
        // A property the block doesn't have can never form a valid variant.
        if !merged.contains_key(key) {
            return None;
        }
        merged.insert(key, value.trim());
    }
    let merged_key = merged
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");
    block
        .variants
        .iter()
        .find(|(k, _)| *k == merged_key)
        .map(|&(_, id)| id)
}

/// Highest global state id we know about. Used to bounds-check raw numeric
/// ids passed to [`resolve_block`]. When the full registry isn't embedded,
/// `MAX_STATE_ID` is `0`, so fall back to a generous guess covering vanilla
/// 1.21's state space.
const FALLBACK_MAX_STATE_ID: u16 = 4095;
const INVALID_BLOCK_INDEX: u16 = u16::MAX;

/// Resolve a block argument that's either a raw global state id (e.g. `"1"`
/// for stone) or a palette key/name (see [`state_id_for`]).
///
/// Numeric input is accepted as-is, bounds-checked against the known state
/// id range so typos like `//set 999999` are rejected early.
pub fn resolve_block(input: &str) -> Option<u16> {
    let trimmed = input.trim();
    if let Ok(id) = trimmed.parse::<u16>() {
        let max = if has_full_registry() {
            MAX_STATE_ID
        } else {
            FALLBACK_MAX_STATE_ID
        };
        return if id <= max { Some(id) } else { None };
    }
    state_id_for(trimmed)
}

/// Return every known state id for a block name. If a property-qualified key is
/// supplied, this returns only the exact resolved state.
pub fn state_ids_for_block(input: &str) -> Vec<u16> {
    let trimmed = input.trim();
    if trimmed.parse::<u16>().is_ok() {
        return resolve_block(trimmed).into_iter().collect();
    }

    let (name, props) = split_key(trimmed);
    let name = normalize(name);
    if let Some(block) = find_generated(&name) {
        if !props.is_empty() {
            return state_id_for(trimmed).into_iter().collect();
        }

        let mut states = Vec::new();
        if block.variants.is_empty() {
            states.push(block.default_id);
        } else {
            for &(_, id) in block.variants {
                if !states.contains(&id) {
                    states.push(id);
                }
            }
        }
        return states;
    }

    resolve_block(trimmed).into_iter().collect()
}

/// Resolve a block tag such as `minecraft:slabs` or `c:stones`.
///
/// `##tag` returns the tag's direct block members (one default state per block),
/// while `##*tag` expands each tagged block to every known state in the embedded
/// block registry. Runtime overrides from `plugins/worldedit-rs/block_tags.json`
/// are merged on top of the generated table during plugin load.
pub fn state_ids_for_tag(tag: &str, all_states: bool) -> Vec<u16> {
    let tag = normalize_tag(tag);
    let tags = runtime_block_tags().read().unwrap();
    let Some(blocks) = tags.get(&tag) else {
        return Vec::new();
    };

    if all_states {
        let mut states = Vec::new();
        for block in blocks {
            for state_id in state_ids_for_block(block) {
                push_unique_state(&mut states, state_id);
            }
        }
        states
    } else {
        let mut states = Vec::new();
        for block in blocks {
            if let Some(state_id) = state_id_for(block) {
                push_unique_state(&mut states, state_id);
            }
        }
        states
    }
}

/// Load or refresh runtime block-tag overrides from the plugin data folder.
///
/// The file shape matches Pumpkin's tag dump:
/// `{ "block": { "custom:tag": ["minecraft:stone", "dirt"] } }`.
///
/// Missing files are fine and simply leave the generated tag table in place.
pub fn load_runtime_block_tags(data_folder: &str) -> Result<usize, String> {
    let mut tags = generated_block_tag_map();
    let path = Path::new(data_folder).join("block_tags.json");

    if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
        let file: RuntimeBlockTagsFile = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
        let custom_count = file.block.len();
        merge_runtime_block_tags(&mut tags, file.block);
        *runtime_block_tags().write().unwrap() = tags;
        Ok(custom_count)
    } else {
        *runtime_block_tags().write().unwrap() = tags;
        Ok(0)
    }
}

/// Apply the property string from `before` to the block type represented by
/// `target_state`, falling back to the target's default if that state
/// combination does not exist.
pub fn apply_existing_states(target_state: u16, before: u16) -> Option<u16> {
    let target_key = palette_key_for_state_id(target_state);
    let before_key = palette_key_for_state_id(before);
    let (target_name, _) = split_key(&target_key);
    let (_, before_props) = split_key(&before_key);
    if before_props.is_empty() {
        return Some(target_state);
    }
    let candidate = format!("{}[{}]", target_name, canonical_props(&before_props));
    state_id_for(&candidate).or(Some(target_state))
}

/// Apply explicit block-state properties to an existing state id.
pub fn apply_state_properties(before: u16, props: &str) -> Option<u16> {
    let before_key = palette_key_for_state_id(before);
    let (name, existing_props) = split_key(&before_key);
    let mut merged: Vec<String> = existing_props.into_iter().map(str::to_string).collect();

    for prop in props
        .split(',')
        .map(str::trim)
        .filter(|prop| !prop.is_empty())
    {
        let key = prop.split_once('=').map_or(prop, |(key, _)| key).trim();
        merged.retain(|existing| {
            existing
                .split_once('=')
                .map_or(existing.as_str(), |(k, _)| k)
                != key
        });
        merged.push(prop.to_string());
    }

    if merged.is_empty() {
        return Some(before);
    }
    let refs: Vec<&str> = merged.iter().map(String::as_str).collect();
    let candidate = format!("{}[{}]", name, canonical_props(&refs));
    state_id_for(&candidate).or(Some(before))
}

/// `true` when the full generated table is present (vs. only the fallback).
pub fn has_full_registry() -> bool {
    !GENERATED_BLOCKS.is_empty()
}

pub fn has_color_palette() -> bool {
    !COLOR_BLOCKS.is_empty()
}

pub fn nearest_color_block(color: u32) -> Option<u16> {
    nearest_color_candidate(normalize_color(color), false).map(|candidate| candidate.state_id)
}

pub fn saturate_existing_block(before: u16, color: u32) -> Option<u16> {
    let before_index = block_index_for_state_id(before)?;
    let current = color_for_state_id(before)?;
    let target = multiply_color(current, normalize_color(color));
    let candidate = nearest_color_candidate(target, false)?;
    (usize::from(candidate.block_index) != before_index).then_some(candidate.state_id)
}

pub fn average_existing_block(before: u16, color: u32) -> Option<u16> {
    let before_index = block_index_for_state_id(before)?;
    let current = color_for_state_id(before)?;
    let target = average_color(current, normalize_color(color));
    let candidate = nearest_color_candidate(target, false)?;
    (usize::from(candidate.block_index) != before_index).then_some(candidate.state_id)
}

pub fn desaturate_existing_block(before: u16, amount: f32) -> Option<u16> {
    let before_index = block_index_for_state_id(before)?;
    let current = color_for_state_id(before)?;
    let target = desaturate_color(current, amount);
    if target == current {
        return None;
    }

    let candidate = nearest_color_candidate(target, true)?;
    (usize::from(candidate.block_index) != before_index).then_some(candidate.state_id)
}

pub fn shade_existing_block(before: u16, darken: bool) -> Option<u16> {
    let before_index = block_index_for_state_id(before)?;
    let current = color_for_state_id(before)?;
    let current_intensity = color_intensity(current);

    let mut best = None::<(u64, &'static GeneratedColorBlock)>;
    for candidate in COLOR_BLOCKS {
        if usize::from(candidate.block_index) == before_index {
            continue;
        }
        let matches_intensity = if darken {
            candidate.intensity < current_intensity
        } else {
            candidate.intensity > current_intensity
        };
        if !matches_intensity {
            continue;
        }

        let distance = color_distance(current, candidate.color);
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, candidate));
        }
    }

    best.map(|(_, candidate)| candidate.state_id)
}

fn block_index_for_state_id(state_id: u16) -> Option<usize> {
    let index = *STATE_TO_BLOCK_INDEX.get(state_id as usize)?;
    (index != INVALID_BLOCK_INDEX).then_some(index as usize)
}

fn block_for_state_id(state_id: u16) -> Option<&'static GeneratedBlock> {
    GENERATED_BLOCKS.get(block_index_for_state_id(state_id)?)
}

fn color_for_state_id(state_id: u16) -> Option<u32> {
    let block = block_for_state_id(state_id)?;
    (block.palette_color != 0).then_some(block.palette_color)
}

fn nearest_color_candidate(
    color: u32,
    exclude_exact_color: bool,
) -> Option<&'static GeneratedColorBlock> {
    let color = normalize_color(color);
    let mut best = None::<(u64, &'static GeneratedColorBlock)>;
    for candidate in COLOR_BLOCKS {
        if exclude_exact_color && candidate.color == color {
            continue;
        }

        let distance = color_distance(color, candidate.color);
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

fn normalize_color(color: u32) -> u32 {
    (255u32 << 24) | (color & 0x00ff_ffff)
}

fn color_intensity(color: u32) -> u16 {
    let red = ((color >> 16) & 0xFF) as u16;
    let green = ((color >> 8) & 0xFF) as u16;
    let blue = (color & 0xFF) as u16;
    2 * red + 4 * green + 3 * blue
}

fn multiply_color(left: u32, right: u32) -> u32 {
    let red = (((left >> 16) & 0xFF) * ((right >> 16) & 0xFF)) / 255;
    let green = (((left >> 8) & 0xFF) * ((right >> 8) & 0xFF)) / 255;
    let blue = ((left & 0xFF) * (right & 0xFF)) / 255;
    (255u32 << 24) | (red << 16) | (green << 8) | blue
}

fn average_color(left: u32, right: u32) -> u32 {
    let red = (((left >> 16) & 0xFF) + ((right >> 16) & 0xFF)) >> 1;
    let green = (((left >> 8) & 0xFF) + ((right >> 8) & 0xFF)) >> 1;
    let blue = ((left & 0xFF) + (right & 0xFF)) >> 1;
    (255u32 << 24) | (red << 16) | (green << 8) | blue
}

fn desaturate_color(color: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let red = ((color >> 16) & 0xFF) as f32;
    let green = ((color >> 8) & 0xFF) as f32;
    let blue = (color & 0xFF) as f32;
    let luminance = 0.3 * red + 0.6 * green + 0.1 * blue;
    let new_red = (red + amount * (luminance - red)).round().clamp(0.0, 255.0) as u32;
    let new_green = (green + amount * (luminance - green))
        .round()
        .clamp(0.0, 255.0) as u32;
    let new_blue = (blue + amount * (luminance - blue))
        .round()
        .clamp(0.0, 255.0) as u32;
    (255u32 << 24) | (new_red << 16) | (new_green << 8) | new_blue
}

fn color_distance(left: u32, right: u32) -> u64 {
    let red1 = ((left >> 16) & 0xFF) as i32;
    let green1 = ((left >> 8) & 0xFF) as i32;
    let blue1 = (left & 0xFF) as i32;
    let red2 = ((right >> 16) & 0xFF) as i32;
    let green2 = ((right >> 8) & 0xFF) as i32;
    let blue2 = (right & 0xFF) as i32;
    let rmean = (red1 + red2) >> 1;
    let red = red1 - red2;
    let green = green1 - green2;
    let blue = blue1 - blue2;
    let hue = hue_distance(red1, green1, blue1, red2, green2, blue2) as i64;
    ((((512 + rmean) as i64) * i64::from(red * red)) >> 8) as u64
        + (4 * i64::from(green * green)) as u64
        + ((((767 - rmean) as i64) * i64::from(blue * blue)) >> 8) as u64
        + (hue * hue) as u64
}

fn hue_distance(red1: i32, green1: i32, blue1: i32, red2: i32, green2: i32, blue2: i32) -> i32 {
    let total1 = red1 + green1 + blue1;
    let total2 = red2 + green2 + blue2;
    if total1 == 0 || total2 == 0 {
        return 0;
    }

    let factor1 = 255.0 / total1 as f32;
    let factor2 = 255.0 / total2 as f32;
    let red = 0.5 * ((red1 as f32 * factor1) - (red2 as f32 * factor2));
    let green = (green1 as f32 * factor1) - (green2 as f32 * factor2);
    let blue = 0.749_023_44 * ((blue1 as f32 * factor1) - (blue2 as f32 * factor2));
    ((red * red + green * green + blue * blue) / 33_554_432.0) as i32
}

/// A chat-friendly representation of a block, for messages like
/// `//set <block>`'s "Set N blocks to <name>."
///
/// `input` is whatever the player typed (a name, palette key, or numeric
/// state id) and `state_id` is the id it resolved to. Where possible this
/// returns a translatable `block.minecraft.<name>` component, which each
/// client renders in its own configured language; otherwise it falls back to
/// a humanized name such as "Grass Block".
pub fn display_component(input: &str, state_id: u16) -> TextComponent {
    let name = if input.trim().parse::<u16>().is_ok() {
        // Numeric input (a raw state id) carries no name — recover one by
        // reverse-looking-up the resolved state id in the generated table.
        name_for_state_id(state_id).map(str::to_string)
    } else {
        Some(normalize(split_key(input).0))
    };

    match name.as_deref().and_then(|n| n.strip_prefix("minecraft:")) {
        Some(suffix) => TextComponent::translate(&format!("block.minecraft.{suffix}"), Vec::new()),
        None => TextComponent::text(&humanize(name.as_deref().unwrap_or(input))),
    }
}

/// Reverse-lookup: find the generated block whose default state or one of its
/// variants matches `state_id`, returning its namespaced name.
fn name_for_state_id(state_id: u16) -> Option<&'static str> {
    block_for_state_id(state_id).map(|block| block.name)
}

/// Reverse-lookup a global state id to a Sponge schematic palette key, e.g.
/// `"minecraft:oak_log[axis=x]"` or `"minecraft:stone"` (no properties).
///
/// Used by `//schematic save` to turn the state ids stored in a clipboard
/// back into palette entries. Falls back to `"minecraft:air"` for state id
/// `0` and for any id that isn't in the generated table (so saving never
/// fails outright when the full registry isn't embedded).
pub fn palette_key_for_state_id(state_id: u16) -> String {
    if state_id == 0 {
        return "minecraft:air".to_string();
    }

    if let Some(block) = block_for_state_id(state_id) {
        if let Some((props, _)) = block.variants.iter().find(|&&(_, id)| id == state_id) {
            return format!("{}[{}]", block.name, props);
        }
        if block.default_id == state_id {
            return block.name.to_string();
        }
    }

    "minecraft:air".to_string()
}

/// Whether this state requires a backing block entity to function/render.
pub fn state_has_block_entity(state_id: u16) -> bool {
    STATE_HAS_BLOCK_ENTITY
        .get(state_id as usize)
        .copied()
        .unwrap_or(false)
}

/// Turn `"minecraft:grass_block"` (or `"grass_block"`) into `"Grass Block"`:
/// strip the namespace, replace underscores with spaces, and title-case each
/// word.
fn humanize(name: &str) -> String {
    let name = name.split_once(':').map_or(name, |(_, rest)| rest);
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_generated(name: &str) -> Option<&'static GeneratedBlock> {
    GENERATED_BLOCKS.iter().find(|b| b.name == name)
}

fn generated_block_tag_map() -> BTreeMap<String, Vec<String>> {
    let mut tags = BTreeMap::new();
    for tag in GENERATED_BLOCK_TAGS {
        tags.insert(
            normalize_tag(tag.name),
            normalize_tag_blocks(tag.blocks.iter().copied().map(str::to_string).collect()),
        );
    }
    tags
}

fn runtime_block_tags() -> &'static RwLock<BTreeMap<String, Vec<String>>> {
    RUNTIME_BLOCK_TAGS.get_or_init(|| RwLock::new(generated_block_tag_map()))
}

fn merge_runtime_block_tags(
    tags: &mut BTreeMap<String, Vec<String>>,
    custom_tags: BTreeMap<String, Vec<String>>,
) {
    for (tag, blocks) in custom_tags {
        tags.insert(normalize_tag(&tag), normalize_tag_blocks(blocks));
    }
}

fn normalize_tag_blocks(blocks: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for block in blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let block = normalize(split_key(block).0);
        if !normalized.contains(&block) {
            normalized.push(block);
        }
    }
    normalized
}

fn normalize_tag(tag: &str) -> String {
    let tag = tag
        .trim()
        .trim_start_matches('#')
        .trim_start_matches('*')
        .to_ascii_lowercase();
    if tag.contains(':') {
        tag
    } else {
        format!("minecraft:{tag}")
    }
}

fn push_unique_state(states: &mut Vec<u16>, state_id: u16) {
    if !states.contains(&state_id) {
        states.push(state_id);
    }
}

/// Split `"minecraft:oak_log[axis=x,foo=bar]"` into
/// `("minecraft:oak_log", ["axis=x", "foo=bar"])`.
fn split_key(key: &str) -> (&str, Vec<&str>) {
    match key.split_once('[') {
        Some((name, rest)) => {
            let rest = rest.strip_suffix(']').unwrap_or(rest);
            let props = if rest.is_empty() {
                Vec::new()
            } else {
                rest.split(',').map(str::trim).collect()
            };
            (name.trim(), props)
        }
        None => (key.trim(), Vec::new()),
    }
}

/// Sort `["axis=x","foo=bar"]` by key so it matches the generated `variants`
/// (which build.rs emits with keys sorted, since it reads a BTreeMap).
fn canonical_props(props: &[&str]) -> String {
    let mut p: Vec<&str> = props.to_vec();
    p.sort_unstable();
    p.join(",")
}

fn normalize(name: &str) -> String {
    if name.contains(':') {
        name.to_string()
    } else {
        format!("minecraft:{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_TAG_MUTATION: Mutex<()> = Mutex::new(());

    fn with_test_runtime_tag<T>(tag: &str, blocks: &[&str], f: impl FnOnce() -> T) -> T {
        let _guard = TEST_TAG_MUTATION.lock().unwrap();
        let original = runtime_block_tags().read().unwrap().clone();
        let mut merged = original.clone();
        merged.insert(
            normalize_tag(tag),
            normalize_tag_blocks(blocks.iter().copied().map(str::to_string).collect()),
        );
        *runtime_block_tags().write().unwrap() = merged;
        let result = f();
        *runtime_block_tags().write().unwrap() = original;
        result
    }

    #[test]
    fn splits_properties() {
        let (n, p) = split_key("minecraft:oak_log[axis=x]");
        assert_eq!(n, "minecraft:oak_log");
        assert_eq!(p, vec!["axis=x"]);
    }

    #[test]
    fn splits_no_properties() {
        let (n, p) = split_key("minecraft:stone");
        assert_eq!(n, "minecraft:stone");
        assert!(p.is_empty());
    }

    #[test]
    fn fallback_resolves_stone() {
        assert_eq!(state_id_for("minecraft:stone"), Some(1));
    }

    #[test]
    fn unknown_block_is_none() {
        assert_eq!(state_id_for("minecraft:definitely_not_a_block"), None);
    }

    #[test]
    fn resolve_block_accepts_numeric_state_id() {
        assert_eq!(resolve_block("1"), Some(1));
        assert_eq!(resolve_block(" 0 "), Some(0));
    }

    #[test]
    fn resolve_block_rejects_out_of_range_id() {
        assert_eq!(resolve_block("65535"), None);
    }

    #[test]
    fn humanize_strips_namespace_and_underscores() {
        assert_eq!(humanize("minecraft:grass_block"), "Grass Block");
        assert_eq!(humanize("oak_log"), "Oak Log");
        assert_eq!(humanize("stone"), "Stone");
    }

    #[test]
    fn resolve_block_falls_back_to_name() {
        assert_eq!(
            resolve_block("minecraft:stone"),
            state_id_for("minecraft:stone")
        );
    }

    #[test]
    fn palette_key_round_trips_through_state_id() {
        assert_eq!(palette_key_for_state_id(0), "minecraft:air");
        let stone_id = state_id_for("minecraft:stone").unwrap();
        assert_eq!(
            state_id_for(&palette_key_for_state_id(stone_id)),
            Some(stone_id)
        );
    }

    #[test]
    fn door_states_round_trip_through_palette_keys() {
        let default = state_id_for("minecraft:oak_door").unwrap();
        // Pumpkin's default oak door state, spelled out as properties.
        assert_eq!(
            state_id_for(
                "minecraft:oak_door[facing=north,half=lower,hinge=left,open=false,powered=false]"
            ),
            Some(default)
        );

        // A non-default state (the upper half) must resolve to its own state
        // id, not collapse to the default, and must reverse-map to a real
        // palette key rather than air.
        let upper = state_id_for(
            "minecraft:oak_door[facing=east,half=upper,hinge=left,open=false,powered=false]",
        )
        .unwrap();
        assert_ne!(upper, default);
        let key = palette_key_for_state_id(upper);
        assert!(key.starts_with("minecraft:oak_door["), "got {key}");
        assert_eq!(state_id_for(&key), Some(upper));
    }

    #[test]
    fn partial_properties_merge_onto_the_default_state() {
        // Door default: facing=north,half=lower,hinge=left,open=false,powered=false.
        let full = state_id_for(
            "minecraft:oak_door[facing=east,half=lower,hinge=left,open=false,powered=false]",
        )
        .unwrap();
        assert_eq!(state_id_for("minecraft:oak_door[facing=east]"), Some(full));

        // Unknown property names / values still degrade to the default state.
        let default = state_id_for("minecraft:oak_door").unwrap();
        assert_eq!(
            state_id_for("minecraft:oak_door[no_such_prop=yes]"),
            Some(default)
        );
        assert_eq!(
            state_id_for("minecraft:oak_door[facing=upside_down]"),
            Some(default)
        );
    }

    #[test]
    fn block_entity_metadata_distinguishes_chests_from_doors() {
        let chest = state_id_for("minecraft:chest").unwrap();
        let door = state_id_for("minecraft:oak_door").unwrap();

        assert!(state_has_block_entity(chest));
        assert!(!state_has_block_entity(door));
    }

    #[test]
    fn block_tags_support_namespaced_lookup() {
        let slab_defaults = state_ids_for_tag("minecraft:slabs", false);
        assert!(!slab_defaults.is_empty());
        assert!(slab_defaults.contains(&state_id_for("minecraft:oak_slab").unwrap()));
    }

    #[test]
    fn block_tags_return_empty_for_empty_tags() {
        assert!(state_ids_for_tag("c:ropes", false).is_empty());
    }

    #[test]
    fn block_tags_can_expand_to_all_known_states() {
        let slab_defaults = state_ids_for_tag("minecraft:slabs", false);
        let slab_states = state_ids_for_tag("minecraft:slabs", true);
        let oak_slab_states = state_ids_for_block("minecraft:oak_slab");

        assert!(oak_slab_states.len() > 1);
        assert!(oak_slab_states.iter().all(|id| slab_states.contains(id)));
        assert!(oak_slab_states.iter().any(|id| !slab_defaults.contains(id)));
        assert!(slab_states.len() > slab_defaults.len());
    }

    #[test]
    fn runtime_block_tags_support_custom_tags() {
        with_test_runtime_tag("custom:terrain_mix", &["stone", "minecraft:dirt"], || {
            let states = state_ids_for_tag("custom:terrain_mix", false);
            assert_eq!(states.len(), 2);
            assert!(states.contains(&state_id_for("stone").unwrap()));
            assert!(states.contains(&state_id_for("dirt").unwrap()));
        });
    }
}
