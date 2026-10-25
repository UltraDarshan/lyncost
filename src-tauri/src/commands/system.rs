//! App lifecycle: update check, restart, and the Windows trial/license gate.
// Windows uses the updater plugin instead of version.json, so the Linux helpers go unused there.
#![cfg_attr(windows, allow(dead_code, unused_imports))]

use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::AppState;
use crate::license::{self, LicenseStatus};
use crate::util::{CmdResult, Ctx};

/// Published alongside each release; see `version.json` in the repository root.
const VERSION_MANIFEST_URL: &str = "https://raw.githubusercontent.com/UltraDarshan/lyncost/main/version.json";
const RELEASES_PAGE: &str = "https://github.com/UltraDarshan/lyncost/releases/latest";
const TRUSTED_DOWNLOAD_PREFIX: &str = "https://github.com/UltraDarshan/lyncost/";

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

/// Linux: compares with `version.json`; the package manager installs the update.
#[cfg(not(windows))]
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

/// Windows: the signed update feed (`latest.json` on GitHub Releases, checked against the
/// updater public key in tauri.windows.conf.json), installed in place by the app itself.
#[cfg(windows)]
#[tauri::command]
pub async fn check_app_update<R: Runtime>(app: AppHandle<R>) -> CmdResult<UpdateInfo> {
    use tauri_plugin_updater::UpdaterExt;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let update = app
        .updater()
        .ctx("Updates are unavailable")?
        .check()
        .await
        .map_err(|_| "Could not reach the update server. Please check your internet connection.".to_string())?;
    Ok(match update {
        Some(u) => UpdateInfo {
            has_update: true,
            current_version: current,
            latest_version: u.version.clone(),
            release_notes: u.body.clone(),
            published_at: u.date.map(|d| d.to_string()),
            download_url: RELEASES_PAGE.to_string(),
            install_mode: "installer",
        },
        None => UpdateInfo {
            has_update: false,
            latest_version: current.clone(),
            current_version: current,
            release_notes: None,
            published_at: None,
            download_url: RELEASES_PAGE.to_string(),
            install_mode: "installer",
        },
    })
}

/// Linux: opens the release page; packages are installed by the system package manager.
#[cfg(not(windows))]
#[tauri::command]
#[allow(non_snake_case)]
pub fn install_app_update<R: Runtime>(app: AppHandle<R>, downloadUrl: Option<String>) -> CmdResult<()> {
    let url = downloadUrl
        .filter(|u| u.starts_with(TRUSTED_DOWNLOAD_PREFIX))
        .unwrap_or_else(|| RELEASES_PAGE.to_string());
    app.opener().open_url(&url, None::<&str>).ctx("Could not open the download page")?;
    Err("The download page is open in your browser. Install the new .rpm/.deb (or re-run the install script), then restart Lyncost.".into())
}

/// Windows: downloads the signed installer, verifies it, and runs it. The installer closes
/// Lyncost, updates it in place (data and license are kept), and opens it again.
#[cfg(windows)]
#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_app_update<R: Runtime>(app: AppHandle<R>, downloadUrl: Option<String>) -> CmdResult<()> {
    use tauri_plugin_updater::UpdaterExt;
    let _ = downloadUrl;
    let update = app
        .updater()
        .ctx("Updates are unavailable")?
        .check()
        .await
        .map_err(|_| "Could not reach the update server. Please check your internet connection.".to_string())?
        .ok_or_else(|| "You already have the latest version.".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("The update could not be installed: {e}"))?;
    app.restart();
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

/// Trial clock for the activation screens (see `license.rs`). Starts on the first run.
#[tauri::command]
pub fn get_trial_status(state: State<'_, AppState>) -> CmdResult<TrialStatus> {
    let s = license::current_status(&*state.conn()?)?;
    let is_expired = !s.unlocked();
    Ok(TrialStatus {
        started_at: s.trial_started_at,
        days_total: s.days_total,
        days_remaining: s.days_remaining,
        is_expired,
    })
}

/// Full license state: free (Linux), trial, licensed, or expired.
#[tauri::command]
pub fn get_license_status(state: State<'_, AppState>) -> CmdResult<LicenseStatus> {
    license::current_status(&*state.conn()?)
}

/// Checks a key and, when it is genuine, activates it on this computer.
/// (The activation screen calls this and only shows success when it returns true.)
#[tauri::command]
pub fn verify_license_key(state: State<'_, AppState>, key: String) -> bool {
    let Ok(conn) = state.conn() else { return false };
    license::activate(&conn, &license::system_stores(), &license::Sealer::system(), &key).is_some()
}

/// QA builds only: move the app's clock forward to test the end of the trial.
/// Does nothing in release builds.
#[tauri::command]
pub fn qa_set_clock_offset(hours: i64) {
    #[cfg(feature = "qa")]
    license::QA_CLOCK_OFFSET_HOURS.store(hours, std::sync::atomic::Ordering::Relaxed);
    let _ = hours;
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
