//! Filesystem path resolution for enjo (Phase 1, local only).
//!
//! Phase 1 has no backend, network, or sync, so the only job here is to decide
//! *where* the local SQLite database lives and to make sure that directory
//! exists before the store opens a DB inside it.
//!
//! ## Data directory resolution
//! The per-user data directory is resolved in this order:
//! 1. The `ENJO_DATA_DIR` environment variable, if set and non-empty. The value
//!    is used verbatim. This lets power users relocate the DB, and (importantly)
//!    lets tests point enjo at a throwaway temp dir without touching the real
//!    home directory.
//! 2. Otherwise, [`directories::ProjectDirs`] via `ProjectDirs::from("", "",
//!    "enjo")`. On macOS this is `~/Library/Application Support/enjo`, with the
//!    platform-appropriate equivalents on Linux (`~/.local/share/enjo`) and
//!    Windows (`%APPDATA%\enjo\data`).
//!
//! The database file is always `enjo.db` inside that directory.
//!
//! ## Phase 2/3 (not implemented here)
//! A later phase will add a TOML config file (`config.toml`) holding the backend
//! URL, device key, and sync interval, loaded via the `toml` crate. That layer
//! is intentionally absent in Phase 1 — no dead fields are added for it yet.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;

/// Environment variable that overrides the resolved data directory.
const DATA_DIR_ENV: &str = "ENJO_DATA_DIR";

/// Resolved filesystem paths for the running enjo instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    data_dir: PathBuf,
}

impl Config {
    /// Resolve the data directory (env override, else [`ProjectDirs`]) and create
    /// it on disk so the store can open a DB inside it.
    ///
    /// Returns an error if no home directory can be determined (and no override
    /// is set), or if the data directory cannot be created.
    pub fn load() -> Result<Self> {
        let data_dir = resolve_data_dir()?;
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create data directory {}", data_dir.display()))?;
        Ok(Self { data_dir })
    }

    /// Construct a `Config` from an explicit data directory without touching the
    /// filesystem. For tests and explicit callers.
    #[allow(dead_code)] // test/explicit-caller helper; Phase 2/3 config loading will use it
    pub fn with_data_dir(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    /// Path to the SQLite database file (`<data_dir>/enjo.db`).
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("enjo.db")
    }

    /// The resolved data directory.
    #[allow(dead_code)] // surfaced for tests / Phase 2/3 config UI
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// Resolve the data directory from the env override or platform project dirs,
/// without creating anything on disk.
fn resolve_data_dir() -> Result<PathBuf> {
    if let Some(dir) = env_override() {
        return Ok(dir);
    }
    let dirs = ProjectDirs::from("", "", "enjo").context(
        "could not determine a home directory for the enjo data dir; \
         set ENJO_DATA_DIR to choose one explicitly",
    )?;
    Ok(dirs.data_dir().to_path_buf())
}

/// The `ENJO_DATA_DIR` override, if set and non-empty.
fn env_override() -> Option<PathBuf> {
    match std::env::var(DATA_DIR_ENV) {
        Ok(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Env vars are process-global and tests run in parallel; serialize every
    /// test that touches `ENJO_DATA_DIR` through this lock.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// RAII guard that sets `ENJO_DATA_DIR` and restores the previous value
    /// (or unsets it) on drop, even if a test panics.
    struct EnvGuard {
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(value: &Path) -> Self {
            let previous = std::env::var(DATA_DIR_ENV).ok();
            std::env::set_var(DATA_DIR_ENV, value);
            Self { previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(v) => std::env::set_var(DATA_DIR_ENV, v),
                None => std::env::remove_var(DATA_DIR_ENV),
            }
        }
    }

    /// A unique path under the system temp dir, never created here.
    fn unique_temp_path(label: &str) -> PathBuf {
        let unique = format!(
            "enjo-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn db_path_is_data_dir_join_enjo_db() {
        let dir = PathBuf::from("/some/where/enjo-data");
        let config = Config::with_data_dir(dir.clone());
        assert_eq!(config.data_dir(), dir.as_path());
        assert_eq!(config.db_path(), dir.join("enjo.db"));
    }

    #[test]
    fn load_honors_env_override_and_creates_dir() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = unique_temp_path("override");
        assert!(!dir.exists(), "temp path must start absent");

        let _guard = EnvGuard::set(&dir);
        let config = Config::load().unwrap();

        assert_eq!(config.data_dir(), dir.as_path());
        assert!(dir.exists(), "load() must create the data dir");
        assert!(dir.is_dir());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_creates_nested_parents() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = unique_temp_path("nested");
        let dir = base.join("a").join("b").join("c");
        assert!(!base.exists(), "temp base must start absent");

        let _guard = EnvGuard::set(&dir);
        let config = Config::load().unwrap();

        assert_eq!(config.data_dir(), dir.as_path());
        assert!(dir.exists(), "load() must create all missing parents");

        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn empty_env_override_falls_back_to_project_dirs() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _guard = EnvGuard::set(Path::new(""));
        // An empty override is ignored; resolution falls through to ProjectDirs,
        // which yields a path ending in "enjo" on every supported platform.
        let dir = resolve_data_dir().unwrap();
        assert!(dir.ends_with("enjo"), "resolved dir was {}", dir.display());
    }
}
