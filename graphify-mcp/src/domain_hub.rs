//! Domain Hub aggregation (openspec/changes/mcp-domain-hub).
//!
//! 33 legacy tools collapse into 6 domain entry points; each call carries an
//! `action` argument that maps 1:1 onto a legacy tool name. The dispatcher
//! rewrites hub calls into legacy names and lets the existing `tools/call`
//! match arms run unchanged — the legacy arms ARE the compatibility layer.

/// One domain entry point: hub tool name, description, and its actions.
struct Domain {
    tool: &'static str,
    desc: &'static str,
    /// (action, legacy tool name, action description)
    actions: &'static [(&'static str, &'static str, &'static str)],
}

const DOMAINS: &[Domain] = &[
    Domain {
        tool: "graphify_graph",
        desc: "Knowledge graph core operations: BFS query, topology summary, shortest path, node probe, file reindex, AST skeleton, workspace status, semantic memory query",
        actions: &[
            ("query", "graphify_graph_query", "BFS traversal of the knowledge graph (params: question)"),
            ("summary", "graphify_graph_summary", "High-level topology summary (no params)"),
            ("reindex", "graphify_graph_reindex", "Reindex a file into the graph (params: file_path)"),
            ("path", "graphify_graph_path", "Shortest path between two nodes (params: source, target)"),
            ("query_node", "graphify_graph_query_node", "Query nodes by ID with depth (params: node_id, depth)"),
            ("skeleton", "graphify_skeleton_extract", "Extract compact AST skeleton from a source file (params: path)"),
            ("workspace", "graphify_workspace_status", "Active workspace key, root path, registration status (no params)"),
            ("memory", "graphify_memory_query", "Semantic memory query (params: query, workspace_key, limit)"),
        ],
    },
    Domain {
        tool: "graphify_relay",
        desc: "Code Relay cross-session / cross-repo state handoff management",
        actions: &[
            ("status", "graphify_relay_status", "Show relay summary: repos, active baton, spec drift (params: path)"),
            ("init", "graphify_relay_init", "Initialize relay.json (params: path, project_context, kind)"),
            ("save", "graphify_relay_save", "Save volatile state, phase, confidence, next-step (params: path, repo, phase, conf, next, volatile, ...)"),
            ("close", "graphify_relay_close", "Auto-save + closing ritual (params: path, repo, next, ...)"),
            ("switch", "graphify_relay_switch", "Pass the baton to another repo (params: path, repo, kind)"),
            ("resume", "graphify_relay_resume", "Render RESUME handover (params: path, repo, kind)"),
            ("add", "graphify_relay_add", "Ingest an old TODO/handoff doc (params: path, file, repo)"),
        ],
    },
    Domain {
        tool: "graphify_opendoc",
        desc: "Spec ↔ symbol binding and doc-drift audit (OpenDoc Layer 1)",
        actions: &[
            ("get_context", "graphify_opendoc_get_context", "Spec blocks documenting a code symbol (params: symbol)"),
            ("index", "graphify_opendoc_index", "Index .md spec blocks in the workspace (params: doc_paths[])"),
            ("audit_drift", "graphify_opendoc_audit_drift", "Audit doc-side drift per indexed link (no params)"),
        ],
    },
    Domain {
        tool: "graphify_review",
        desc: "Code review points bound to graph nodes (code-review-graph bridge)",
        actions: &[
            ("get_context", "graphify_review_get_context", "Unresolved reviews for a canonical node id (params: node)"),
            ("ingest", "graphify_review_ingest", "Import a CRG IngestPayload JSON file (params: payload)"),
            ("resolve", "graphify_review_resolve", "Mark a review resolved (params: review_id, reason)"),
            ("search_crg", "graphify_review_search_crg", "Bind CRG top-risk changed functions as review points (params: base)"),
        ],
    },
    Domain {
        tool: "graphify_metrics",
        desc: "Telemetry and test-coverage metrics bound to graph nodes",
        actions: &[
            ("coverage_ingest", "graphify_coverage_ingest", "Import LCOV/cobertura coverage text (params: format, data)"),
            ("coverage_get", "graphify_coverage_get_context", "Coverage bindings for a node (params: node)"),
            ("coverage_blindspots", "graphify_coverage_blindspots", "List nodes with <50% coverage (no params)"),
            ("telemetry_ingest", "graphify_telemetry_ingest", "Import telemetry metrics (params: source, path_or_draco_params)"),
            ("telemetry_get", "graphify_telemetry_get_context", "Telemetry bindings for a node (params: node, include_impact_radius)"),
        ],
    },
    Domain {
        tool: "graphify_compose",
        desc: "Cross-workspace architecture composition (Assembly Manifest)",
        actions: &[
            ("read", "graphify_compose_read", "Read and validate an Assembly Manifest (params: manifest)"),
            ("write", "graphify_compose_write", "Atomically write a validated manifest (params: manifest, content)"),
            ("render", "graphify_compose_render", "Render the unified graph as ASCII/SVG (params: manifest, svg)"),
        ],
    },
];

/// Build the `tools/list` entry for one domain hub tool.
fn domain_tool_json(domain: &Domain) -> serde_json::Value {
    let actions: Vec<serde_json::Value> = domain
        .actions
        .iter()
        .map(|(action, _, action_desc)| {
            serde_json::json!({ "const": action, "description": action_desc })
        })
        .collect();
    serde_json::json!({
        "name": domain.tool,
        "description": domain.desc,
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": domain.actions.iter().map(|(a, _, _)| *a).collect::<Vec<_>>(),
                    "description": "Operation to run within this domain",
                },
                "params": {
                    "type": "object",
                    "description": "Action-specific arguments, forwarded verbatim to the underlying tool",
                    "oneOf": actions,
                },
            },
            "required": ["action"],
        },
    })
}

