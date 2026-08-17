//! `SQLite` Global Registry (`graphify.db`).
//!
//! Tracks workspaces, plugin registrations, and handoff snapshots across the
//! Graphify ecosystem. Serves as the routing and rehydration authority for
//! workspace-scoped memory (RFC-0004 §1.1).

use std::fmt;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use thiserror::Error;

use graphify_core::HandoffSnapshot;

/// Current registry schema version, tracked via `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 2;

/// Handoff snapshot TTL in days (RFC-0004 §5, spec handoff-pruning).
pub const HANDOFF_TTL_DAYS: i64 = 7;

/// Maximum handoff snapshots retained per workspace (FIFO eviction).
pub const HANDOFF_MAX_PER_WORKSPACE: u64 = 20;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("registry io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("registry database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("registry schema mismatch: {0}")]
    Schema(String),
}

/// Plugin registration status. Four-state model (plugin-health-admission
/// Phase 1): Healthy, Degraded, Unavailable, Quarantined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginStatus {
    Healthy,
    Degraded,
    Unavailable,
    Quarantined,
}

impl PluginStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "Healthy",
            Self::Degraded => "Degraded",
            Self::Unavailable => "Unavailable",
            Self::Quarantined => "Quarantined",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "Healthy" => Self::Healthy,
            "Degraded" => Self::Degraded,
            "Quarantined" => Self::Quarantined,
            // Unknown/legacy values collapse to Unavailable; the CHECK
            // constraint prevents them from being written in the first place.
            _ => Self::Unavailable,
        }
    }
}

impl fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub workspace_key: String,
    pub root_path: String,
    pub is_active: bool,
    pub last_indexed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRegistrationRow {
    pub plugin_id: String,
    pub workspace_key: String,
    pub qdrant_collection_name: String,
    pub last_synced_at: i64,
    pub status: PluginStatus,
}

/// The global registry. Wraps a single synchronous `SQLite` connection.
pub struct RegistryDb {
    conn: Connection,
}

