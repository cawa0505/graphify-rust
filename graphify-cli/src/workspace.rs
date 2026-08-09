//! Graphify workspace management — list, switch, and status of registered workspaces.
//!
//! Backed by the `graphify-registry` `SQLite` database (`RegistryDb`).

use anyhow::{Context, Result};
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
}
