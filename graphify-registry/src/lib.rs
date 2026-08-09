//! Graphify `SQLite` Global Registry — workspace/plugin/handoff tracking.

pub mod db;
pub mod resync;

pub use db::{
    HANDOFF_MAX_PER_WORKSPACE, HANDOFF_TTL_DAYS, PluginRegistrationRow, PluginStatus, RegistryDb,
    RegistryError, WorkspaceRow,
};
pub use resync::{ProviderProbe, ResyncOutcome, SyncJob, check_and_resync};

use std::ffi::OsString;
use std::path::PathBuf;

/// Resolve the global registry DB path per the XDG Base Directory spec.
///
/// Precedence: `GRAPHIFY_REGISTRY_PATH` (explicit override) >
/// `$XDG_DATA_HOME/graphify/graphify.db` > `~/.local/share/graphify/graphify.db`.
/// Falls back to `./graphify.db` when neither `XDG_DATA_HOME` nor `HOME`
/// is set.
#[must_use]
pub fn registry_db_path() -> PathBuf {
    registry_db_path_from(
        std::env::var_os("GRAPHIFY_REGISTRY_PATH"),
        std::env::var_os("XDG_DATA_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Pure variant of [`registry_db_path`] — takes the raw env values so the
/// resolution logic is testable without mutating process-global state.
#[must_use]
pub fn registry_db_path_from(
    override_path: Option<OsString>,
    xdg_data_home: Option<OsString>,
    home: Option<OsString>,
) -> PathBuf {
    if let Some(p) = override_path {
        return PathBuf::from(p);
    }
    let base = xdg_data_home
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_default();
    if base.as_os_str().is_empty() {
        PathBuf::from("graphify.db")
    } else {
        base.join("graphify").join("graphify.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_env_wins_over_xdg_and_home() {
        let p = registry_db_path_from(
            Some("/tmp/custom/graphify.db".into()),
            Some("/tmp/xdg".into()),
            Some("/tmp/home".into()),
        );
        assert_eq!(p, PathBuf::from("/tmp/custom/graphify.db"));
    }

    #[test]
    fn xdg_data_home_takes_precedence_over_home() {
        let p = registry_db_path_from(None, Some("/tmp/xdg".into()), Some("/tmp/home".into()));
        assert_eq!(p, PathBuf::from("/tmp/xdg/graphify/graphify.db"));
    }

    #[test]
    fn home_fallback_when_xdg_unset() {
        let p = registry_db_path_from(None, None, Some("/tmp/home".into()));
        assert_eq!(
            p,
            PathBuf::from("/tmp/home/.local/share/graphify/graphify.db")
        );
    }

    #[test]
    fn cwd_fallback_when_no_home() {
        let p = registry_db_path_from(None, None, None);
        assert_eq!(p, PathBuf::from("graphify.db"));
    }
}
