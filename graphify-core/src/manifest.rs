//! Assembly Manifest shared data types (workspace-compose).
//!
//! A manifest is an "assembly instruction sheet": it declares which existing
//! workspaces to compose and the semantic relations between them. It is NOT
//! the single source of truth of the system — multiple manifests may coexist
//! over the same set of workspaces, each describing a different view.
//!
//! These are pure data types with `serde` derives only: no YAML parser lives
//! in graphify-core (see design.md D6 — parsing happens in graphify-cli and
//! graphify-mcp via `serde_yaml_ng`).

use serde::{Deserialize, Serialize};

/// One workspace entry in an assembly manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEntry {
    /// Short identifier used in relations and node references. Must not
    /// contain `::` (reserved separator for node-granularity references).
    pub id: String,
    /// Filesystem path to the workspace root (relative paths resolve against
    /// the manifest file's directory).
    pub path: String,
}

/// One cross-workspace relation. Endpoints are workspace ids, optionally with
/// node granularity using the `ws-id::node_id` reference syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationEntry {
    /// Source endpoint: workspace id or `ws-id::node_id`.
    pub from: String,
    /// Semantic relation type (e.g. `uses`, `audits`, `executes_via`).
    #[serde(rename = "type")]
    pub relation: String,
    /// Target endpoint: workspace id or `ws-id::node_id`.
    pub to: String,
}

/// Assembly Manifest: the composition instruction sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssemblyManifest {
    #[serde(default)]
    pub workspaces: Vec<WorkspaceEntry>,
    #[serde(default)]
    pub relations: Vec<RelationEntry>,
}

/// Error for invalid workspace ids (e.g. containing the `::` separator).
#[derive(Debug, thiserror::Error)]
#[error("invalid workspace id `{id}`: {reason}")]
pub struct ManifestIdError {
    pub id: String,
    pub reason: String,
}

impl WorkspaceEntry {
    /// Validates the workspace id format: non-empty and free of `::`.
    ///
    /// # Errors
    /// Returns [`ManifestIdError`] when the id is empty or contains `::`.
    pub fn validate_id(&self) -> Result<(), ManifestIdError> {
        if self.id.is_empty() {
            return Err(ManifestIdError {
                id: self.id.clone(),
                reason: "id must not be empty".to_string(),
            });
        }
        if self.id.contains("::") {
            return Err(ManifestIdError {
                id: self.id.clone(),
                reason: "id must not contain `::` (reserved for node references)".to_string(),
            });
        }
        Ok(())
    }
}

/// Splits a node-granularity reference `ws-id::node_id` into its parts.
/// Returns `None` when the reference has no `::` separator (plain workspace
/// id) or more than one separator.
#[must_use]
pub fn split_node_reference(reference: &str) -> Option<(String, String)> {
    let (ws, node) = reference.split_once("::")?;
    if node.contains("::") || ws.is_empty() || node.is_empty() {
        return None;
    }
    Some((ws.to_string(), node.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> AssemblyManifest {
        AssemblyManifest {
            workspaces: vec![
                WorkspaceEntry {
                    id: "agentshop".to_string(),
                    path: "./AgentShop".to_string(),
                },
                WorkspaceEntry {
                    id: "nexusledger".to_string(),
                    path: "./NexusLedger".to_string(),
                },
            ],
            relations: vec![RelationEntry {
                from: "agentshop".to_string(),
                relation: "uses".to_string(),
                to: "nexusledger".to_string(),
            }],
        }
    }

    // ponytail: let-else + bail instead of expect() per clippy pedantic in tests (AGENTS.md #3193)
    fn roundtrip_json(manifest: &AssemblyManifest) -> AssemblyManifest {
        let json = match serde_json::to_string(manifest) {
            Ok(s) => s,
            Err(e) => panic!("serialize manifest failed: {e}"),
        };
        match serde_json::from_str(&json) {
            Ok(m) => m,
            Err(e) => panic!("deserialize manifest failed: {e}"),
        }
    }

    #[test]
    fn test_manifest_types_roundtrip() {
        let manifest = sample_manifest();
        assert_eq!(manifest, roundtrip_json(&manifest));
    }

    #[test]
    fn test_manifest_yaml_shape_roundtrip() {
        // YAML shape check without a YAML parser in core: serde_json proves
        // the derive shape; the actual YAML roundtrip lives in graphify-cli.
        let manifest = sample_manifest();
        let json = match serde_json::to_value(&manifest) {
            Ok(v) => v,
            Err(e) => panic!("to value failed: {e}"),
        };
        let Some(workspaces) = json.get("workspaces").and_then(serde_json::Value::as_array) else {
            panic!("workspaces array missing");
        };
        assert_eq!(workspaces.len(), 2);
        let Some(first) = workspaces.first() else {
            panic!("first workspace missing");
        };
        assert_eq!(
            first.get("id").and_then(serde_json::Value::as_str),
            Some("agentshop")
        );
        let Some(relations) = json.get("relations").and_then(serde_json::Value::as_array) else {
            panic!("relations array missing");
        };
        let Some(rel) = relations.first() else {
            panic!("first relation missing");
        };
        // `type` is the serialized field name, not `relation`.
        assert_eq!(
            rel.get("type").and_then(serde_json::Value::as_str),
            Some("uses")
        );
    }

    #[test]
    fn test_workspace_id_rejects_double_colon() {
        let entry = WorkspaceEntry {
            id: "bad::id".to_string(),
            path: "./x".to_string(),
        };
        assert!(entry.validate_id().is_err());
        let entry = WorkspaceEntry {
            id: String::new(),
            path: "./x".to_string(),
        };
        assert!(entry.validate_id().is_err());
        let ok = WorkspaceEntry {
            id: "agentshop".to_string(),
            path: "./x".to_string(),
        };
        assert!(ok.validate_id().is_ok());
    }

    #[test]
    fn test_split_node_reference() {
        assert_eq!(
            split_node_reference("agentshop::src/main.rs:42"),
            Some(("agentshop".to_string(), "src/main.rs:42".to_string()))
        );
        assert_eq!(split_node_reference("agentshop"), None);
        assert_eq!(split_node_reference("a::b::c"), None);
        assert_eq!(split_node_reference("::node"), None);
        assert_eq!(split_node_reference("ws::"), None);
    }
}
