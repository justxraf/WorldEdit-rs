//! Maps a Sponge schematic palette entry (e.g. `"minecraft:oak_log[axis=x]"`)
//! or a raw numeric global state id (e.g. `"1"`) to a Pumpkin global
//! block-state id (the `u16` that [`world.set-block-state`](crate) wants).
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

/// One block as emitted by `build.rs`. `variants` is `(property-string, state-id)`
/// where `property-string` is the canonical `k=v,k=v` form with keys sorted.
pub struct GeneratedBlock {
    pub name: &'static str,
    pub default_id: u16,
    pub variants: &'static [(&'static str, u16)],
}

// Pulls in `pub static GENERATED_BLOCKS: &[GeneratedBlock]`.
include!(concat!(env!("OUT_DIR"), "/block_map.rs"));

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

/// `true` when the full generated table is present (vs. only the fallback).
pub fn has_full_registry() -> bool {
    !GENERATED_BLOCKS.is_empty()
}

fn find_generated(name: &str) -> Option<&'static GeneratedBlock> {
    GENERATED_BLOCKS.iter().find(|b| b.name == name)
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
    fn resolve_block_falls_back_to_name() {
        assert_eq!(
            resolve_block("minecraft:stone"),
            state_id_for("minecraft:stone")
        );
    }
}
