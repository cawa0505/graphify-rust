//! Managed local Qdrant process for the dual-track fallback (RFC-0004 §1.3).
//!
//! `QdrantLocalProcess` downloads the official standalone qdrant binary on
//! first use (SHA-256 verified against the GitHub Releases API), spawns it
//! with `QDRANT__` environment overrides, waits for readiness, and terminates
//! it gracefully (SIGTERM) on drop.
//!
//! Design: see the `OpenSpec` change design document, decision D1, for rationale.
//! The in-process `Qdrant::from_path` API from RFC-0004 §1.3 does not exist in
//! any qdrant-client release (0.11.1–1.19.0) — a managed subprocess is the
//! chosen replacement (docs/ref/RFC-0004 kept verbatim per #3127).

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};

/// Default asset target triplet for `x86_64` Linux.
pub const DEFAULT_TARGET: &str = "x86_64-unknown-linux-gnu";
/// Readiness probe poll interval.
const READY_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Maximum time to wait for the local process to accept connections.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// A spawned, managed qdrant standalone process.
pub struct QdrantLocalProcess {
    child: Child,
    http_port: u16,
    storage_dir: PathBuf,
}

impl QdrantLocalProcess {
    /// Resolve a possibly-relative path against `$HOME`, mirroring the
    /// fastembed cache convention in `memory.rs` (relative config defaults
    /// like `.cache/graphify/qdrant` are XDG-style paths under `$HOME`).
    pub fn resolve_path(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            dirs::home_dir().map_or_else(|| path.to_path_buf(), |home| home.join(path))
        }
    }

    /// Download and verify the qdrant standalone binary (first use only).
    ///
    /// Fetches the release asset tarball, compares its SHA-256 against the
    /// `digest` from the GitHub Releases API (releases ship no checksums.txt),
    /// extracts the `qdrant` binary into `bin_dir`, and returns its path.
    pub async fn download_binary(
        client: &reqwest::Client,
        version: &str,
        target: &str,
        bin_dir: &Path,
    ) -> Result<PathBuf> {
        let asset_url = format!(
            "https://github.com/qdrant/qdrant/releases/download/{version}/qdrant-{target}.tar.gz"
        );
        let api_url = format!("https://api.github.com/repos/qdrant/qdrant/releases/tags/{version}");

        let release: serde_json::Value = client
            .get(&api_url)
            .send()
            .await
            .context("querying GitHub Releases API for digest")?
            .json()
            .await
            .context("parsing GitHub Releases API response")?;
        let digest = release
            .get("assets")
            .and_then(|assets| assets.as_array())
            .and_then(|assets| {
                assets.iter().find(|a| {
                    a.get("name").and_then(|n| n.as_str())
                        == Some(&format!("qdrant-{target}.tar.gz"))
                })
            })
            .and_then(|a| a.get("digest"))
            .and_then(|d| d.as_str())
            .ok_or_else(|| anyhow!("release asset qdrant-{target}.tar.gz digest not found"))?;

        let body = client
            .get(&asset_url)
            .send()
            .await
            .context("downloading qdrant release asset")?
            .error_for_status()
            .context("download failed")?
            .bytes()
            .await
            .context("reading download body")?;

        verify_sha256(&body, digest)?;

        std::fs::create_dir_all(bin_dir).context("creating qdrant bin dir")?;
        let out_path = bin_dir.join("qdrant");
        let mut out = std::fs::File::create(&out_path).context("creating qdrant binary file")?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(body.as_ref()));
        let mut extracted = false;
        for entry in archive.entries().context("reading tarball")? {
            let mut entry = entry.context("reading tarball entry")?;
            let is_qdrant = entry
                .path()
                .ok()
                .and_then(|p| p.file_name().map(|f| f == "qdrant"))
                .unwrap_or(false);
            if is_qdrant && entry.header().entry_type().is_file() {
                std::io::copy(&mut entry, &mut out).context("extracting qdrant binary")?;
                extracted = true;
                break;
            }
        }
        if !extracted {
            bail!("qdrant binary not found inside tarball");
        }
        // Executable bit: the tar entry carries mode bits; `create` resets them.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(0o755))
                .context("marking qdrant binary executable")?;
        }
        Ok(out_path)
    }

    /// Build the spawn command with `QDRANT__` environment overrides
    /// (official highest-precedence config mechanism; the standalone binary
    /// has no equivalent CLI flags).
    pub fn build_spawn_command(
        bin_path: &Path,
        storage_dir: &Path,
        http_port: u16,
        grpc_port: u16,
    ) -> Command {
        let mut cmd = Command::new(bin_path);
        cmd.env("QDRANT__SERVICE__HTTP_PORT", http_port.to_string())
            .env("QDRANT__SERVICE__GRPC_PORT", grpc_port.to_string())
            .env("QDRANT__STORAGE__STORAGE_PATH", storage_dir)
            .env("QDRANT__TELEMETRY_DISABLED", "true")
            .env("QDRANT__LOG_LEVEL", "error")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }

    /// Spawn the local process and wait for readiness.
    pub fn spawn(
        bin_path: &Path,
        storage_dir: &Path,
        http_port: u16,
        grpc_port: u16,
    ) -> Result<Self> {
        let mut child = Self::build_spawn_command(bin_path, storage_dir, http_port, grpc_port)
            .spawn()
            .context("spawning local qdrant process")?;
        if let Err(e) = wait_ready(http_port, READY_TIMEOUT) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.context("local qdrant failed readiness probe"));
        }
        Ok(Self {
            child,
            http_port,
            storage_dir: storage_dir.to_path_buf(),
        })
    }

    /// HTTP port the local process listens on.
    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    /// Storage directory backing the local process.
    pub fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }
}

