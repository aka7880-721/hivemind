// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyGit License
//
// For details, see the LICENSE file in the repository root.

use serde::{Deserialize, Serialize};

/// Represents a single variator entry with an auto-assigned id and variator_id
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Variator {
    pub id: u32,
    pub variator_id: String,
    pub variator: String,
}

/// Outcome of a rename operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameOutcome {
    /// Successfully renamed; contains the updated entry
    Renamed(Variator),
    /// Key matched both a numeric id and a variator_id of *different* entries;
    /// contains the by-numeric match, the by-name match, and the requested new value
    Conflict(Variator, Variator, String),
    /// No entry found for the given key
    NotFound,
}

impl Variator {
    pub fn new(id: u32, variator_id: impl Into<String>, variator: impl Into<String>) -> Self {
        Self {
            id,
            variator_id: variator_id.into(),
            variator: variator.into(),
        }
    }
}

/// Storage for all variators with auto-incrementing id
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct VariatorStorage {
    #[serde(default)]
    pub variators: Vec<Variator>,
    #[serde(default)]
    next_id: u32,
}

impl VariatorStorage {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            variators: Vec::new(),
            next_id: 1,
        }
    }

    /// Set a new variator, or overwrite existing one with same variator_id
    pub fn set(&mut self, variator_id: impl Into<String>, variator: impl Into<String>) -> u32 {
        let variator_id = variator_id.into();
        let variator = variator.into();

        // Check if variator_id already exists
        if let Some(existing) = self
            .variators
            .iter_mut()
            .find(|v| v.variator_id == variator_id)
        {
            let id = existing.id;
            existing.variator = variator;
            return id;
        }

        // Create new variator with auto-incremented id
        let id = self.next_id;
        self.next_id += 1;
        self.variators
            .push(Variator::new(id, variator_id, variator));
        id
    }

    /// Get a variator by id or variator_id
    pub fn get(&self, key: impl AsRef<str>) -> Option<&Variator> {
        let key = key.as_ref();

        // Try as numeric id first
        if let Ok(id) = key.parse::<u32>()
            && let Some(v) = self.variators.iter().find(|v| v.id == id)
        {
            return Some(v);
        }

        // Try as variator_id
        self.variators.iter().find(|v| v.variator_id == key)
    }

    /// Remove variators by id or variator_id (supports comma-separated list)
    pub fn remove(&mut self, keys: impl AsRef<str>) -> Vec<String> {
        let keys: Vec<&str> = keys.as_ref().split(',').map(|s| s.trim()).collect();
        let mut removed = Vec::new();

        for key in keys {
            if let Ok(id) = key.parse::<u32>() {
                // Remove by numeric id
                if let Some(pos) = self.variators.iter().position(|v| v.id == id) {
                    let v = self.variators.remove(pos);
                    removed.push(format!("{} (id={})", v.variator_id, v.id));
                }
            } else {
                // Remove by variator_id
                if let Some(pos) = self.variators.iter().position(|v| v.variator_id == key) {
                    let v = self.variators.remove(pos);
                    removed.push(format!("{} (id={})", v.variator_id, v.id));
                }
            }
        }

        removed
    }

    /// Rename a variator's value by id or variator_id.
    /// Returns `Ok(None)` when the key is ambiguous (matches both a numeric id and a variator_id
    /// of different entries), in which case `rename_by_numeric_id` or `rename_by_variator_id`
    /// should be used after prompting the user.
    pub fn rename(&mut self, key: impl AsRef<str>, new_value: impl Into<String>) -> RenameOutcome {
        let key = key.as_ref();
        let new_value = new_value.into();

        let by_numeric = key
            .parse::<u32>()
            .ok()
            .and_then(|id| self.variators.iter().position(|v| v.id == id));
        let by_name = self.variators.iter().position(|v| v.variator_id == key);

        match (by_numeric, by_name) {
            (Some(ni), Some(vi)) if ni != vi => {
                // Ambiguous: numeric id and variator_id resolve to different entries
                RenameOutcome::Conflict(
                    self.variators[ni].clone(),
                    self.variators[vi].clone(),
                    new_value,
                )
            }
            (Some(pos), _) | (None, Some(pos)) => {
                self.variators[pos].variator = new_value;
                RenameOutcome::Renamed(self.variators[pos].clone())
            }
            (None, None) => RenameOutcome::NotFound,
        }
    }

    /// Rename a variator's value by its numeric id directly (no ambiguity check).
    pub fn rename_by_numeric_id(&mut self, id: u32, new_value: impl Into<String>) -> bool {
        if let Some(v) = self.variators.iter_mut().find(|v| v.id == id) {
            v.variator = new_value.into();
            true
        } else {
            false
        }
    }

    /// Clear all variators and reset the id counter
    pub fn clear_all(&mut self) {
        self.variators.clear();
        self.next_id = 1;
    }

    /// List all variators as (id, variator_id, variator) tuples
    pub fn list(&self) -> &Vec<Variator> {
        &self.variators
    }

    /// Check if storage is empty
    pub fn is_empty(&self) -> bool {
        self.variators.is_empty()
    }

    /// Expand variator references in a string
    /// Pattern: (!id) or (!variator_id)
    /// Must NOT match ! at start (breaking change)
    pub fn expand(&self, input: impl AsRef<str>) -> String {
        let input = input.as_ref();
        let mut result = input.to_string();

        // Find all occurrences of (!...) and expand them
        // We need to be careful not to expand if it's at the very start (breaking change)
        while let Some(start) = result.find("(!") {
            // Check if this is at the very beginning of the string (breaking change pattern)
            // Breaking change: "!feat: message" at position 0
            // Variator: "(!id)" anywhere in the string
            if start == 0 && !result.starts_with("(!") {
                // This is a breaking change pattern, skip
                break;
            }

            // Find the closing )
            if let Some(end) = result[start..].find(')') {
                let end = start + end;
                let key = &result[start + 2..end];

                if let Some(variator) = self.get(key) {
                    result.replace_range(start..=end, &variator.variator);
                } else {
                    // Variator not found, keep the original but move past it
                    // Replace with a marker to avoid infinite loop
                    result.replace_range(start..=start + 1, "__VRTR_OPEN__");
                }
            } else {
                // No closing ), stop processing
                break;
            }
        }

        // Restore any markers
        result = result.replace("__VRTR_OPEN__", "(!");

        result
    }
}

