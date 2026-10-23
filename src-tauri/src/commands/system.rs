//! App lifecycle: update check, restart, and the Windows trial/license gate.

use std::process::Command;

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::AppState;
use crate::util::{CmdResult, Ctx};

/// Published alongside each release; see `version.json` in the repository root.
const VERSION_MANIFEST_URL: &str = "https://raw.githubusercontent.com/UltraDarshan/lyncost/main/version.json";
const RELEASES_PAGE: &str = "https://github.com/UltraDarshan/lyncost/releases/latest";
const TRUSTED_DOWNLOAD_PREFIX: &str = "https://github.com/UltraDarshan/lyncost/";
const TRIAL_DAYS: i64 = 14;

#[derive(Deserialize)]
struct VersionManifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    release_date: Option<String>,
    #[serde(default)]
    release_url: Option<String>,
}

#[derive(Serialize)]
pub struct UpdateInfo {
    has_update: bool,
    current_version: String,
    latest_version: String,
    release_notes: Option<String>,
    published_at: Option<String>,
    download_url: String,
    /// "manual": the download page opens in the browser; the system package
    /// manager (dnf/apt) or installer performs the actual update.
    install_mode: &'static str,
}

fn parse_version(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

fn fetch(url: &str) -> CmdResult<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--proto", "=https", "--tlsv1.2", "--connect-timeout", "10", "--max-time", "15", url]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ctx("Failed to run curl")?;
    if !out.status.success() {
        return Err("Could not reach the update server. Please check your internet connection.".into());
    }
    String::from_utf8(out.stdout).ctx("Invalid response from update server")
}

#[tauri::command]
pub async fn check_app_update() -> CmdResult<UpdateInfo> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let body = tauri::async_runtime::spawn_blocking(|| fetch(VERSION_MANIFEST_URL))
        .await
        .map_err(|e| e.to_string())??;
    let manifest: VersionManifest = serde_json::from_str(&body).ctx("Invalid update manifest")?;
    let download_url = manifest
        .release_url
        .filter(|u| u.starts_with(TRUSTED_DOWNLOAD_PREFIX))
        .unwrap_or_else(|| RELEASES_PAGE.to_string());
    Ok(UpdateInfo {
        has_update: is_newer(&manifest.version, &current),
        current_version: current,
        latest_version: manifest.version.trim_start_matches('v').to_string(),
        release_notes: manifest.notes,
        published_at: manifest.release_date,
        download_url,
        install_mode: "manual",
    })
}

/// Opens the release page; packages are installed by the system package manager.
#[tauri::command]
#[allow(non_snake_case)]
pub fn install_app_update<R: Runtime>(app: AppHandle<R>, downloadUrl: Option<String>) -> CmdResult<()> {
    let url = downloadUrl
        .filter(|u| u.starts_with(TRUSTED_DOWNLOAD_PREFIX))
        .unwrap_or_else(|| RELEASES_PAGE.to_string());
    app.opener().open_url(&url, None::<&str>).ctx("Could not open the download page")?;
    Err(if cfg!(target_os = "linux") {
        "The download page is open in your browser. Install the new .rpm/.deb (or re-run the install script), then restart Lyncost.".into()
    } else {
        "The download page is open in your browser. Run the new installer to update Lyncost.".into()
    })
}

#[tauri::command]
pub fn restart_application<R: Runtime>(app: AppHandle<R>) {
    app.restart();
}

#[derive(Serialize)]
pub struct TrialStatus {
    started_at: String,
    days_total: i64,
    days_remaining: i64,
    is_expired: bool,
}

/// Windows trial clock, started on first call and stored in the database.
/// (The UI only shows the trial gate on Windows; Linux builds are free.)
#[tauri::command]
pub fn get_trial_status(state: State<'_, AppState>) -> CmdResult<TrialStatus> {
    let conn = state.conn()?;
    let stored: Option<String> = conn
        .query_row("SELECT trial_started_at FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .ctx("Failed to read trial status")?;
    let started = match stored.as_deref().and_then(|s| DateTime::parse_from_rfc3339(s).ok()) {
        Some(t) => t.with_timezone(&Utc),
        None => {
            let now = Utc::now();
            conn.execute("UPDATE app_settings SET trial_started_at = ?1 WHERE id = 1", params![now.to_rfc3339()])
                .ctx("Failed to start trial")?;
            now
        }
    };
    let elapsed = (Utc::now() - started).num_days().max(0);
    let remaining = (TRIAL_DAYS - elapsed).max(0);
    Ok(TrialStatus {
        started_at: started.to_rfc3339(),
        days_total: TRIAL_DAYS,
        days_remaining: remaining,
        is_expired: remaining == 0,
    })
}

/// Offline license verification for the paid Windows build.
///
/// TODO(windows-release): restore LYNC2- key verification. The original build
/// checked an Ed25519 signature against a public key that was lost with the
/// source; it must be re-added (from the key generator behind
/// lyncost.vercel.app/license) before shipping a Windows release.
#[tauri::command]
pub fn verify_license_key(key: String) -> bool {
    let _ = key;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.9", "0.1.9"));
        assert!(!is_newer("0.1.8", "0.1.9"));
    }
}