impl Drop for QdrantLocalProcess {
    /// Graceful shutdown: SIGTERM (lets qdrant flush storage), then reap.
    fn drop(&mut self) {
        let pid = nix::unistd::Pid::from_raw(i32::try_from(self.child.id()).unwrap_or(-1));
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
        let _ = self.child.wait();
    }
}

/// Verify a byte buffer against an expected lowercase hex SHA-256 digest.
pub fn verify_sha256(data: &[u8], expected: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(data));
    if actual == expected.trim().to_lowercase() {
        Ok(())
    } else {
        Err(anyhow!(
            "SHA-256 mismatch: expected {expected}, got {actual} — refusing to run unverified binary"
        ))
    }
}

/// Poll until the local process accepts a TCP connection on `http_port`.
pub fn wait_ready(http_port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect(("127.0.0.1", http_port)) {
            Ok(_) => return Ok(()),
            Err(_) if Instant::now() < deadline => std::thread::sleep(READY_POLL_INTERVAL),
            Err(e) => return Err(anyhow!("local qdrant not ready after {timeout:?}: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_absolute_unchanged() {
        let p = PathBuf::from("/tmp/x/qdrant-storage");
        assert_eq!(QdrantLocalProcess::resolve_path(&p), p);
    }

    #[test]
    fn resolve_path_relative_joins_home() -> Result<()> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
        let resolved = QdrantLocalProcess::resolve_path(Path::new(".cache/graphify/qdrant"));
        assert_eq!(resolved, home.join(".cache/graphify/qdrant"));
        Ok(())
    }

    #[test]
    fn verify_sha256_mismatch_rejected() {
        let data = b"not the real binary";
        let wrong = "0".repeat(64);
        assert!(verify_sha256(data, &wrong).is_err());
    }

    #[test]
    fn verify_sha256_match_accepted() {
        use sha2::{Digest, Sha256};
        let data = b"hello";
        let digest = format!("{:x}", Sha256::digest(data));
        assert!(verify_sha256(data, &digest).is_ok());
    }

    #[test]
    fn wait_ready_accepts_listening_port() -> Result<()> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        std::thread::spawn(move || {
            let _ = listener.accept();
        });
        assert!(wait_ready(port, Duration::from_secs(2)).is_ok());
        Ok(())
    }

    #[test]
    fn wait_ready_times_out_on_closed_port() -> Result<()> {
        // Bind then drop: port is closed.
        let port = {
            let l = std::net::TcpListener::bind(("127.0.0.1", 0))?;
            l.local_addr()?.port()
        };
        assert!(wait_ready(port, Duration::from_millis(600)).is_err());
        Ok(())
    }

    #[test]
    fn spawn_command_carries_env_overrides() {
        let cmd = QdrantLocalProcess::build_spawn_command(
            Path::new("/bin/true"),
            Path::new("/tmp/store"),
            16_333,
            16_334,
        );
        let envs = cmd.get_envs().collect::<Vec<_>>();
        let get = |k: &str| {
            envs.iter()
                .find(|(ek, _)| ek == &std::ffi::OsString::from(k))
                .and_then(|(_, v)| v.map(|v| v.to_string_lossy().to_string()))
        };
        assert_eq!(get("QDRANT__SERVICE__HTTP_PORT").as_deref(), Some("16333"));
        assert_eq!(get("QDRANT__SERVICE__GRPC_PORT").as_deref(), Some("16334"));
        assert_eq!(
            get("QDRANT__STORAGE__STORAGE_PATH").as_deref(),
            Some("/tmp/store")
        );
        assert_eq!(get("QDRANT__TELEMETRY_DISABLED").as_deref(), Some("true"));
        assert_eq!(get("QDRANT__LOG_LEVEL").as_deref(), Some("error"));
    }
}
