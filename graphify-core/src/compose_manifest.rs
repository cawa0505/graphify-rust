//! Assembly Manifest loading + semantic validation for `graphify compose`.
//!
//! Never fail-fast: collects ALL validation errors and reports them together.
//! Never silently skips — every declared workspace/relation endpoint resolves
//! or the whole command fails (per spec: no silent fallbacks).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::manifest::{AssemblyManifest, WorkspaceEntry, split_node_reference};
use crate::{GraphOutput, NodeId};
use anyhow::{Context, anyhow};

/// A parsed manifest with resolved absolute workspace root paths.
#[derive(Debug, Clone)]
pub struct LoadedManifest {
    pub manifest: AssemblyManifest,
    /// `workspace id -> absolute workspace root`
    pub roots: Vec<(String, PathBuf)>,
    /// Absolute path of the manifest file itself (for provenance fields).
    pub manifest_path: PathBuf,
}

impl LoadedManifest {
    /// Looks up the workspace root for a declared workspace id.
    #[must_use]
    pub fn root_of(&self, ws_id: &str) -> Option<&Path> {
        self.roots
            .iter()
            .find(|(id, _)| id == ws_id)
            .map(|(_, root)| root.as_path())
    }
}

// ponytail: allow too_many_lines — collecting every error class in one pass is
// naturally branchy; splitting would scatter the error accounting.
#[allow(clippy::too_many_lines)]
pub fn load_manifest(manifest_path: &Path) -> anyhow::Result<LoadedManifest> {
    let manifest_path = manifest_path.canonicalize().map_err(|e| {
        anyhow!(
            "manifest 檔案不存在或無法讀取: {}: {e}",
            manifest_path.display()
        )
    })?;
    let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("讀取 manifest 失敗: {}", manifest_path.display()))?;
    let manifest: AssemblyManifest = serde_yaml_ng::from_str(&content)
        .map_err(|e| anyhow!("manifest YAML 解析失敗 ({}): {e}", manifest_path.display()))?;
    let base_dir = manifest_path
        .parent()
        .map_or_else(|| PathBuf::from("."), std::path::Path::to_path_buf);

    let mut errors: Vec<String> = Vec::new();

    // (a) id format + (b) duplicate ids
    let mut seen_ids: HashSet<String> = HashSet::new();
    for ws in &manifest.workspaces {
        if let Err(e) = ws.validate_id() {
            errors.push(format!("workspace `{}`: {e}", ws.id));
        }
        if !seen_ids.insert(ws.id.clone()) {
            errors.push(format!("workspace `{}`: id 重複宣告", ws.id));
        }
    }

    // (c) workspace path exists + contains a graph output
    for ws in &manifest.workspaces {
        let root = base_dir.join(&ws.path);
        let graph_toon = root.join("graphify-out/graph.toon");
        let graph_json = root.join("graphify-out/graph.json");
        if !graph_toon.is_file() && !graph_json.is_file() {
            errors.push(format!(
                "workspace `{}`: 路徑 `{}` 下找不到 graphify-out/graph.toon 或 graph.json（先在該 workspace 執行 `graphify index`）",
                ws.id,
                root.display()
            ));
        }
    }

    // (d) relation endpoints reference declared workspaces
    for rel in &manifest.relations {
        for (role, endpoint) in [("from", &rel.from), ("to", &rel.to)] {
            let mut ws_part_owned = String::new();
            let ws_part: &str =
                split_node_reference(endpoint).map_or(endpoint.as_str(), |(ws, _)| {
                    // `ws` is owned by the closure; leak-free alternative is to
                    // clone the ws part into a String and take a slice of it.
                    ws_part_owned = ws;
                    ws_part_owned.as_str()
                });
            if !seen_ids.contains(ws_part) {
                errors.push(format!(
                    "relation {role}=`{endpoint}`: 未宣告的 workspace `{ws_part}`"
                ));
            }
        }
    }

    // (e) node-granularity references must exist in the referenced workspace graph
    let node_refs_needed: Vec<(String, String)> = manifest
        .relations
        .iter()
        .flat_map(|rel| [rel.from.clone(), rel.to.clone()])
        .filter_map(|ep| split_node_reference(&ep))
        .collect();
    if !node_refs_needed.is_empty() {
        // load each referenced workspace's graph at most once
        let mut graphs: Vec<(String, GraphOutput)> = Vec::new();
        for (ws_id, _node_ref) in &node_refs_needed {
            if graphs.iter().any(|(id, _)| id == ws_id) {
                continue;
            }
            let Some(root) = roots(manifest.workspaces.as_slice(), &base_dir)
                .into_iter()
                .find(|(id, _)| id == ws_id)
                .map(|(_, root)| root)
            else {
                // ws-level validation already reported the unknown workspace
                continue;
            };
            match load_toon_or_json(&root) {
                Ok(graph) => graphs.push((ws_id.clone(), graph)),
                Err(e) => errors.push(format!("workspace `{ws_id}`: {e}")),
            }
        }
        for (ws_id, node_ref) in &node_refs_needed {
            let Some((_, graph)) = graphs.iter().find(|(id, _)| id == ws_id) else {
                continue;
            };
            let exists = graph.nodes.iter().any(|n| &n.id.0 == node_ref);
            if !exists {
                errors.push(format!(
                    "節點引用 `{ws_id}::{node_ref}` 在 workspace `{ws_id}` 的 graph 中不存在"
                ));
            }
        }
    }

    let resolved_roots = roots(manifest.workspaces.as_slice(), &base_dir);
    if !errors.is_empty() {
        return Err(anyhow!(
            "manifest 驗證失敗（{} 項）:\n{}",
            errors.len(),
            errors
                .iter()
                .map(|e| format!("  - {e}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    Ok(LoadedManifest {
        manifest,
        roots: resolved_roots,
        manifest_path,
    })
}

/// Resolves every declared workspace id to its absolute root. Only valid after
/// path checks pass; callers must not rely on it for validation.
fn roots(workspaces: &[WorkspaceEntry], base_dir: &Path) -> Vec<(String, PathBuf)> {
    workspaces
        .iter()
        .map(|ws| (ws.id.clone(), base_dir.join(&ws.path)))
        .collect()
}

/// Loads a workspace's graph: prefers `.toon`, falls back to legacy `.json`.
pub fn load_toon_or_json(ws_root: &Path) -> anyhow::Result<GraphOutput> {
    let toon_path = ws_root.join("graphify-out/graph.toon");
    let json_path = ws_root.join("graphify-out/graph.json");
    if toon_path.is_file() {
        let content = std::fs::read_to_string(&toon_path)
            .with_context(|| format!("讀取 graph.toon 失敗: {}", toon_path.display()))?;
        return crate::from_toon(&content)
            .map_err(|e| anyhow!("解析 graph.toon 失敗 ({}): {e}", toon_path.display()));
    }
    if json_path.is_file() {
        let content = std::fs::read_to_string(&json_path)
            .with_context(|| format!("讀取 graph.json 失敗: {}", json_path.display()))?;
        return serde_json::from_str::<GraphOutput>(&content).map_err(|e| {
            // Legacy partial JSON: surface a precise error instead of guessing.
            anyhow!(
                "解析 graph.json 失敗 ({}): {e}（舊 schema 不支援，請在該 workspace 重新執行 `graphify index` 產生 .toon）",
                json_path.display()
            )
        });
    }
    Err(anyhow!(
        "找不到 graph output: {} 或 {}",
        toon_path.display(),
        json_path.display()
    ))
}

/// Finds a node's display label inside a workspace graph (for error messages).
#[must_use]
pub fn node_exists(graph: &GraphOutput, id: &NodeId) -> bool {
    graph.nodes.iter().any(|n| &n.id == id)
}

/// Validates YAML content then writes it atomically to `manifest_path`.
///
/// On any failure the target file is left byte-for-byte unchanged.
/// Semantic validation runs against the directory the new file lives in.
///
/// # Errors
/// Fails on YAML parse errors, or when semantic validation of the written
/// path fails. The file is never partially written.
pub fn write_manifest_atomic(manifest_path: &Path, content: &str) -> anyhow::Result<()> {
    // Parse first — a YAML syntax error leaves the disk untouched.
    let _manifest: AssemblyManifest =
        serde_yaml_ng::from_str(content).map_err(|e| anyhow!("manifest YAML 解析失敗: {e}"))?;

    // Resolve the target directory (create it if missing) so load_manifest
    // can validate workspace paths relative to the manifest's future home.
    if let Some(parent) = manifest_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("建立 manifest 目錄失敗: {}", parent.display()))?;
        }
    }

    // Stage the validated content in a sibling temp file, then rename.
    // Write-to-temp-then-rename keeps the target byte-for-byte unchanged on
    // any I/O failure (rename within the same directory is atomic on POSIX).
    let staged = manifest_path.with_extension("yaml.tmp");
    std::fs::write(&staged, content)
        .with_context(|| format!("寫入暫存檔失敗: {}", staged.display()))?;

    // Semantic validation against the staged file (relative workspace paths
    // resolve against its parent = the manifest's parent).
    if let Err(e) = load_manifest(&staged) {
        drop(std::fs::remove_file(&staged));
        return Err(anyhow!("manifest 語意驗證失敗，原檔未變動: {e}"));
    }

    std::fs::rename(&staged, manifest_path)
        .with_context(|| format!("原子寫入 manifest 失敗: {}", manifest_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_toon_or_json_missing_graphs() {
        let dir = std::env::temp_dir().join("graphify-compose-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok();
        assert!(load_toon_or_json(&dir).is_err());
    }
}
