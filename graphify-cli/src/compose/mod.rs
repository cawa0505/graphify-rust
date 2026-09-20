//! `graphify compose` — Assembly Manifest (YAML) → unified graph → diagram.
//!
//! Commands:
//! - `validate`: resolve + verify every declared workspace/relation endpoint.
//! - `graph`: build the unified graph and print a JSON summary.
//! - `render`: project the unified graph into box-of-rain and print the diagram.

use std::path::Path;

use anyhow::anyhow;
use graphify_core::compose_manifest::load_manifest;
use graphify_core::compose_merge::build_unified_graph;
use graphify_core::compose_render::{project, render_via_npx};

/// `graphify compose validate <manifest>`
///
/// # Errors
/// Fails when the manifest fails to parse or any validation error exists.
pub fn validate(manifest_path: &Path) -> anyhow::Result<()> {
    let loaded = load_manifest(manifest_path)?;
    println!(
        "✓ manifest 驗證通過: workspaces {} 個, relations {} 條 ({})",
        loaded.manifest.workspaces.len(),
        loaded.manifest.relations.len(),
        manifest_path.display()
    );
    Ok(())
}

/// `graphify compose graph <manifest>`
///
/// # Errors
/// Fails when the manifest fails validation or any workspace graph cannot be
/// loaded/merged.
pub fn graph(manifest_path: &Path) -> anyhow::Result<()> {
    let loaded = load_manifest(manifest_path)?;
    let unified = build_unified_graph(&loaded)?;
    let summary = serde_json::json!({
        "workspaces": unified.workspace_count,
        "total_nodes": unified.nodes.len(),
        "total_edges": unified.edges.len(),
        "cross_edges": unified.cross_edges,
    });
    println!("{summary}");
    Ok(())
}

/// `graphify compose render <manifest> [--svg]`
///
/// # Errors
/// Fails on manifest validation, merge, projection, or npx execution errors.
pub fn render(manifest_path: &Path, svg: bool) -> anyhow::Result<()> {
    let loaded = load_manifest(manifest_path)?;
    let unified = build_unified_graph(&loaded)?;
    if unified.workspace_count == 0 {
        return Err(anyhow!("manifest 未宣告任何 workspace，無圖可渲染"));
    }
    let projection = project(&unified);
    let output = render_via_npx(&projection, svg)?;
    print!("{output}");
    Ok(())
}
