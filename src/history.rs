//! Per-player undo/redo history.
//!
//! Mirrors WorldEdit's `//undo` and `//redo`: every edit command records the
//! state of each block it changes *before* and *after* the edit as one
//! [`EditEntry`], pushed onto the player's undo stack. `//undo` pops an entry,
//! restores the `before` states, and pushes the entry onto the redo stack;
//! `//redo` does the reverse.
//!
//! TODO(FAWE parity): real WorldEdit/FAWE keeps history per-session and can
//! spill large change sets to disk (`history.use-disk` in FAWE config) so an
//! edit of millions of blocks doesn't balloon memory. This implementation is
//! purely in-memory and caps the number of *entries* (not blocks), which is
//! fine for small/medium edits but not a full replacement.
//!
//! `//undo [times] [player]` and `//redo [times] [player]` can address another
//! player's in-memory stack by key. Unlike FAWE, edit entries do not store a
//! source world, so commands still apply through the invoking player's current
//! world.

use std::cell::RefCell;
use std::collections::HashMap;

use pumpkin_plugin_api::common::BlockPos;

use crate::block_data::BlockPlacement;

/// One undoable edit: for every changed block, its state before and after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditChange {
    pub pos: BlockPos,
    pub before: BlockPlacement,
    pub after: BlockPlacement,
}

/// One undoable edit: for every changed block, its state before and after.
#[derive(Default, Clone)]
pub struct EditEntry {
    /// Full before/after placements, including sign payloads when present.
    pub changes: Vec<EditChange>,
}

impl EditEntry {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn push_change(&mut self, pos: BlockPos, before: BlockPlacement, after: BlockPlacement) {
        self.changes.push(EditChange { pos, before, after });
    }

    pub fn push_state_change(&mut self, pos: BlockPos, before: u16, after: u16) {
        self.push_change(pos, BlockPlacement::new(before), BlockPlacement::new(after));
    }
}

/// Maximum number of edit entries kept per player on each stack. Older
/// entries are dropped once this is exceeded — see the module-level TODO
/// about FAWE's disk-backed history for handling much larger edit volumes.
const MAX_HISTORY_ENTRIES: usize = 32;

#[derive(Default)]
struct PlayerHistory {
    undo: Vec<EditEntry>,
    redo: Vec<EditEntry>,
}

thread_local! {
    /// History stacks keyed by player name. The plugin's wasm component is
    /// single-threaded, so a thread-local map is sufficient.
    static HISTORY: RefCell<HashMap<String, PlayerHistory>> = RefCell::new(HashMap::new());
}

/// Record a completed edit, clearing the player's redo stack (a fresh edit
/// invalidates any previously-undone redo history, matching WorldEdit).
///
/// No-ops if `entry` is empty (an edit that changed nothing isn't undoable).
pub fn push(key: &str, entry: EditEntry) {
    if entry.is_empty() {
        return;
    }
    HISTORY.with_borrow_mut(|map| {
        let history = map.entry(key.to_string()).or_default();
        history.undo.push(entry);
        if history.undo.len() > MAX_HISTORY_ENTRIES {
            history.undo.remove(0);
        }
        history.redo.clear();
    });
}

/// Pop the most recent undo entry, moving it to the redo stack, and return it
/// to the caller for application.
pub fn undo(key: &str) -> Option<EditEntry> {
    HISTORY.with_borrow_mut(|map| {
        let history = map.get_mut(key)?;
        let entry = history.undo.pop()?;
        history.redo.push(entry.clone());
        if history.redo.len() > MAX_HISTORY_ENTRIES {
            history.redo.remove(0);
        }
        Some(entry)
    })
}

/// Pop the most recent redo entry, moving it back to the undo stack, and
/// return it to the caller for application.
pub fn redo(key: &str) -> Option<EditEntry> {
    HISTORY.with_borrow_mut(|map| {
        let history = map.get_mut(key)?;
        let entry = history.redo.pop()?;
        history.undo.push(entry.clone());
        if history.undo.len() > MAX_HISTORY_ENTRIES {
            history.undo.remove(0);
        }
        Some(entry)
    })
}

/// Clear both stacks for a player (`//clearhistory`).
pub fn clear(key: &str) {
    HISTORY.with_borrow_mut(|map| {
        map.remove(key);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    fn entry(n: i32) -> EditEntry {
        let mut entry = EditEntry::default();
        entry.push_state_change(at(n, 0, 0), 0, 1);
        entry
    }

    #[test]
    fn undo_then_redo_round_trips() {
        let key = "undo_then_redo_round_trips";
        push(key, entry(1));
        push(key, entry(2));

        let popped = undo(key).unwrap();
        assert_eq!(popped.changes[0].pos.x, 2);

        let redone = redo(key).unwrap();
        assert_eq!(redone.changes[0].pos.x, 2);

        // Redo stack is now empty again.
        assert!(redo(key).is_none());

        // Redoing entry 2 puts it back on top of the undo stack.
        let popped = undo(key).unwrap();
        assert_eq!(popped.changes[0].pos.x, 2);

        // Entry 1 is still underneath it.
        let popped = undo(key).unwrap();
        assert_eq!(popped.changes[0].pos.x, 1);
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let key = "new_edit_clears_redo_stack";
        push(key, entry(1));
        undo(key);
        assert!(!HISTORY.with_borrow(|m| m.get(key).unwrap().redo.is_empty()));

        push(key, entry(2));
        assert!(HISTORY.with_borrow(|m| m.get(key).unwrap().redo.is_empty()));
    }

    #[test]
    fn empty_entry_is_not_pushed() {
        let key = "empty_entry_is_not_pushed";
        push(key, EditEntry::default());
        assert!(undo(key).is_none());
    }

    #[test]
    fn history_caps_at_max_entries() {
        let key = "history_caps_at_max_entries";
        for i in 0..(MAX_HISTORY_ENTRIES as i32 + 5) {
            push(key, entry(i));
        }
        let count = HISTORY.with_borrow(|m| m.get(key).unwrap().undo.len());
        assert_eq!(count, MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn clear_removes_history() {
        let key = "clear_removes_history";
        push(key, entry(1));
        clear(key);
        assert!(undo(key).is_none());
    }
}
