// Copyright © 2026 ComfyHome™
// All rights reserved.
//
// Licensed under the ComfyVersionBumper License v1.2
//
// For details, see the LICENSE file in the repository root.

use std::{
	collections::HashSet,
	fs,
	path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Local;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const COMFYGIT_DIR: &str = ".comfygit";
const SYNCMEM_DIR: &str = "syncmem";
const STD_CHANGELOG_FILE: &str = "stdchlg.json";
const STD_CHANGELOG_LOCAL_FILE: &str = "stdchlg-local.json";

pub(crate) fn syncmem_dir_path(repo_root: &str) -> PathBuf {
	Path::new(repo_root).join(COMFYGIT_DIR).join(SYNCMEM_DIR)
}

pub(crate) fn ensure_syncmem_dir(repo_root: &str) -> Result<PathBuf> {
	let path = syncmem_dir_path(repo_root);
	fs::create_dir_all(&path).with_context(|| format!("failed to create {}", path.display()))?;
	Ok(path)
}

pub(crate) fn std_changelog_memory_path(repo_root: &str, local: bool) -> PathBuf {
	syncmem_dir_path(repo_root).join(if local {
		STD_CHANGELOG_LOCAL_FILE
	} else {
		STD_CHANGELOG_FILE
	})
}

pub(crate) fn load_json_or_default<T>(path: &Path) -> Result<T>
where
	T: DeserializeOwned + Default,
{
	if !path.is_file() {
		return Ok(T::default());
	}

	let content = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
	if content.trim().is_empty() {
		return Ok(T::default());
	}

	serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

pub(crate) fn write_json_pretty<T>(path: &Path, value: &T) -> Result<()>
where
	T: Serialize,
{
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
	}

	let rendered = serde_json::to_string_pretty(value).context("failed to serialize JSON memory")?;
	fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))?;
	Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum StdChangelogGeneratedState {
	False,
	True,
	Postponed,
	Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StdChangelogMemoryEntry {
	pub(crate) tag_from: String,
	pub(crate) tag_origin: String,
	pub(crate) tag_to: Option<String>,
	pub(crate) generated: StdChangelogGeneratedState,
	pub(crate) generated_error: Option<String>,
	pub(crate) ts: String,
}

impl StdChangelogMemoryEntry {
	pub(crate) fn new(tag_from: impl Into<String>, tag_origin: impl Into<String>) -> Self {
		Self {
			tag_from: tag_from.into(),
			tag_origin: tag_origin.into(),
			tag_to: None,
			generated: StdChangelogGeneratedState::False,
			generated_error: None,
			ts: sync_timestamp_now(),
		}
	}

	pub(crate) fn mark_generated(&mut self, tag_to: impl Into<String>) {
		self.tag_to = Some(tag_to.into());
		self.generated = StdChangelogGeneratedState::True;
		self.generated_error = None;
		self.ts = sync_timestamp_now();
	}

	pub(crate) fn mark_postponed(&mut self) {
		self.generated = StdChangelogGeneratedState::Postponed;
		self.generated_error = None;
		self.ts = sync_timestamp_now();
	}

	pub(crate) fn mark_error(&mut self, reason: impl Into<String>) {
		self.generated = StdChangelogGeneratedState::Error;
		self.generated_error = Some(reason.into());
		self.ts = sync_timestamp_now();
	}

	fn dedupe_key(&self) -> String {
		format!("{}::{}", self.tag_from, self.tag_origin)
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StdChangelogMemory {
	pub(crate) entries: Vec<StdChangelogMemoryEntry>,
}

impl StdChangelogMemory {
	pub(crate) fn prepend(&mut self, entry: StdChangelogMemoryEntry) {
		self.entries.retain(|existing| existing.dedupe_key() != entry.dedupe_key());
		self.entries.insert(0, entry);
		self.entries.sort_by(|left, right| right.ts.cmp(&left.ts));
	}
}

pub(crate) fn load_std_changelog_memory(repo_root: &str, local: bool) -> Result<StdChangelogMemory> {
	load_json_or_default(&std_changelog_memory_path(repo_root, local))
}

pub(crate) fn save_std_changelog_memory(repo_root: &str, local: bool, memory: &StdChangelogMemory) -> Result<PathBuf> {
	ensure_syncmem_dir(repo_root)?;
	let path = std_changelog_memory_path(repo_root, local);
	write_json_pretty(&path, memory)?;
	Ok(path)
}

pub(crate) fn load_merged_std_changelog_memory(repo_root: &str) -> Result<StdChangelogMemory> {
	let shared = load_std_changelog_memory(repo_root, false)?;
	let mut local = load_std_changelog_memory(repo_root, true)?;
	if merge_std_changelog_memories(&mut local, &shared) {
		save_std_changelog_memory(repo_root, true, &local)?;
	}
	Ok(local)
}

pub(crate) fn merge_std_changelog_memories(local: &mut StdChangelogMemory, incoming: &StdChangelogMemory) -> bool {
	let before = local.clone();
	let mut seen = local
		.entries
		.iter()
		.map(StdChangelogMemoryEntry::dedupe_key)
		.collect::<HashSet<_>>();

	for entry in &incoming.entries {
		if seen.insert(entry.dedupe_key()) {
			local.entries.push(entry.clone());
		}
	}

	local.entries.sort_by(|left, right| right.ts.cmp(&left.ts));
	*local != before
}

pub(crate) fn record_std_changelog_created(repo_root: &str, tag_name: &str, branch_name: &str) -> Result<()> {
	apply_std_changelog_entry_both(repo_root, tag_name, branch_name, |entry| {
		entry.tag_to = None;
		entry.generated = StdChangelogGeneratedState::False;
		entry.generated_error = None;
		entry.ts = sync_timestamp_now();
	})
}

pub(crate) fn record_std_changelog_generated(repo_root: &str, tag_name: &str, branch_name: &str) -> Result<()> {
	apply_std_changelog_entry_both(repo_root, tag_name, branch_name, |entry| {
		entry.mark_generated(tag_name.to_string());
	})
}

pub(crate) fn record_std_changelog_postponed(repo_root: &str, tag_name: &str, branch_name: &str) -> Result<()> {
	apply_std_changelog_entry_both(repo_root, tag_name, branch_name, |entry| {
		entry.mark_postponed();
	})
}

pub(crate) fn record_std_changelog_error(
	repo_root: &str,
	tag_name: &str,
	branch_name: &str,
	reason: &str,
) -> Result<()> {
	apply_std_changelog_entry_both(repo_root, tag_name, branch_name, |entry| {
		entry.mark_error(reason.to_string());
	})
}

fn apply_std_changelog_entry_both(
	repo_root: &str,
	tag_name: &str,
	branch_name: &str,
	mutate: impl Fn(&mut StdChangelogMemoryEntry),
) -> Result<()> {
	apply_std_changelog_entry(repo_root, false, tag_name, branch_name, &mutate)?;
	apply_std_changelog_entry(repo_root, true, tag_name, branch_name, &mutate)?;
	Ok(())
}

fn apply_std_changelog_entry(
	repo_root: &str,
	local: bool,
	tag_name: &str,
	branch_name: &str,
	mutate: &impl Fn(&mut StdChangelogMemoryEntry),
) -> Result<PathBuf> {
	let mut memory = load_std_changelog_memory(repo_root, local)?;
	let key = format!("{}::{}", tag_name.trim(), branch_name.trim());
	if let Some(entry) = memory.entries.iter_mut().find(|entry| entry.dedupe_key() == key) {
		mutate(entry);
	} else {
		let mut entry = StdChangelogMemoryEntry::new(tag_name.trim().to_string(), branch_name.trim().to_string());
		mutate(&mut entry);
		memory.prepend(entry);
	}
	save_std_changelog_memory(repo_root, local, &memory)
}

fn sync_timestamp_now() -> String {
	Local::now().format("%Y%m%d-%H%M%S-%3f").to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn std_changelog_memory_paths_target_syncmem_directory() {
		let repo_root = std::env::temp_dir().join("cvb-mmr-path-test");
		let shared = std_changelog_memory_path(&repo_root.display().to_string(), false);
		let local = std_changelog_memory_path(&repo_root.display().to_string(), true);

		assert!(shared.ends_with(Path::new(".comfygit").join("syncmem").join("stdchlg.json")));
		assert!(local.ends_with(Path::new(".comfygit").join("syncmem").join("stdchlg-local.json")));
	}

	#[test]
	fn save_and_load_std_changelog_memory_round_trips() {
		let repo_root = std::env::temp_dir().join(format!(
			"cvb-mmr-roundtrip-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos()
		));
		let repo_root_string = repo_root.display().to_string();
		let mut memory = StdChangelogMemory::default();
		let mut entry = StdChangelogMemoryEntry::new("v0.7.3", "main");
		entry.mark_generated("v0.7.4");
		memory.prepend(entry);

		save_std_changelog_memory(&repo_root_string, false, &memory).expect("memory should save");
		let loaded = load_std_changelog_memory(&repo_root_string, false).expect("memory should load");

		assert_eq!(loaded, memory);
		let _ = std::fs::remove_dir_all(repo_root);
	}

	#[test]
	fn merge_std_changelog_memories_prefers_local_and_appends_unknown_entries() {
		let mut postponed = StdChangelogMemoryEntry::new("v0.7.3", "branch-a");
		postponed.mark_postponed();
		postponed.ts = "20260416-100000-001".to_string();

		let mut errored = StdChangelogMemoryEntry::new("v0.7.1", "branch-b");
		errored.mark_error("tag range was empty");
		assert_eq!(errored.generated, StdChangelogGeneratedState::Error);
		assert_eq!(errored.generated_error.as_deref(), Some("tag range was empty"));

		let mut local = StdChangelogMemory {
			entries: vec![postponed],
		};
		let incoming = StdChangelogMemory {
			entries: vec![
				StdChangelogMemoryEntry {
					tag_from: "v0.7.2".to_string(),
					tag_origin: "main".to_string(),
					tag_to: Some("v0.7.3".to_string()),
					generated: StdChangelogGeneratedState::True,
					generated_error: None,
					ts: "20260416-110000-001".to_string(),
				},
				StdChangelogMemoryEntry {
					tag_from: "v0.7.3".to_string(),
					tag_origin: "branch-a".to_string(),
					tag_to: Some("v0.7.4".to_string()),
					generated: StdChangelogGeneratedState::True,
					generated_error: None,
					ts: "20260416-120000-001".to_string(),
				},
			],
		};

		let changed = merge_std_changelog_memories(&mut local, &incoming);

		assert!(changed);
		assert_eq!(local.entries.len(), 2);
		assert_eq!(local.entries[0].tag_from, "v0.7.2");
		assert_eq!(local.entries[1].generated, StdChangelogGeneratedState::Postponed);
	}

	#[test]
	fn record_std_changelog_state_updates_shared_and_local_memories() {
		let repo_root = std::env::temp_dir().join(format!(
			"cvb-mmr-record-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos()
		));
		let repo_root_string = repo_root.display().to_string();

		record_std_changelog_created(&repo_root_string, "v0.7.3", "main").expect("created state should save");
		record_std_changelog_generated(&repo_root_string, "v0.7.3", "main").expect("generated state should save");

		for local in [false, true] {
			let loaded = load_std_changelog_memory(&repo_root_string, local).expect("memory should load");
			assert_eq!(loaded.entries.len(), 1);
			assert_eq!(loaded.entries[0].tag_from, "v0.7.3");
			assert_eq!(loaded.entries[0].tag_origin, "main");
			assert_eq!(loaded.entries[0].tag_to.as_deref(), Some("v0.7.3"));
			assert_eq!(loaded.entries[0].generated, StdChangelogGeneratedState::True);
		}

		let _ = std::fs::remove_dir_all(repo_root);
	}

	#[test]
	fn record_std_changelog_postponed_updates_both_memories() {
		let repo_root = std::env::temp_dir().join(format!(
			"cvb-mmr-postponed-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos()
		));
		let repo_root_string = repo_root.display().to_string();

		record_std_changelog_created(&repo_root_string, "v0.7.4", "feature-a").expect("created state should save");
		record_std_changelog_postponed(&repo_root_string, "v0.7.4", "feature-a").expect("postponed state should save");

		for local in [false, true] {
			let loaded = load_std_changelog_memory(&repo_root_string, local).expect("memory should load");
			assert_eq!(loaded.entries[0].generated, StdChangelogGeneratedState::Postponed);
		}

		let _ = std::fs::remove_dir_all(repo_root);
	}
}