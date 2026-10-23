//! Product warranties with expiry tracking.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{opt_string, parse_date, today, CmdResult, Ctx};

/// Warranties ending within this many days are flagged as expiring soon.
const EXPIRING_SOON_DAYS: i64 = 30;

#[derive(Serialize)]
pub struct Warranty {
    id: i64,
    item_name: String,
    purchase_date: Option<String>,
    expires_on: Option<String>,
    transaction_id: Option<i64>,
    notes: Option<String>,
    days_remaining: Option<i64>,
    is_expired: bool,
    is_expiring_soon: bool,
}

const SELECT_WARRANTY: &str = "SELECT id, item_name, purchase_date, expires_on, transaction_id, notes FROM warranties";

fn map_warranty(r: &Row) -> rusqlite::Result<Warranty> {
    let expires_on: Option<String> = r.get(3)?;
    let days = expires_on.as_deref().and_then(parse_date).map(|d| (d - today()).num_days());
    Ok(Warranty {
        id: r.get(0)?,
        item_name: r.get(1)?,
        purchase_date: r.get(2)?,
        expires_on,
        transaction_id: r.get(4)?,
        notes: r.get(5)?,
        days_remaining: days,
        is_expired: days.is_some_and(|d| d < 0),
        is_expiring_soon: days.is_some_and(|d| (0..=EXPIRING_SOON_DAYS).contains(&d)),
    })
}

fn load_warranty(conn: &Connection, id: i64) -> CmdResult<Warranty> {
    conn.query_row(&format!("{SELECT_WARRANTY} WHERE id = ?1"), params![id], map_warranty)
        .optional()
        .ctx("Failed to load warranty")?
        .ok_or_else(|| "Warranty not found".to_string())
}

#[tauri::command]
pub fn get_warranties(state: State<'_, AppState>) -> CmdResult<Vec<Warranty>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT_WARRANTY} ORDER BY CASE WHEN expires_on IS NULL THEN 1 ELSE 0 END, expires_on ASC"
        ))
        .ctx("Failed to load warranties")?;
    let rows = stmt.query_map([], map_warranty).ctx("Failed to load warranties")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load warranties")
}

#[derive(Deserialize)]
pub struct WarrantyPayload {
    #[serde(default, deserialize_with = "opt_string")]
    item_name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    purchase_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    expires_on: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    notes: Option<String>,
}

#[tauri::command]
pub fn create_warranty(payload: WarrantyPayload, state: State<'_, AppState>) -> CmdResult<Warranty> {
    let item = payload.item_name.ok_or("Item name cannot be empty")?;
    let conn = state.conn()?;
    conn.execute(
        "INSERT INTO warranties (item_name, purchase_date, expires_on, notes) VALUES (?1, ?2, ?3, ?4)",
        params![item, payload.purchase_date, payload.expires_on, payload.notes],
    )
    .ctx("Failed to create warranty")?;
    load_warranty(&conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn update_warranty(id: i64, payload: WarrantyPayload, state: State<'_, AppState>) -> CmdResult<Warranty> {
    let item = payload.item_name.ok_or("Item name cannot be empty")?;
    let conn = state.conn()?;
    conn.execute(
        "UPDATE warranties SET item_name = ?1, purchase_date = ?2, expires_on = ?3, notes = ?4 WHERE id = ?5",
        params![item, payload.purchase_date, payload.expires_on, payload.notes, id],
    )
    .ctx("Failed to update warranty")?;
    load_warranty(&conn, id)
}

#[tauri::command]
pub fn delete_warranty(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    state.conn()?.execute("DELETE FROM warranties WHERE id = ?1", params![id]).ctx("Failed to delete warranty")?;
    Ok(())
}