/// All 6 domain hub tool declarations for `tools/list`.
pub fn domain_tools() -> Vec<serde_json::Value> {
    DOMAINS.iter().map(domain_tool_json).collect()
}

/// Resolve a hub tool call: `(hub tool name, arguments)` →
/// `Ok(Some((legacy tool name, legacy arguments)))` when the action is valid,
/// `Ok(None)` when the name is not a hub tool (legacy passthrough),
/// `Err(message)` on missing/unknown action (spec 需求 2: list valid actions).
pub fn resolve_hub_call(
    tool_name: &str,
    arguments: &serde_json::Value,
) -> std::result::Result<Option<(&'static str, serde_json::Value)>, String> {
    let Some(domain) = DOMAINS.iter().find(|d| d.tool == tool_name) else {
        return Ok(None);
    };
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!(
                "missing 'action' for {tool_name}; valid actions: {}",
                domain.actions.iter().map(|(a, _, _)| *a).collect::<Vec<_>>().join(", ")
            )
        })?;
    let (_, legacy, _) = domain
        .actions
        .iter()
        .find(|(a, _, _)| *a == action)
        .ok_or_else(|| {
            format!(
                "unknown action '{action}' for {tool_name}; valid actions: {}",
                domain.actions.iter().map(|(a, _, _)| *a).collect::<Vec<_>>().join(", ")
            )
        })?;
    // Flatten: action-specific params may arrive nested under "params" or
    // inline alongside "action"; inline wins on conflict.
    let mut legacy_args = match arguments.get("params") {
        Some(serde_json::Value::Object(map)) => map.clone(),
        _ => serde_json::Map::new(),
    };
    if let Some(obj) = arguments.as_object() {
        for (k, v) in obj {
            if k != "action" && k != "params" {
                legacy_args.insert(k.clone(), v.clone());
            }
        }
    }
    Ok(Some((legacy, serde_json::Value::Object(legacy_args))))
}

#[cfg(test)]
mod tests {
    // Tests assert on resolved values; unwrap/expect is the readable form
    // and the workspace deny only targets production paths.
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn test_domain_tools_count_and_names() {
        let tools = domain_tools();
        assert_eq!(tools.len(), 6);
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                "graphify_graph",
                "graphify_relay",
                "graphify_opendoc",
                "graphify_review",
                "graphify_metrics",
                "graphify_compose",
            ]
        );
        for t in &tools {
            let schema = t.get("inputSchema").expect("inputSchema present");
            let actions = schema
                .pointer("/properties/action/enum")
                .and_then(|v| v.as_array())
                .expect("action enum present");
            assert!(!actions.is_empty(), "every domain declares actions");
        }
    }

    #[test]
    fn test_resolve_maps_action_to_legacy_name() {
        let (legacy, args) = resolve_hub_call(
            "graphify_relay",
            &serde_json::json!({ "action": "status", "path": "/tmp/ws" }),
        )
        .unwrap()
        .expect("hub call resolves");
        assert_eq!(legacy, "graphify_relay_status");
        assert_eq!(args["path"], "/tmp/ws");
        assert!(args.get("action").is_none(), "action must not leak through");
    }

    #[test]
    fn test_resolve_nested_params_flatten() {
        let (legacy, args) = resolve_hub_call(
            "graphify_metrics",
            &serde_json::json!({
                "action": "coverage_ingest",
                "params": { "format": "lcov", "data": "SF:foo.rs" }
            }),
        )
        .unwrap()
        .expect("hub call resolves");
        assert_eq!(legacy, "graphify_coverage_ingest");
        assert_eq!(args["format"], "lcov");
        assert_eq!(args["data"], "SF:foo.rs");
    }

    #[test]
    fn test_resolve_inline_params_merge() {
        // Inline args alongside action (LLM-friendly flat form).
        let (legacy, args) = resolve_hub_call(
            "graphify_graph",
            &serde_json::json!({ "action": "query", "question": "auth flow" }),
        )
        .unwrap()
        .expect("hub call resolves");
        assert_eq!(legacy, "graphify_graph_query");
        assert_eq!(args["question"], "auth flow");
    }

    #[test]
    fn test_resolve_missing_action_lists_valid() {
        let err = resolve_hub_call("graphify_relay", &serde_json::json!({ "path": "/x" }))
            .expect_err("missing action must error");
        assert!(err.contains("missing 'action'"), "{err}");
        assert!(err.contains("status"), "{err}");
        assert!(err.contains("add"), "{err}");
    }

    #[test]
    fn test_resolve_unknown_action_lists_valid() {
        let err = resolve_hub_call(
            "graphify_compose",
            &serde_json::json!({ "action": "explode" }),
        )
        .expect_err("unknown action must error");
        assert!(err.contains("unknown action 'explode'"), "{err}");
        assert!(err.contains("read"), "{err}");
        assert!(err.contains("render"), "{err}");
    }

    #[test]
    fn test_resolve_non_hub_name_passthrough() {
        let resolved = resolve_hub_call(
            "graphify_relay_status",
            &serde_json::json!({ "path": "/x" }),
        )
        .unwrap();
        assert!(resolved.is_none(), "legacy names pass through untouched");
    }
}
