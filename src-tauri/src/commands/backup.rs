//! Database backups (manual + daily automatic), restore and CSV export.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use tauri::State;

use crate::db::{self, AppState};
use crate::util::{CmdResult, Ctx};

const BACKUP_PREFIX: &str = "lyncost_";
const PRE_RESTORE_PREFIX: &str = "lyncost_pre_restore_";
const PRE_WIPE_PREFIX: &str = "lyncost_pre_wipe_";
/// Automatic daily backups kept in the app's backups folder (manual ones are never pruned).
const KEEP_AUTO_BACKUPS: usize = 14;
const AUTO_BACKUP_INTERVAL_HOURS: i64 = 24;

fn backups_dir(state: &AppState) -> CmdResult<PathBuf> {
    let dir = state.data_dir.join("backups");
    fs::create_dir_all(&dir).ctx("Failed to create backups directory")?;
    Ok(dir)
}

fn stamp() -> String {
    Local::now().format("%Y%m%d_%H%M%S").to_string()
}

/// Consistent online copy of the live database.
fn vacuum_into(conn: &Connection, dest: &Path) -> CmdResult<()> {
    if dest.exists() {
        fs::remove_file(dest).ctx("Failed to replace existing file")?;
    }
    conn.execute("VACUUM INTO ?1", params![dest.to_string_lossy()])
        .map_err(|e| format!("Failed to write backup to {}: {e}", dest.display()))?;
    Ok(())
}

/// Safety copy taken right before all data is erased (Forgot PIN / Reset App Data),
/// so a wipe done by mistake can be undone from the backups list.
pub fn write_pre_wipe_backup(state: &AppState, conn: &Connection) -> CmdResult<PathBuf> {
    write_backup(conn, &backups_dir(state)?, PRE_WIPE_PREFIX)
}

fn write_backup(conn: &Connection, folder: &Path, prefix: &str) -> CmdResult<PathBuf> {
    fs::create_dir_all(folder).ctx("Failed to create destination folder")?;
    let dest = folder.join(format!("{prefix}{}.db", stamp()));
    vacuum_into(conn, &dest)?;
    if prefix == BACKUP_PREFIX {
        conn.execute(
            "UPDATE app_settings SET last_backup_at = strftime('%Y-%m-%d %H:%M:%S', 'now', 'localtime') WHERE id = 1",
            [],
        )
        .ctx("Failed to record backup time")?;
    }
    Ok(dest)
}

/// Back up to the app's backups folder, or to `customDestFolder` when given.
/// Returns the full path of the new backup file.
#[tauri::command]
#[allow(non_snake_case)]
pub fn create_backup(customDestFolder: Option<String>, state: State<'_, AppState>) -> CmdResult<String> {
    let folder = match customDestFolder.map(|f| f.trim().to_string()).filter(|f| !f.is_empty()) {
        Some(f) => {
            let p = PathBuf::from(shellexpand_home(&f));
            if !p.is_absolute() {
                return Err("Backup folder must be an absolute path (e.g. /home/you/Backups)".into());
            }
            p
        }
        None => backups_dir(&state)?,
    };
    let conn = state.conn()?;
    let path = write_backup(&conn, &folder, BACKUP_PREFIX)?;
    Ok(path.to_string_lossy().into_owned())
}

/// Accept `~/Backups` as a convenience.
fn shellexpand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

#[derive(Serialize)]
pub struct BackupInfo {
    filename: String,
    file_path: String,
    size_bytes: u64,
    created_at: String,
}

fn list_backups(dir: &Path) -> Vec<BackupInfo> {
    let mut items: Vec<(std::time::SystemTime, BackupInfo)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !name.starts_with(BACKUP_PREFIX) || path.extension()? != "db" {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().ok()?;
            let created_at = chrono::DateTime::<Local>::from(modified).format("%Y-%m-%d %H:%M:%S").to_string();
            Some((
                modified,
                BackupInfo { filename: name, file_path: path.to_string_lossy().into_owned(), size_bytes: meta.len(), created_at },
            ))
        })
        .collect();
    items.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    items.into_iter().map(|(_, info)| info).collect()
}

#[tauri::command]
pub fn get_backups_list(state: State<'_, AppState>) -> CmdResult<Vec<BackupInfo>> {
    Ok(list_backups(&backups_dir(&state)?))
}

/// Automatic backups are named `lyncost_YYYYMMDD_HHMMSS.db`; keep only the newest few.
fn prune_auto_backups(dir: &Path) {
    let is_auto = |name: &str| {
        name.strip_prefix(BACKUP_PREFIX)
            .and_then(|rest| rest.strip_suffix(".db"))
            .is_some_and(|ts| NaiveDateTime::parse_from_str(ts, "%Y%m%d_%H%M%S").is_ok())
    };
    let autos: Vec<BackupInfo> = list_backups(dir).into_iter().filter(|b| is_auto(&b.filename)).collect();
    for old in autos.iter().skip(KEEP_AUTO_BACKUPS) {
        let _ = fs::remove_file(&old.file_path);
    }
}

