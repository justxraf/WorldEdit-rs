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

use pumpkin_plugin_api::text::TextComponent;

/// One block as emitted by `build.rs`. `variants` is `(property-string, state-id)`
/// where `property-string` is the canonical `k=v,k=v` form with keys sorted.
pub struct GeneratedBlock {
    pub name: &'static str,
    pub default_id: u16,
    pub variants: &'static [(&'static str, u16)],
}

pub struct GeneratedBlockTag {
    pub name: &'static str,
    pub blocks: &'static [&'static str],
}

// Pulls in `pub static GENERATED_BLOCKS: &[GeneratedBlock]`.
include!(concat!(env!("OUT_DIR"), "/block_map.rs"));
include!(concat!(env!("OUT_DIR"), "/block_tags.rs"));

/// Minimal hand-written table so Stage-1 testing works before `blocks.json` is
/// vendored. These ids are the vanilla 1.21 flattened global state ids, which is
/// the numbering Pumpkin uses. Treat them as best-effort: the generated table
/// always wins when present.
///
/// NOTE: verify against your Pumpkin build's `blocks.json` before relying on
/// these for anything beyond a smoke test.
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
/// Returns `None` if the block name is unknown. An unknown property combination
/// degrades to the block's default state rather than failing.
pub fn state_id_for(palette_key: &str) -> Option<u16> {
    let (name, props) = split_key(palette_key);
    let name = normalize(name);

    if let Some(block) = find_generated(&name) {
        if !props.is_empty() && !block.variants.is_empty() {
            let wanted = canonical_props(&props);
            if let Some((_, id)) = block.variants.iter().find(|(k, _)| *k == wanted) {
                return Some(*id);
            }
        }
        // No properties, no variant data, or no exact match: use the default.
        return Some(block.default_id);
    }

    // Fallback table is name-only (ignores properties).
    FALLBACK.iter().find(|(n, _)| *n == name).map(|(_, id)| *id)
}

/// Highest global state id we know about. Used to bounds-check raw numeric
/// ids passed to [`resolve_block`]. When the full registry isn't embedded,
/// `MAX_STATE_ID` is `0`, so fall back to a generous guess covering vanilla
/// 1.21's state space.
const FALLBACK_MAX_STATE_ID: u16 = 4095;

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
/// block registry.
pub fn state_ids_for_tag(tag: &str, all_states: bool) -> Vec<u16> {
    let tag = normalize_tag(tag);
    let Some(tag) = find_generated_tag(&tag) else {
        return Vec::new();
    };

    if all_states {
        let mut states = Vec::new();
        for &block in tag.blocks {
            for state_id in state_ids_for_block(block) {
                push_unique_state(&mut states, state_id);
            }
        }
        states
    } else {
        let mut states = Vec::new();
        for &block in tag.blocks {
            if let Some(state_id) = state_id_for(block) {
                push_unique_state(&mut states, state_id);
            }
        }
        states
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
    GENERATED_BLOCKS.iter().find_map(|block| {
        let matches =
            block.default_id == state_id || block.variants.iter().any(|&(_, id)| id == state_id);
        matches.then_some(block.name)
    })
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

    for block in GENERATED_BLOCKS {
        if let Some((props, _)) = block.variants.iter().find(|&&(_, id)| id == state_id) {
            return format!("{}[{}]", block.name, props);
        }
        if block.default_id == state_id {
            return block.name.to_string();
        }
    }

    "minecraft:air".to_string()
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

fn find_generated_tag(name: &str) -> Option<&'static GeneratedBlockTag> {
    GENERATED_BLOCK_TAGS.iter().find(|tag| tag.name == name)
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
}
