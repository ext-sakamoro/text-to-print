//! Opt-in local crash reporter (#36 Phase B client-side)
//!
//! Provides a `std::panic::set_hook` integration that captures panic
//! payload + backtrace + minimal context to
//! `{data_dir}/crash_reports/{uuid}.json` when the user has opted in
//!
//! Upload to a hosted collector (Cloudflare Workers Sentry-alternative
//! per Epic-Infra #36) is left as a `TODO` for when the backend is
//! deployed The local file dump is honest — a user who checks the
//! folder can see exactly what would be uploaded
//!
//! Design notes:
//! - Opt-in is expressed via a filesystem sentinel file
//!   (`{data_dir}/.crash_reports_enabled`), so the panic hook can read
//!   it without depending on the `AppState` which may already be
//!   corrupted at panic time
//! - The panic hook chain is preserved — we call the previous hook
//!   after writing our report so `RUST_BACKTRACE=1` still logs to
//!   stderr
//! - Report format is versioned via `schema_version = "1"` so the
//!   upload backend can evolve without breaking older clients
//! - Writes are best-effort; a failure inside the panic hook is
//!   swallowed to avoid recursive panics

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One crash report row, versioned schema Serialised to JSON for local
/// storage and (eventually) upload to the Sentry-alternative collector
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub schema_version: String,
    pub uuid: String,
    pub timestamp: String,
    pub app_version: String,
    pub os: String,
    pub arch: String,
    /// The panic message extracted from `PanicInfo::payload()`
    pub message: String,
    /// Source file + line if available
    pub location: Option<String>,
    /// Captured backtrace (empty when `RUST_BACKTRACE=0`)
    pub backtrace: String,
}

impl CrashReport {
    /// Serialise + write the report as `{dir}/{uuid}.json` The directory
    /// is created if missing Errors from IO / serialisation are
    /// returned but never re-panicked — callers inside a panic hook are
    /// expected to ignore them
    pub fn write_to(&self, dir: &Path) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", self.uuid));
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&path, json)?;
        Ok(path)
    }
}

/// Marker sentinel file The `install_panic_hook` reads this at panic
/// time to decide whether to emit a report The sentinel file is
/// preferred over an env var so a crash from a background thread still
/// sees the up-to-date opt-in state
pub const OPTIN_SENTINEL_FILENAME: &str = ".crash_reports_enabled";

/// Set or clear the opt-in sentinel file
///
/// # Errors
///
/// IO error when creating / removing the sentinel file
pub fn set_optin(data_dir: &Path, enabled: bool) -> std::io::Result<()> {
    let path = data_dir.join(OPTIN_SENTINEL_FILENAME);
    if enabled {
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(&path, b"1\n")?;
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Query the opt-in sentinel file
#[must_use]
pub fn is_optin(data_dir: &Path) -> bool {
    data_dir.join(OPTIN_SENTINEL_FILENAME).exists()
}

/// Directory where per-crash JSON reports are written
#[must_use]
pub fn crash_reports_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("crash_reports")
}

/// Count the pending (never uploaded) crash reports in the dir Used by
/// the Settings UI to surface a "N crash reports queued" line
#[must_use]
pub fn count_pending(data_dir: &Path) -> usize {
    let dir = crash_reports_dir(data_dir);
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "json")
        })
        .count()
}

/// Install the panic hook Existing hook (default or user-registered)
/// is preserved and chained after our own so `RUST_BACKTRACE` /
/// standard stderr output continues to work
///
/// `app_version` is expected to be `env!("CARGO_PKG_VERSION")`
///
/// The hook writes to `{data_dir}/crash_reports/` only when the
/// [`is_optin`] sentinel is set at panic time
///
/// # Panics
///
/// This function itself does not panic; the installed hook uses
/// `catch_unwind`-friendly IO that swallows all secondary failures
pub fn install_panic_hook(data_dir: PathBuf, app_version: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if is_optin(&data_dir) {
            let report = build_report(info, app_version);
            let _ = report.write_to(&crash_reports_dir(&data_dir));
        }
        previous(info);
    }));
}

fn build_report(info: &std::panic::PanicHookInfo<'_>, app_version: &'static str) -> CrashReport {
    let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    };
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let backtrace = std::backtrace::Backtrace::force_capture().to_string();
    CrashReport {
        schema_version: "1".to_string(),
        uuid: uuid::Uuid::now_v7().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        app_version: app_version.to_string(),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        message,
        location,
        backtrace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optin_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_optin(dir.path()));
        set_optin(dir.path(), true).unwrap();
        assert!(is_optin(dir.path()));
        set_optin(dir.path(), false).unwrap();
        assert!(!is_optin(dir.path()));
    }

    #[test]
    fn crash_report_write_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let report = CrashReport {
            schema_version: "1".to_string(),
            uuid: "018f4e8c-0000-7000-8000-000000000001".to_string(),
            timestamp: "2026-07-29T00:00:00Z".to_string(),
            app_version: "test-0.0.0".to_string(),
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            message: "kaboom".to_string(),
            location: Some("file.rs:1:1".to_string()),
            backtrace: "(none)".to_string(),
        };
        let path = report.write_to(dir.path()).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let back: CrashReport = serde_json::from_str(&content).unwrap();
        assert_eq!(back.uuid, report.uuid);
        assert_eq!(back.message, "kaboom");
    }

    #[test]
    fn count_pending_returns_zero_for_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(count_pending(&dir.path().join("nonexistent")), 0);
    }

    #[test]
    fn count_pending_counts_json_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let reports_dir = crash_reports_dir(dir.path());
        std::fs::create_dir_all(&reports_dir).unwrap();
        std::fs::write(reports_dir.join("a.json"), "{}").unwrap();
        std::fs::write(reports_dir.join("b.json"), "{}").unwrap();
        std::fs::write(reports_dir.join("ignore.txt"), "not json").unwrap();
        assert_eq!(count_pending(dir.path()), 2);
    }
}
