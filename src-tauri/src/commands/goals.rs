//! Savings goals with deposit/withdrawal history.

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{opt_bool, opt_f64, opt_i64, opt_string, round2, CmdResult, Ctx};

#[derive(Serialize)]
pub struct Goal {
    id: i64,
    name: String,
    target_amount: f64,
    current_amount: f64,
    target_date: Option<String>,
    account_id: Option<i64>,
    color: Option<String>,
    icon: Option<String>,
    note: Option<String>,
    is_reached: i64,
    created_at: Option<String>,
    remaining_amount: f64,
    percentage: f64,
}

const GOAL_COLUMNS: &str =
    "id, name, target_amount, current_amount, target_date, account_id, color, icon, note, is_reached, created_at";

fn map_goal(r: &Row) -> rusqlite::Result<Goal> {
    let target: f64 = r.get(2)?;
    let current: f64 = r.get(3)?;
    Ok(Goal {
        id: r.get(0)?,
        name: r.get(1)?,
        target_amount: target,
        current_amount: current,
        target_date: r.get(4)?,
        account_id: r.get(5)?,
        color: r.get(6)?,
        icon: r.get(7)?,
        note: r.get(8)?,
        is_reached: r.get::<_, Option<i64>>(9)?.unwrap_or(0),
        created_at: r.get(10)?,
        remaining_amount: round2((target - current).max(0.0)),
        percentage: if target > 0.0 { round2(current / target * 100.0) } else { 0.0 },
    })
}

fn load_goal(conn: &Connection, id: i64) -> CmdResult<Goal> {
    conn.query_row(&format!("SELECT {GOAL_COLUMNS} FROM goals WHERE id = ?1"), params![id], map_goal)
        .map_err(|_| "Goal not found".to_string())
}

#[tauri::command]
pub fn get_goals(state: State<'_, AppState>) -> CmdResult<Vec<Goal>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!("SELECT {GOAL_COLUMNS} FROM goals ORDER BY is_reached ASC, id DESC"))
        .ctx("Failed to load goals")?;
    let rows = stmt.query_map([], map_goal).ctx("Failed to load goals")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load goals")
}

#[derive(Deserialize)]
pub struct GoalPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    target_amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    current_amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    target_date: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    color: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    icon: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_reached: Option<bool>,
}

impl GoalPayload {
    fn validated(&self) -> CmdResult<(String, f64, f64)> {
        let name = self.name.clone().ok_or("Goal name is required")?;
        let target = self.target_amount.filter(|t| *t > 0.0).ok_or("Target amount must be greater than zero")?;
        let current = self.current_amount.unwrap_or(0.0).max(0.0);
        Ok((name, round2(target), round2(current)))
    }
}

#[tauri::command]
pub fn create_goal(payload: GoalPayload, state: State<'_, AppState>) -> CmdResult<Goal> {
    let (name, target, current) = payload.validated()?;
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to create goal")?;
    tx.execute(
        "INSERT INTO goals (name, target_amount, current_amount, target_date, account_id, color, icon, note, is_reached)
         VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, '#8b5cf6'), COALESCE(?7, 'target'), ?8, ?9)",
        params![
            name, target, current, payload.target_date, payload.account_id,
            payload.color, payload.icon, payload.note, current >= target
        ],
    )
    .ctx("Failed to create goal")?;
    let id = tx.last_insert_rowid();
    if current > 0.0 {
        tx.execute(
            "INSERT INTO goal_contributions (goal_id, amount, note) VALUES (?1, ?2, 'Initial deposit')",
            params![id, current],
        )
        .ctx("Failed to create goal")?;
    }
    tx.commit().ctx("Failed to create goal")?;
    load_goal(&conn, id)
}

#[tauri::command]
pub fn update_goal(id: i64, payload: GoalPayload, state: State<'_, AppState>) -> CmdResult<Goal> {
    let (name, target, current) = payload.validated()?;
    let conn = state.conn()?;
    let reached = payload.is_reached.unwrap_or(false) || current >= target;
    conn.execute(
        "UPDATE goals SET name = ?1, target_amount = ?2, current_amount = ?3, target_date = ?4,
             account_id = ?5, color = COALESCE(?6, color), icon = COALESCE(?7, icon), note = ?8, is_reached = ?9
         WHERE id = ?10",
        params![
            name, target, current, payload.target_date, payload.account_id,
            payload.color, payload.icon, payload.note, reached, id
        ],
    )
    .ctx("Failed to update goal")?;
    load_goal(&conn, id)
}

#[tauri::command]
pub fn delete_goal(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.conn()?;
    conn.execute("DELETE FROM goal_contributions WHERE goal_id = ?1", params![id]).ctx("Failed to delete goal")?;
    conn.execute("DELETE FROM goals WHERE id = ?1", params![id]).ctx("Failed to delete goal")?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ContributionPayload {
    #[serde(default, deserialize_with = "opt_i64")]
    goal_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
}

/// Positive amounts deposit, negative amounts withdraw (never below zero).
#[tauri::command]
pub fn contribute_to_goal(payload: ContributionPayload, state: State<'_, AppState>) -> CmdResult<Goal> {
    let goal_id = payload.goal_id.ok_or("Goal not found")?;
    let amount = round2(payload.amount.filter(|a| *a != 0.0).ok_or("Please enter a valid amount.")?);
    let mut conn = state.conn()?;
    load_goal(&conn, goal_id)?;
    let tx = conn.transaction().ctx("Failed to save contribution")?;
    tx.execute(
        "INSERT INTO goal_contributions (goal_id, amount, note) VALUES (?1, ?2, ?3)",
        params![goal_id, amount, payload.note],
    )
    .ctx("Failed to save contribution")?;
    tx.execute(
        "UPDATE goals SET current_amount = MAX(0, ROUND(current_amount + ?1, 2)),
             is_reached = CASE WHEN MAX(0, current_amount + ?1) >= target_amount THEN 1 ELSE 0 END
         WHERE id = ?2",
        params![amount, goal_id],
    )
    .ctx("Failed to save contribution")?;
    tx.commit().ctx("Failed to save contribution")?;
    load_goal(&conn, goal_id)
}
