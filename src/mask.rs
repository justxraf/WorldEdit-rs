//! Per-player global mask (`//gmask`), layered on top of every edit command.
//!
//! Mirrors WorldEdit/FAWE's `GeneralCommands#gmask`: once set, region
//! commands, `//paste`, and brushes only touch positions that also match this
//! mask, in addition to whatever mask the command itself applies.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::pattern::BlockMask;

thread_local! {
    /// Global masks keyed by player name. The plugin's wasm component is
    /// single-threaded, so a thread-local map is sufficient.
    static GLOBAL_MASKS: RefCell<HashMap<String, BlockMask>> = RefCell::new(HashMap::new());
}

/// Set a player's global mask, replacing any existing one.
pub fn set(key: &str, mask: BlockMask) {
    GLOBAL_MASKS.with_borrow_mut(|map| {
        map.insert(key.to_string(), mask);
    });
}

/// Clear a player's global mask (`//gmask` with no argument).
pub fn clear(key: &str) {
    GLOBAL_MASKS.with_borrow_mut(|map| {
        map.remove(key);
    });
}

/// `true` if `state_id` passes the player's global mask, or if no global
/// mask is set.
pub fn passes(key: &str, state_id: u16) -> bool {
    GLOBAL_MASKS.with_borrow(|map| {
        map.get(key)
            .is_none_or(|mask| mask.matches(state_id))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_mask_passes_everything() {
        let key = "no_mask_passes_everything";
        assert!(passes(key, 0));
        assert!(passes(key, 42));
    }

    #[test]
    fn set_mask_restricts_matches() {
        let key = "set_mask_restricts_matches";
        set(key, BlockMask::States(vec![1]));
        assert!(passes(key, 1));
        assert!(!passes(key, 2));
        clear(key);
        assert!(passes(key, 2));
    }
}
