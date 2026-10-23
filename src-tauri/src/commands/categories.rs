//! Income and expense categories (optionally nested one level via parent_id).

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{opt_i64, opt_string, CmdResult, Ctx};

#[derive(Serialize)]
pub struct Category {
    id: i64,
    name: String,
    kind: String,
    parent_id: Option<i64>,
    icon: Option<String>,
    color: Option<String>,
}

fn map_category(r: &Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        parent_id: r.get(3)?,
        icon: r.get(4)?,
        color: r.get(5)?,
    })
}

fn load_category(conn: &Connection, id: i64) -> CmdResult<Category> {
    conn.query_row(
        "SELECT id, name, kind, parent_id, icon, color FROM categories WHERE id = ?1",
        params![id],
        map_category,
    )
    .map_err(|_| "Category not found".to_string())
}

#[derive(Deserialize)]
pub struct CategoryPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    parent_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    icon: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    color: Option<String>,
}

impl CategoryPayload {
    fn validated(&self) -> CmdResult<(String, String)> {
        let name = self.name.clone().ok_or("Category name cannot be empty")?;
        let kind = self.kind.clone().unwrap_or_else(|| "expense".into());
        if kind != "income" && kind != "expense" {
            return Err("Category kind must be either 'income' or 'expense'".into());
        }
        Ok((name, kind))
    }
}

#[tauri::command]
pub fn get_categories(state: State<'_, AppState>) -> CmdResult<Vec<Category>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare("SELECT id, name, kind, parent_id, icon, color FROM categories ORDER BY kind ASC, name ASC")
        .ctx("Failed to load categories")?;
    let rows = stmt.query_map([], map_category).ctx("Failed to load categories")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load categories")
}

#[tauri::command]
pub fn create_category(payload: CategoryPayload, state: State<'_, AppState>) -> CmdResult<Category> {
    let (name, kind) = payload.validated()?;
    let conn = state.conn()?;
    conn.execute(
        "INSERT INTO categories (name, kind, parent_id, icon, color) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, kind, payload.parent_id, payload.icon, payload.color],
    )
    .ctx("Failed to create category")?;
    load_category(&conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn update_category(id: i64, payload: CategoryPayload, state: State<'_, AppState>) -> CmdResult<Category> {
    let (name, kind) = payload.validated()?;
    if payload.parent_id == Some(id) {
        return Err("A category cannot be its own parent".into());
    }
    let conn = state.conn()?;
    conn.execute(
        "UPDATE categories SET name = ?1, kind = ?2, parent_id = ?3, icon = ?4, color = ?5 WHERE id = ?6",
        params![name, kind, payload.parent_id, payload.icon, payload.color, id],
    )
    .ctx("Failed to update category")?;
    load_category(&conn, id)
}

#[tauri::command]
pub fn delete_category(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let count = |sql: &str| -> CmdResult<i64> {
        conn.query_row(sql, params![id], |r| r.get(0)).ctx("Failed to delete category")
    };
    let subcategories = count("SELECT COUNT(*) FROM categories WHERE parent_id = ?1")?;
    if subcategories > 0 {
        return Err(format!(
            "Cannot delete category because it has {subcategories} subcategories. Please reassign or delete the subcategories first."
        ));
    }
    let rules = count("SELECT COUNT(*) FROM recurring_rules WHERE category_id = ?1")?;
    if rules > 0 {
        return Err(format!(
            "Cannot delete category because it is used in {rules} recurring rule(s). Please edit or delete those rules first."
        ));
    }
    let txns = count("SELECT COUNT(*) FROM transactions WHERE category_id = ?1")?;
    if txns > 0 {
        return Err(format!(
            "Cannot delete category because it is used in {txns} transaction(s). Please reassign those transactions first."
        ));
    }
    let tx = conn.transaction().ctx("Failed to delete category")?;
    for sql in [
        "DELETE FROM budget_categories WHERE category_id = ?1",
        "UPDATE bills SET category_id = NULL WHERE category_id = ?1",
        "UPDATE debt_transactions SET category_id = NULL WHERE category_id = ?1",
        "DELETE FROM categories WHERE id = ?1",
    ] {
        tx.execute(sql, params![id]).ctx("Failed to delete category")?;
    }
    tx.commit().ctx("Failed to delete category")
}
