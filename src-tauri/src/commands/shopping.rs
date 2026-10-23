//! Simple shopping list.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{opt_string, CmdResult, Ctx};

#[derive(Serialize)]
pub struct ShoppingItem {
    id: i64,
    name: String,
    is_checked: i64,
    note: Option<String>,
    created_at: Option<String>,
}

const SELECT_ITEM: &str = "SELECT id, name, COALESCE(is_checked, 0), note, created_at FROM shopping_list_items";

fn map_item(r: &Row) -> rusqlite::Result<ShoppingItem> {
    Ok(ShoppingItem { id: r.get(0)?, name: r.get(1)?, is_checked: r.get(2)?, note: r.get(3)?, created_at: r.get(4)? })
}

fn load_item(conn: &Connection, id: i64) -> CmdResult<ShoppingItem> {
    conn.query_row(&format!("{SELECT_ITEM} WHERE id = ?1"), params![id], map_item)
        .optional()
        .ctx("Failed to load shopping item")?
        .ok_or_else(|| "Shopping item not found".to_string())
}

#[derive(Deserialize)]
pub struct ShoppingPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
}

#[tauri::command]
pub fn get_shopping_items(state: State<'_, AppState>) -> CmdResult<Vec<ShoppingItem>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!("{SELECT_ITEM} ORDER BY is_checked ASC, id DESC"))
        .ctx("Failed to load shopping list")?;
    let rows = stmt.query_map([], map_item).ctx("Failed to load shopping list")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load shopping list")
}

#[tauri::command]
pub fn create_shopping_item(payload: ShoppingPayload, state: State<'_, AppState>) -> CmdResult<ShoppingItem> {
    let name = payload.name.ok_or("Item name cannot be empty")?;
    let conn = state.conn()?;
    conn.execute("INSERT INTO shopping_list_items (name, note) VALUES (?1, ?2)", params![name, payload.note])
        .ctx("Failed to create shopping item")?;
    load_item(&conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn update_shopping_item(id: i64, payload: ShoppingPayload, state: State<'_, AppState>) -> CmdResult<ShoppingItem> {
    let name = payload.name.ok_or("Item name cannot be empty")?;
    let conn = state.conn()?;
    conn.execute("UPDATE shopping_list_items SET name = ?1, note = ?2 WHERE id = ?3", params![name, payload.note, id])
        .ctx("Failed to update shopping item")?;
    load_item(&conn, id)
}

#[tauri::command]
pub fn toggle_shopping_item(id: i64, state: State<'_, AppState>) -> CmdResult<ShoppingItem> {
    let conn = state.conn()?;
    conn.execute("UPDATE shopping_list_items SET is_checked = 1 - COALESCE(is_checked, 0) WHERE id = ?1", params![id])
        .ctx("Failed to toggle shopping item")?;
    load_item(&conn, id)
}

#[tauri::command]
pub fn delete_shopping_item(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("DELETE FROM shopping_list_items WHERE id = ?1", params![id])
        .ctx("Failed to delete shopping item")?;
    Ok(())
}

#[tauri::command]
pub fn clear_completed_shopping_items(state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("DELETE FROM shopping_list_items WHERE is_checked = 1", [])
        .ctx("Failed to clear completed shopping items")?;
    Ok(())
}
