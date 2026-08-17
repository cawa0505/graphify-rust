//! Graphify workspace management — list, switch, add, delete, and status of registered workspaces.
//!
//! Backed by the `graphify-registry` `SQLite` database (`RegistryDb`).

use anyhow::{Context, Result};
use graphify_core::derive_workspace_key;
use graphify_registry::db::{RegistryDb, WorkspaceRow};
use graphify_registry::registry_db_path;

/// Open the global registry, creating the database if missing.
pub(crate) fn open_registry() -> Result<RegistryDb> {
    let path = registry_db_path();
    RegistryDb::open(&path).with_context(|| format!("opening registry at {}", path.display()))
}

/// List all registered workspaces.
pub fn list_workspaces() -> Result<Vec<WorkspaceRow>> {
    open_registry()?
        .list_workspaces()
        .map_err(anyhow::Error::from)
}

/// Switch the active workspace.
pub fn switch_workspace(workspace_key: &str) -> Result<()> {
    open_registry()?
        .set_active_workspace(workspace_key)
        .map_err(anyhow::Error::from)
}

/// Return the currently active workspace, if any.
pub fn active_workspace() -> Result<Option<WorkspaceRow>> {
    open_registry()?
        .get_active_workspace()
        .map_err(anyhow::Error::from)
}

/// Return a single workspace by key, if registered.
pub fn workspace_status(workspace_key: &str) -> Result<Option<WorkspaceRow>> {
    let rows = open_registry()?
        .list_workspaces()
        .map_err(anyhow::Error::from)?;
    Ok(rows.into_iter().find(|w| w.workspace_key == workspace_key))
}

/// Add a workspace by root path. Key is auto-derived from the canonical path.
/// Returns `Ok(true)` if added, `Ok(false)` if already registered.
/// Resolves `.` to the current working directory's full path.
pub fn add_workspace(root_path: &str) -> Result<bool> {
    let path = std::path::Path::new(root_path);
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolving path: {root_path}"))?;
    let canonical_str = canonical.to_string_lossy().to_string();

    let db = open_registry()?;

    // Check if path already registered
    if db.find_workspace_by_path(&canonical_str)?.is_some() {
        return Ok(false);
    }

    let key = derive_workspace_key(&canonical);
    db.upsert_workspace(&key, &canonical_str)?;
    Ok(true)
}

/// Delete a workspace by key.
pub fn delete_workspace(workspace_key: &str) -> Result<()> {
    open_registry()?
        .delete_workspace(workspace_key)
        .map_err(anyhow::Error::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open a throwaway registry and seed two workspaces.
    fn seeded_registry() -> Result<(RegistryDb, tempfile::TempDir)> {
        let dir = tempfile::tempdir()?;
        let db = RegistryDb::open(&dir.path().join("graphify.db"))?;
        db.upsert_workspace("ws-a", "/tmp/ws-a")?;
        db.upsert_workspace("ws-b", "/tmp/ws-b")?;
        Ok((db, dir))
    }

    #[test]
    fn list_returns_all_registered() -> Result<()> {
        let (db, _dir) = seeded_registry()?;
        let rows = db.list_workspaces()?;
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|w| w.workspace_key == "ws-a"));
        assert!(rows.iter().any(|w| w.workspace_key == "ws-b"));
        Ok(())
    }

    #[test]
    fn switch_makes_one_active() -> Result<()> {
        let (db, _dir) = seeded_registry()?;
        db.set_active_workspace("ws-b")?;
        let active = db
            .get_active_workspace()?
            .ok_or_else(|| anyhow::anyhow!("no active workspace"))?;
        assert_eq!(active.workspace_key, "ws-b");
        Ok(())
    }

    #[test]
    fn status_finds_by_key() -> Result<()> {
        let (db, _dir) = seeded_registry()?;
        let rows = db.list_workspaces()?;
        let found = rows.into_iter().find(|w| w.workspace_key == "ws-a");
        assert_eq!(found.map(|w| w.workspace_key).as_deref(), Some("ws-a"));
        Ok(())
    }

    #[test]
    fn delete_removes_workspace() -> Result<()> {
        let (db, _dir) = seeded_registry()?;
        db.delete_workspace("ws-a")?;
        let rows = db.list_workspaces()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].workspace_key, "ws-b");
        Ok(())
    }

    #[test]
    fn find_by_path_returns_workspace() -> Result<()> {
        let (db, _dir) = seeded_registry()?;
        let found = db.find_workspace_by_path("/tmp/ws-a")?;
        let ws = found.ok_or_else(|| anyhow::anyhow!("expected workspace"))?;
        assert_eq!(ws.workspace_key, "ws-a");
        let missing = db.find_workspace_by_path("/nonexistent")?;
        assert!(missing.is_none());
        Ok(())
    }
}