impl RegistryDb {
    /// Open (creating if missing) the registry at `path` with schema v1.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn open(path: &Path) -> Result<Self, RegistryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        let db = Self { conn };
        db.ensure_schema()?;
        Ok(db)
    }

    fn ensure_schema(&self) -> Result<(), RegistryError> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => {
                self.migrate_to_v1()?;
                self.migrate_to_v2()
            }
            1 => self.migrate_to_v2(),
            SCHEMA_VERSION => Ok(()),
            other => Err(RegistryError::Schema(format!(
                "database at version {other}, expected {SCHEMA_VERSION}"
            ))),
        }
    }

    fn migrate_to_v1(&self) -> Result<(), RegistryError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS workspaces (
                workspace_key  TEXT PRIMARY KEY,
                root_path      TEXT NOT NULL,
                is_active      INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
                last_indexed_at INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS plugin_registrations (
                plugin_id              TEXT NOT NULL,
                workspace_key          TEXT NOT NULL,
                qdrant_collection_name TEXT NOT NULL,
                last_synced_at         INTEGER NOT NULL DEFAULT 0,
                status                 TEXT NOT NULL CHECK (status IN ('Ready', 'Unavailable')),
                PRIMARY KEY (plugin_id, workspace_key),
                FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS handoff_registry (
                snapshot_id   TEXT PRIMARY KEY,
                session_id    TEXT NOT NULL,
                workspace_key TEXT NOT NULL,
                created_at    INTEGER NOT NULL,
                expires_at    INTEGER NOT NULL,
                payload       TEXT NOT NULL,
                FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
            );
            ",
        )?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
        Ok(())
    }

    /// v1 → v2: rebuild `plugin_registrations` with the four-state status
    /// CHECK constraint (plugin-health-admission Phase 1). Existing rows are
    /// preserved; `'Ready'` maps to `'Healthy'` and `'Unavailable'` stays.
    fn migrate_to_v2(&self) -> Result<(), RegistryError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch(
            "
            CREATE TABLE plugin_registrations_v2 (
                plugin_id              TEXT NOT NULL,
                workspace_key          TEXT NOT NULL,
                qdrant_collection_name TEXT NOT NULL,
                last_synced_at         INTEGER NOT NULL DEFAULT 0,
                status                 TEXT NOT NULL CHECK (status IN
                    ('Healthy', 'Degraded', 'Unavailable', 'Quarantined')),
                PRIMARY KEY (plugin_id, workspace_key),
                FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
            );

            INSERT INTO plugin_registrations_v2
                (plugin_id, workspace_key, qdrant_collection_name, last_synced_at, status)
            SELECT plugin_id, workspace_key, qdrant_collection_name, last_synced_at,
                   CASE status WHEN 'Ready' THEN 'Healthy' ELSE status END
            FROM plugin_registrations;

            DROP TABLE plugin_registrations;
            ALTER TABLE plugin_registrations_v2 RENAME TO plugin_registrations;
            ",
        )?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
        Ok(())
    }

    // ---- workspaces ----

    /// Upsert a workspace. A brand-new workspace becomes the active one
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn upsert_workspace(
        &self,
        workspace_key: &str,
        root_path: &str,
    ) -> Result<(), RegistryError> {
        let exists: bool = self
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workspaces WHERE workspace_key = ?1)",
                [workspace_key],
                |row| row.get::<_, i64>(0).map(|v| v != 0),
            )
            .map_err(RegistryError::from)?;
        if exists {
            self.conn.execute(
                "UPDATE workspaces SET root_path = ?2 WHERE workspace_key = ?1",
                (workspace_key, root_path),
            )?;
        } else {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute("UPDATE workspaces SET is_active = 0", [])?;
            tx.execute(
                "INSERT INTO workspaces (workspace_key, root_path, is_active)
                 VALUES (?1, ?2, 1)",
                (workspace_key, root_path),
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Mark exactly one workspace active; all others are cleared.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn set_active_workspace(&self, workspace_key: &str) -> Result<(), RegistryError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE workspaces SET is_active = 0", [])?;
        tx.execute(
            "UPDATE workspaces SET is_active = 1 WHERE workspace_key = ?1",
            [workspace_key],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// List all registered workspaces.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT workspace_key, root_path, is_active, last_indexed_at
             FROM workspaces ORDER BY workspace_key",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkspaceRow {
                workspace_key: row.get(0)?,
                root_path: row.get(1)?,
                is_active: row.get::<_, i64>(2)? != 0,
                last_indexed_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Return the currently active workspace, if any.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn get_active_workspace(&self) -> Result<Option<WorkspaceRow>, RegistryError> {
        self.conn
            .query_row(
                "SELECT workspace_key, root_path, is_active, last_indexed_at
                 FROM workspaces WHERE is_active = 1 LIMIT 1",
                [],
                |row| {
                    Ok(WorkspaceRow {
                        workspace_key: row.get(0)?,
                        root_path: row.get(1)?,
                        is_active: row.get::<_, i64>(2)? != 0,
                        last_indexed_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    // ---- plugin registrations ----

    /// Register a plugin for a workspace. First registration starts with
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn upsert_plugin_registration(
        &self,
        plugin_id: &str,
        workspace_key: &str,
        qdrant_collection_name: &str,
    ) -> Result<(), RegistryError> {
        self.conn.execute(
            "INSERT INTO plugin_registrations
                 (plugin_id, workspace_key, qdrant_collection_name, last_synced_at, status)
             VALUES (?1, ?2, ?3, 0, 'Unavailable')
             ON CONFLICT(plugin_id, workspace_key) DO NOTHING",
            (plugin_id, workspace_key, qdrant_collection_name),
        )?;
        Ok(())
    }

    /// Set the registration status for a plugin in a workspace.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn set_status(
        &self,
        plugin_id: &str,
        workspace_key: &str,
        status: PluginStatus,
    ) -> Result<(), RegistryError> {
        self.conn.execute(
            "UPDATE plugin_registrations SET status = ?3
             WHERE plugin_id = ?1 AND workspace_key = ?2",
            (plugin_id, workspace_key, status.as_str()),
        )?;
        Ok(())
    }

    /// Fetch one plugin registration, if present.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn get_registration(
        &self,
        plugin_id: &str,
        workspace_key: &str,
    ) -> Result<Option<PluginRegistrationRow>, RegistryError> {
        self.conn
            .query_row(
                "SELECT plugin_id, workspace_key, qdrant_collection_name, last_synced_at, status
                 FROM plugin_registrations WHERE plugin_id = ?1 AND workspace_key = ?2",
                (plugin_id, workspace_key),
                |row| {
                    Ok(PluginRegistrationRow {
                        plugin_id: row.get(0)?,
                        workspace_key: row.get(1)?,
                        qdrant_collection_name: row.get(2)?,
                        last_synced_at: row.get(3)?,
                        status: PluginStatus::from_str(&row.get::<_, String>(4)?),
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// Advance the rehydration checkpoint after a successful sync (same
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn mark_synced(
        &self,
        plugin_id: &str,
        workspace_key: &str,
        timestamp: i64,
    ) -> Result<(), RegistryError> {
        self.conn.execute(
            "UPDATE plugin_registrations
             SET last_synced_at = ?3, status = 'Healthy'
             WHERE plugin_id = ?1 AND workspace_key = ?2",
            (plugin_id, workspace_key, timestamp),
        )?;
        Ok(())
    }

    /// List every registration for a workspace (resync iterates these).
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn list_registrations(
        &self,
        workspace_key: &str,
    ) -> Result<Vec<PluginRegistrationRow>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT plugin_id, workspace_key, qdrant_collection_name, last_synced_at, status
             FROM plugin_registrations WHERE workspace_key = ?1
             ORDER BY plugin_id",
        )?;
        let rows = stmt.query_map([workspace_key], |row| {
            Ok(PluginRegistrationRow {
                plugin_id: row.get(0)?,
                workspace_key: row.get(1)?,
                qdrant_collection_name: row.get(2)?,
                last_synced_at: row.get(3)?,
                status: PluginStatus::from_str(&row.get::<_, String>(4)?),
            })
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }

    // ---- handoff snapshots ----

    /// Insert a snapshot; `expires_at` defaults to `created_at + 7 days` when
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn put_snapshot(&self, snapshot: &HandoffSnapshot) -> Result<(), RegistryError> {
        let expires_at = if snapshot.expires_at == 0 {
            snapshot.created_at + HANDOFF_TTL_DAYS * 86_400
        } else {
            snapshot.expires_at
        };
        let mut stored = snapshot.clone();
        stored.expires_at = expires_at;
        let payload =
            serde_json::to_string(&stored).map_err(|e| RegistryError::Schema(e.to_string()))?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO handoff_registry
                 (snapshot_id, session_id, workspace_key, created_at, expires_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &snapshot.snapshot_id,
                &snapshot.session_id,
                &snapshot.workspace_key,
                snapshot.created_at,
                expires_at,
                &payload,
            ),
        )?;
        Self::prune_in_tx(&tx, &snapshot.workspace_key)?;
        tx.commit()?;
        Ok(())
    }

    fn prune_in_tx(
        tx: &rusqlite::Transaction<'_>,
        workspace_key: &str,
    ) -> Result<(), RegistryError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or_default())
            .unwrap_or_default();
        // 1. TTL expiry first.
        tx.execute(
            "DELETE FROM handoff_registry WHERE workspace_key = ?1 AND expires_at < ?2",
            (workspace_key, now),
        )?;
        // 2. FIFO cap: keep newest HANDOFF_MAX_PER_WORKSPACE by created_at.
        tx.execute(
            "DELETE FROM handoff_registry WHERE workspace_key = ?1 AND snapshot_id NOT IN (
                 SELECT snapshot_id FROM handoff_registry
                 WHERE workspace_key = ?1
                 ORDER BY created_at DESC, rowid DESC LIMIT ?2
             )",
            (
                workspace_key,
                i64::try_from(HANDOFF_MAX_PER_WORKSPACE).unwrap_or_default(),
            ),
        )?;
        Ok(())
    }

    /// Fetch one handoff snapshot by id.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<HandoffSnapshot>, RegistryError> {
        self.conn
            .query_row(
                "SELECT payload FROM handoff_registry WHERE snapshot_id = ?1",
                [snapshot_id],
                |row| {
                    let payload: String = row.get(0)?;
                    serde_json::from_str(&payload)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                },
            )
            .optional()
            .map_err(Into::into)
    }

    /// List handoff snapshots for a workspace, newest first.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn list_snapshots(
        &self,
        workspace_key: &str,
    ) -> Result<Vec<HandoffSnapshot>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM handoff_registry
             WHERE workspace_key = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([workspace_key], |row| {
            let payload: String = row.get(0)?;
            serde_json::from_str(&payload)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }

    /// List snapshots written after `last_synced_at` — the pending backlog
    /// for rehydration (P4 hooks the plugin domain store here; the SQL side
    /// is the `handoff_registry` checkpoint comparison).
    ///
    /// # Errors
    ///
    /// Returns `RegistryError` on `SQLite` failure or schema mismatch.
    pub fn get_pending_snapshots_since(
        &self,
        workspace_key: &str,
        last_synced_at: i64,
    ) -> Result<Vec<HandoffSnapshot>, RegistryError> {
        let mut stmt = self.conn.prepare(
            "SELECT payload FROM handoff_registry
             WHERE workspace_key = ?1 AND created_at > ?2
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map((workspace_key, last_synced_at), |row| {
            let payload: String = row.get(0)?;
            serde_json::from_str(&payload)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;
        rows.collect::<Result<_, _>>().map_err(Into::into)
    }
}

/// Current unix timestamp (seconds) — the rehydration checkpoint clock.
#[must_use]
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or_default())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test timestamp in the future (now + offset) so TTL pruning never
    fn future_ts(offset_secs: i64) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or_default() + offset_secs)
            .unwrap_or_default()
    }

    /// Test helper: unwrap an `Option` with a panic message (avoids
    /// `expect()` which the workspace bans even in tests).
    fn unwrap_opt<T>(value: Option<T>, msg: &str) -> T {
        value.unwrap_or_else(|| panic!("{msg}"))
    }
    fn open_temp() -> Result<(RegistryDb, tempfile::TempDir), RegistryError> {
        let dir = tempfile::tempdir().map_err(RegistryError::Io)?;
        let path = dir.path().join("graphify.db");
        let db = RegistryDb::open(&path)?;
        Ok((db, dir))
    }

    #[test]
    fn test_schema_created_on_first_open() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        assert_eq!(db.list_workspaces()?.len(), 0);
        Ok(())
    }

    #[test]
    fn test_upsert_and_active_workspace() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_workspace("ws-b", "/tmp/b")?;
        let active = unwrap_opt(db.get_active_workspace()?, "a workspace is active");
        assert_eq!(active.workspace_key, "ws-b");
        db.set_active_workspace("ws-a")?;
        let active = unwrap_opt(db.get_active_workspace()?, "a workspace is active");
        assert_eq!(active.workspace_key, "ws-a");
        let active_count = db.list_workspaces()?.iter().filter(|w| w.is_active).count();
        assert_eq!(active_count, 1);
        Ok(())
    }

    #[test]
    fn test_plugin_registration_defaults() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        let reg = db.get_registration("opendoc", "ws-a")?;
        let reg = unwrap_opt(reg, "registration exists");
        assert_eq!(reg.last_synced_at, 0);
        assert_eq!(reg.status, PluginStatus::Unavailable);
        db.set_status("opendoc", "ws-a", PluginStatus::Healthy)?;
        let reg = db.get_registration("opendoc", "ws-a")?;
        let reg = unwrap_opt(reg, "registration exists");
        assert_eq!(reg.status, PluginStatus::Healthy);
        Ok(())
    }

    #[test]
    fn test_mark_synced_advances_timestamp() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        db.mark_synced("opendoc", "ws-a", 1_700_000_000)?;
        let reg = db.get_registration("opendoc", "ws-a")?;
        let reg = unwrap_opt(reg, "registration exists");
        assert_eq!(reg.last_synced_at, 1_700_000_000);
        assert_eq!(reg.status, PluginStatus::Healthy);
        Ok(())
    }

    #[test]
    fn test_status_round_trip_all_states() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        for status in [
            PluginStatus::Healthy,
            PluginStatus::Degraded,
            PluginStatus::Unavailable,
            PluginStatus::Quarantined,
        ] {
            db.set_status("opendoc", "ws-a", status)?;
            let reg = db.get_registration("opendoc", "ws-a")?;
            let reg = unwrap_opt(reg, "registration exists");
            assert_eq!(reg.status, status);
        }
        Ok(())
    }

    #[test]
    fn test_unregistered_read_returns_none() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        assert!(db.get_registration("missing", "ws-a")?.is_none());
        Ok(())
    }

    #[test]
    fn test_status_check_constraint_rejects_unknown_state() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        // Legacy v1 vocabulary and out-of-vocabulary values must be rejected
        // by the four-state CHECK.
        for bad in ["Ready", "SyncPending", "Broken"] {
            let result = db.conn.execute(
                "UPDATE plugin_registrations SET status = ?1
                 WHERE plugin_id = 'opendoc' AND workspace_key = 'ws-a'",
                [bad],
            );
            assert!(result.is_err(), "{bad} must be rejected by CHECK");
        }
        Ok(())
    }

    #[test]
    fn test_migrate_v1_to_v2_preserves_rows_and_maps_status() -> Result<(), RegistryError> {
        // Craft a v1 database by hand: v1 schema, one Ready + one Unavailable
        // registration, then open through RegistryDb to trigger migration.
        let dir = tempfile::tempdir().map_err(RegistryError::Io)?;
        let path = dir.path().join("graphify.db");
        {
            let conn = rusqlite::Connection::open(&path)?;
            conn.execute_batch(
                "
                CREATE TABLE workspaces (
                    workspace_key  TEXT PRIMARY KEY,
                    root_path      TEXT NOT NULL,
                    is_active      INTEGER NOT NULL DEFAULT 0 CHECK (is_active IN (0, 1)),
                    last_indexed_at INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE plugin_registrations (
                    plugin_id              TEXT NOT NULL,
                    workspace_key          TEXT NOT NULL,
                    qdrant_collection_name TEXT NOT NULL,
                    last_synced_at         INTEGER NOT NULL DEFAULT 0,
                    status                 TEXT NOT NULL CHECK (status IN ('Ready', 'Unavailable')),
                    PRIMARY KEY (plugin_id, workspace_key),
                    FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
                );
                CREATE TABLE handoff_registry (
                    snapshot_id   TEXT PRIMARY KEY,
                    session_id    TEXT NOT NULL,
                    workspace_key TEXT NOT NULL,
                    created_at    INTEGER NOT NULL,
                    expires_at    INTEGER NOT NULL,
                    payload       TEXT NOT NULL,
                    FOREIGN KEY (workspace_key) REFERENCES workspaces(workspace_key) ON DELETE CASCADE
                );
                INSERT INTO workspaces (workspace_key, root_path, is_active) VALUES ('ws-a', '/tmp/a', 1);
                INSERT INTO plugin_registrations
                    (plugin_id, workspace_key, qdrant_collection_name, status)
                    VALUES ('opendoc', 'ws-a', 'graphify_plugin_opendoc', 'Ready'),
                           ('sdk', 'ws-a', 'graphify_plugin_sdk', 'Unavailable');
                PRAGMA user_version = 1;
                ",
            )?;
        }
        let db = RegistryDb::open(&path)?;
        let regs = db.list_registrations("ws-a")?;
        assert_eq!(regs.len(), 2, "row count preserved across migration");
        let opendoc = db.get_registration("opendoc", "ws-a")?;
        let opendoc = unwrap_opt(opendoc, "opendoc registration exists");
        assert_eq!(opendoc.status, PluginStatus::Healthy, "Ready maps to Healthy");
        let sdk = db.get_registration("sdk", "ws-a")?;
        let sdk = unwrap_opt(sdk, "sdk registration exists");
        assert_eq!(sdk.status, PluginStatus::Unavailable, "Unavailable stays");
        // v2 CHECK now active: legacy 'Ready' is rejected.
        let result = db.conn.execute(
            "UPDATE plugin_registrations SET status = 'Ready'
             WHERE plugin_id = 'opendoc' AND workspace_key = 'ws-a'",
            [],
        );
        assert!(result.is_err(), "Ready must be rejected post-migration");
        Ok(())
    }

    #[test]
    fn test_cascade_delete_workspace() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        db.upsert_plugin_registration("opendoc", "ws-a", "graphify_plugin_opendoc")?;
        let snap = graphify_core::HandoffSnapshot {
            snapshot_id: "snap-1".into(),
            session_id: "sess-1".into(),
            workspace_key: "ws-a".into(),
            created_at: future_ts(0),
            expires_at: 0,
            payload: graphify_core::HandoffPayload {
                task_goal: "t".into(),
                pinned_node_ids: vec![],
                focused_subgraph_toon: String::new(),
                reconstructable_query_metadata: graphify_core::MemoryQueryCriteria {
                    target_symbols: vec![],
                    domain_categories: vec![],
                    search_terms: vec![],
                },
                schema_version: 1,
            },
        };
        db.put_snapshot(&snap)?;
        db.conn
            .execute("DELETE FROM workspaces WHERE workspace_key = 'ws-a'", [])?;
        assert!(db.get_registration("opendoc", "ws-a")?.is_none());
        assert!(db.list_snapshots("ws-a")?.is_empty());
        Ok(())
    }

    #[test]
    fn test_snapshot_default_ttl() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        let snap = graphify_core::HandoffSnapshot {
            snapshot_id: "snap-1".into(),
            session_id: "sess-1".into(),
            workspace_key: "ws-a".into(),
            created_at: future_ts(0),
            expires_at: 0,
            payload: graphify_core::HandoffPayload {
                task_goal: "t".into(),
                pinned_node_ids: vec![],
                focused_subgraph_toon: String::new(),
                reconstructable_query_metadata: graphify_core::MemoryQueryCriteria {
                    target_symbols: vec![],
                    domain_categories: vec![],
                    search_terms: vec![],
                },
                schema_version: 1,
            },
        };
        db.put_snapshot(&snap)?;
        let stored = unwrap_opt(db.get_snapshot("snap-1")?, "snapshot exists");
        assert_eq!(
            stored.expires_at,
            stored.created_at + HANDOFF_TTL_DAYS * 86_400,
            "expires_at defaults to created_at + 7 days"
        );
        Ok(())
    }

    #[test]
    fn test_prune_expired_then_fifo_cap() -> Result<(), RegistryError> {
        let (db, _d) = open_temp()?;
        db.upsert_workspace("ws-a", "/tmp/a")?;
        // Insert 22 snapshots with ascending created_at (all in the future).
        for i in 0..HANDOFF_MAX_PER_WORKSPACE + 2 {
            let snap = graphify_core::HandoffSnapshot {
                snapshot_id: format!("snap-{i}"),
                session_id: "sess".into(),
                workspace_key: "ws-a".into(),
                created_at: future_ts(i64::try_from(i).unwrap_or_default()),
                expires_at: 0,
                payload: graphify_core::HandoffPayload {
                    task_goal: format!("goal {i}"),
                    pinned_node_ids: vec![],
                    focused_subgraph_toon: String::new(),
                    reconstructable_query_metadata: graphify_core::MemoryQueryCriteria {
                        target_symbols: vec![],
                        domain_categories: vec![],
                        search_terms: vec![],
                    },
                    schema_version: 1,
                },
            };
            db.put_snapshot(&snap)?;
        }
        let snaps = db.list_snapshots("ws-a")?;
        assert_eq!(
            snaps.len(),
            usize::try_from(HANDOFF_MAX_PER_WORKSPACE).unwrap_or_default()
        );
        // FIFO: oldest two (snap-0, snap-1) evicted.
        assert!(!snaps.iter().any(|s| s.snapshot_id == "snap-0"));
        assert!(!snaps.iter().any(|s| s.snapshot_id == "snap-1"));
        assert!(snaps.iter().any(|s| s.snapshot_id == "snap-21"));
        Ok(())
    }
}
