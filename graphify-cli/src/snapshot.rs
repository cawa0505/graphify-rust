//! SHA256 snapshot registry for incremental indexing.
//!
//! The snapshot maps source-file paths (identical to `node.source_file` strings)
//! to their SHA256 digests. `run_index` diffs the live tree against the last
//! snapshot so unchanged files skip extraction/embedding/upsert entirely.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Recursively collects source file paths, reusing the extraction walk so the
/// returned keys match `node.source_file` exactly.
pub fn collect_source_files(dir: &Path, file_paths: &mut Vec<std::path::PathBuf>) -> Result<()> {
    // ponytail: same walk as extraction (collect_files) — see graphify-cli main.rs
    crate::collect_files(dir, file_paths)
}

/// Computes a `path -> sha256` map for every source file under `dir`.
pub fn compute_file_hashes(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut file_paths = Vec::new();
    collect_source_files(dir, &mut file_paths)?;

    let mut hashes = BTreeMap::new();
    for path in file_paths {
        let key = path.to_string_lossy().to_string();
        let bytes = std::fs::read(&path)
            .with_context(|| format!("Failed to read {} for snapshot hashing", path.display()))?;
        let digest = Sha256::digest(&bytes);
        hashes.insert(key, format!("{digest:x}"));
    }
    Ok(hashes)
}

/// Loads the snapshot from disk; a missing or corrupt snapshot yields an empty map.
#[must_use]
pub fn load_snapshot(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persists the snapshot atomically (write temp + rename).
pub fn save_snapshot(path: &Path, hashes: &BTreeMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(hashes)?;
    let tmp = path.with_extension("snapshot.tmp");
    std::fs::write(&tmp, json).with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("Failed to rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Returns the set of file keys that are new, modified, or deleted relative to `old`.
#[must_use]
pub fn diff_hashes(old: &BTreeMap<String, String>, current: &BTreeMap<String, String>) -> HashSet<String> {
    let mut changed = HashSet::new();

    // new or modified files
    for (path, digest) in current {
        if old.get(path) != Some(digest) {
            changed.insert(path.clone());
        }
    }

    // deleted files
    for path in old.keys() {
        if !current.contains_key(path) {
            changed.insert(path.clone());
        }
    }

    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_detects_new_modified_deleted() {
        let mut old = BTreeMap::new();
        old.insert("a.rs".to_string(), "aa".to_string());
        old.insert("b.rs".to_string(), "bb".to_string());
        old.insert("gone.rs".to_string(), "gg".to_string());

        let mut current = BTreeMap::new();
        current.insert("a.rs".to_string(), "aa".to_string()); // unchanged
        current.insert("b.rs".to_string(), "bb2".to_string()); // modified
        current.insert("new.rs".to_string(), "nn".to_string()); // new

        let changed = diff_hashes(&old, &current);
        assert_eq!(changed.len(), 3);
        assert!(changed.contains("b.rs"));
        assert!(changed.contains("new.rs"));
        assert!(changed.contains("gone.rs"));
        assert!(!changed.contains("a.rs"));
    }
}