/// Back up once a day. Returns the new backup's path, or None if one is recent enough.
#[tauri::command]
pub fn check_and_run_daily_backup(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    let conn = state.conn()?;
    let last: Option<String> = conn
        .query_row("SELECT last_backup_at FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .ctx("Failed to read backup settings")?;
    let due = match last.as_deref().and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()) {
        Some(at) => (Local::now().naive_local() - at).num_hours() >= AUTO_BACKUP_INTERVAL_HOURS,
        None => true,
    };
    if !due {
        return Ok(None);
    }
    let dir = backups_dir(&state)?;
    let path = write_backup(&conn, &dir, BACKUP_PREFIX)?;
    prune_auto_backups(&dir);
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn validate_backup(path: &Path) -> CmdResult<()> {
    if !path.is_file() {
        return Err(format!("Backup file not found at {}", path.display()));
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("Selected file is not a valid SQLite database: {e}"))?;
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|e| format!("Selected file is not a valid SQLite database: {e}"))?;
    if integrity != "ok" {
        return Err(format!("Backup file is corrupted ({integrity}). Restore aborted."));
    }
    let is_lyncost: bool = conn
        .query_row(
            "SELECT COUNT(*) = 3 FROM sqlite_master WHERE type = 'table' AND name IN ('app_settings', 'accounts', 'transactions')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !is_lyncost {
        return Err("Selected file is not a valid Lyncost database backup".into());
    }
    Ok(())
}

fn remove_wal_files(db_path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let mut p = db_path.as_os_str().to_owned();
        p.push(suffix);
        let _ = fs::remove_file(PathBuf::from(p));
    }
}

/// Replace the live database with a backup. A safety copy of the current data
/// is written to the backups folder first, and put back if anything fails.
#[tauri::command]
#[allow(non_snake_case)]
pub fn restore_backup(backupFilePath: String, state: State<'_, AppState>) -> CmdResult<()> {
    let source = PathBuf::from(backupFilePath.trim());
    validate_backup(&source)?;

    let db_path = state.db_path();
    let dir = backups_dir(&state)?;
    let mut conn = state.conn()?;

    // Stage the backup first so a failed copy never touches the live database.
    let staged = dir.join(format!(".restore_staging_{}.db", stamp()));
    fs::copy(&source, &staged).ctx("Failed to stage backup file")?;

    let safety = write_backup(&conn, &dir, PRE_RESTORE_PREFIX)
        .map_err(|e| format!("Could not create safety backup, restore aborted: {e}"))?;

    // Close the live connection (checkpointing WAL) before swapping files.
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    *conn = Connection::open_in_memory().ctx("Failed to release database")?;
    remove_wal_files(&db_path);

    let swap = fs::rename(&staged, &db_path).ctx("Failed to replace database file").and_then(|_| db::open(&db_path));
    match swap {
        Ok(new_conn) => {
            *conn = new_conn;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&staged);
            remove_wal_files(&db_path);
            let rollback = fs::copy(&safety, &db_path).map_err(|e| e.to_string()).and_then(|_| db::open(&db_path));
            match rollback {
                Ok(old) => {
                    *conn = old;
                    Err(format!("Restore failed, your previous data was kept: {e}"))
                }
                Err(e2) => Err(format!(
                    "Restore failed ({e}) and the previous database could not be reopened ({e2}). \
                     Your pre-restore data is saved at {}",
                    safety.display()
                )),
            }
        }
    }
}

/// Write an exported CSV into the user's Downloads folder; returns the saved path.
#[tauri::command]
#[allow(non_snake_case)]
pub fn save_export_file(fileName: String, contents: String) -> CmdResult<String> {
    let name = fileName.trim();
    let valid = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '));
    if !valid {
        return Err("Invalid export file name".into());
    }
    let folder = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or("Could not find a folder to save into")?;
    fs::create_dir_all(&folder).ctx("Failed to create Downloads folder")?;

    let path = Path::new(name);
    let (stem, ext) = (
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "export".into()),
        path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default(),
    );
    let mut dest = folder.join(name);
    let mut n = 1;
    while dest.exists() {
        dest = folder.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    // BOM so spreadsheet apps detect UTF-8 (currency symbols, non-Latin names).
    fs::write(&dest, format!("\u{feff}{contents}")).map_err(|e| format!("Failed to save {}: {e}", dest.display()))?;
    Ok(dest.to_string_lossy().into_owned())
}