/// Help text for variator commands
pub const VARIATOR_HELP: &str = r#"Variator - Store and reuse commit message configurations

USAGE:
  cg var                      List all defined variators
  cg var see|list             Same as above
  cg var set <id> "<value>"   Define a new variator
  cg var clear <id>           Remove specific variator(s)
  cg var clear all            Remove all variators
  cg var help|-h|--help       Show this help

CALLING VARIATORS:
  Use (!id) or (!variator_id) in commit messages:
    @.feat(!tpaw): message    →  @.feat(Top Picks - Auto Wrap): message
    (!nftpaw): message        →  @.feat(Top Picks - Auto Wrap): message

EXAMPLES:
  cg var set tpaw "(Top Picks - Auto Wrap)"
  cg var set nftpaw "@.feat(Top Picks - Auto Wrap)"
  cg var clear tpaw
  cg var clear 1,2,3          (remove multiple)

NOTE:
  - Numeric IDs are auto-assigned (1, 2, 3...)
  - (!id) is different from ! at start (breaking change)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variator_storage_set_and_get() {
        let mut storage = VariatorStorage::new();

        let id1 = storage.set("tpaw", "(Top Picks - Auto Wrap)");
        assert_eq!(id1, 1);

        let id2 = storage.set("nftpaw", "@.feat(Top Picks - Auto Wrap)");
        assert_eq!(id2, 2);

        // Overwrite existing
        let id3 = storage.set("tpaw", "(Updated)");
        assert_eq!(id3, 1); // Same id

        let v = storage.get("1").unwrap();
        assert_eq!(v.variator, "(Updated)");

        let v = storage.get("tpaw").unwrap();
        assert_eq!(v.id, 1);
    }

    #[test]
    fn test_variator_storage_remove() {
        let mut storage = VariatorStorage::new();
        storage.set("a", "value a");
        storage.set("b", "value b");
        storage.set("c", "value c");

        let removed = storage.remove("1");
        assert_eq!(removed.len(), 1);
        assert_eq!(storage.variators.len(), 2);

        let removed = storage.remove("b");
        assert_eq!(removed.len(), 1);
        assert_eq!(storage.variators.len(), 1);
    }

    #[test]
    fn test_variator_storage_remove_multiple() {
        let mut storage = VariatorStorage::new();
        storage.set("a", "value a");
        storage.set("b", "value b");
        storage.set("c", "value c");

        let removed = storage.remove("1, b");
        assert_eq!(removed.len(), 2);
        assert_eq!(storage.variators.len(), 1);
    }

    #[test]
    fn test_variator_expand() {
        let mut storage = VariatorStorage::new();
        storage.set("tpaw", "(Top Picks - Auto Wrap)");
        storage.set("nftpaw", "@.feat(Top Picks - Auto Wrap)");

        let expanded = storage.expand("@.feat(!tpaw): message");
        assert_eq!(expanded, "@.feat(Top Picks - Auto Wrap): message");

        let expanded = storage.expand("(!nftpaw): message");
        assert_eq!(expanded, "@.feat(Top Picks - Auto Wrap): message");

        let expanded = storage.expand("(!1): message");
        assert_eq!(expanded, "(Top Picks - Auto Wrap): message");
    }

    #[test]
    fn test_variator_expand_unknown() {
        let storage = VariatorStorage::new();

        // Unknown variator should remain unchanged
        let expanded = storage.expand("@.feat(!unknown): message");
        assert_eq!(expanded, "@.feat(!unknown): message");
    }

    #[test]
    fn rename_by_variator_id_succeeds() {
        let mut storage = VariatorStorage::new();
        storage.set("feat", "old value");

        let outcome = storage.rename("feat", "new value");
        assert!(matches!(outcome, RenameOutcome::Renamed(ref v) if v.variator == "new value"));
        assert_eq!(storage.get("feat").unwrap().variator, "new value");
    }

    #[test]
    fn rename_by_numeric_id_succeeds() {
        let mut storage = VariatorStorage::new();
        let id = storage.set("feat", "old value");

        let outcome = storage.rename(id.to_string(), "new value");
        assert!(matches!(outcome, RenameOutcome::Renamed(ref v) if v.variator == "new value"));
    }

    #[test]
    fn rename_not_found_when_no_match() {
        let mut storage = VariatorStorage::new();
        storage.set("feat", "value");

        assert_eq!(storage.rename("nonexistent", "x"), RenameOutcome::NotFound);
        assert_eq!(storage.rename("99", "x"), RenameOutcome::NotFound);
    }

    #[test]
    fn rename_detects_conflict_between_numeric_id_and_variator_id() {
        let mut storage = VariatorStorage::new();
        storage.set("v", "first value"); // id=1
        storage.set("2", "second value"); // id=2 — variator_id is "2"
        storage.set("1", "third value"); // id=3 — variator_id is "1"

        // "1" matches id=1 (entry with variator_id="v") AND variator_id="1" (entry with id=3)
        let outcome = storage.rename("1", "new value");
        assert!(
            matches!(outcome, RenameOutcome::Conflict(ref by_id, ref by_name, _)
            if by_id.variator_id == "v" && by_name.variator_id == "1")
        );
    }

    #[test]
    fn rename_by_numeric_id_direct_resolves_conflict() {
        let mut storage = VariatorStorage::new();
        let id1 = storage.set("v", "first");
        let id3 = storage.set("1", "third");

        storage.rename_by_numeric_id(id1, "updated");

        // id1 corresponds to variator_id="v", which should be updated
        let v_entry = storage.variators.iter().find(|v| v.id == id1).unwrap();
        assert_eq!(v_entry.variator, "updated");

        // id3 corresponds to variator_id="1", which should be untouched
        let one_entry = storage.variators.iter().find(|v| v.id == id3).unwrap();
        assert_eq!(one_entry.variator, "third");
    }
}
