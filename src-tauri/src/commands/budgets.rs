//! Budgets with live spending, period windows, rollover and pace warnings.

use chrono::{Duration, NaiveDate};
use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{
    add_months, fmt_date, opt_bool, opt_f64, opt_string, parse_date, round2, today, vec_i64, CmdResult, Ctx,
};

#[derive(Serialize)]
pub struct Budget {
    id: i64,
    name: String,
    amount: f64,
    period: String,
    start_date: String,
    end_date: Option<String>,
    /// bool, not 0/1: the UI renders `rollover && <badge>`, which would print a stray "0".
    rollover: bool,
    is_active: i64,
    category_ids: Vec<i64>,
    category_names: Vec<String>,
    effective_amount: f64,
    spent: f64,
    remaining: f64,
    progress_percent: f64,
    period_start: String,
    period_end: String,
    total_days: i64,
    days_elapsed: i64,
    days_remaining: i64,
    projected_spent: f64,
    is_pace_warning: bool,
}

struct BudgetRow {
    id: i64,
    name: String,
    amount: f64,
    period: String,
    start_date: String,
    end_date: Option<String>,
    rollover: i64,
    is_active: i64,
}

/// The period window (inclusive) that contains `on`, or the first window if `on` is earlier.
fn window(period: &str, start: NaiveDate, end: Option<NaiveDate>, on: NaiveDate) -> (NaiveDate, NaiveDate) {
    match period {
        "weekly" => {
            let k = ((on - start).num_days().max(0)) / 7;
            let s = start + Duration::days(k * 7);
            (s, s + Duration::days(6))
        }
        "monthly" => {
            let anchor = Some(chrono::Datelike::day(&start));
            let mut s = start;
            loop {
                let next = add_months(start, months_between(start, s) + 1, anchor);
                if next > on {
                    return (s, next - Duration::days(1));
                }
                s = next;
            }
        }
        _ => (start, end.unwrap_or(start).max(start)),
    }
}

fn months_between(a: NaiveDate, b: NaiveDate) -> i32 {
    use chrono::Datelike;
    (b.year() - a.year()) * 12 + b.month() as i32 - a.month() as i32
}

fn spent_between(conn: &Connection, category_ids: &[i64], from: NaiveDate, to: NaiveDate) -> rusqlite::Result<f64> {
    if category_ids.is_empty() {
        return Ok(0.0);
    }
    let marks = vec!["?"; category_ids.len()].join(",");
    let sql = format!(
        "SELECT ROUND(COALESCE(SUM(t.base_amount), 0), 2) FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.type = 'expense' AND t.is_confirmed = 1
           AND (t.category_id IN ({marks}) OR c.parent_id IN ({marks}))
           AND DATE(t.txn_date) >= ? AND DATE(t.txn_date) <= ?"
    );
    let mut args: Vec<Value> = category_ids.iter().chain(category_ids).map(|id| Value::Integer(*id)).collect();
    args.push(Value::Text(fmt_date(from)));
    args.push(Value::Text(fmt_date(to)));
    conn.query_row(&sql, params_from_iter(args), |r| r.get(0))
}

fn build(conn: &Connection, b: BudgetRow) -> CmdResult<Budget> {
    let (category_ids, category_names): (Vec<i64>, Vec<String>) = conn
        .prepare(
            "SELECT c.id, c.name FROM categories c
             JOIN budget_categories bc ON bc.category_id = c.id
             WHERE bc.budget_id = ?1 ORDER BY c.name",
        )
        .and_then(|mut s| {
            s.query_map(params![b.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()
        })
        .ctx("Failed to load budget categories")?
        .into_iter()
        .unzip();

    let today = today();
    let start = parse_date(&b.start_date).unwrap_or(today);
    let end = b.end_date.as_deref().and_then(parse_date);
    let (p_start, p_end) = window(&b.period, start, end, today);

    let spent = spent_between(conn, &category_ids, p_start, p_end).ctx("Failed to compute budget spending")?;
    let mut effective = b.amount;
    if b.rollover != 0 && b.period != "custom" && p_start > start {
        let (prev_start, prev_end) = window(&b.period, start, end, p_start - Duration::days(1));
        let prev_spent =
            spent_between(conn, &category_ids, prev_start, prev_end).ctx("Failed to compute budget rollover")?;
        effective += (b.amount - prev_spent).max(0.0);
    }

    let total_days = (p_end - p_start).num_days() + 1;
    let days_elapsed = ((today - p_start).num_days() + 1).clamp(0, total_days);
    let projected = if days_elapsed > 0 { spent / days_elapsed as f64 * total_days as f64 } else { 0.0 };

    Ok(Budget {
        id: b.id,
        name: b.name,
        amount: b.amount,
        period: b.period,
        start_date: b.start_date,
        end_date: b.end_date,
        rollover: b.rollover != 0,
        is_active: b.is_active,
        category_ids,
        category_names,
        effective_amount: round2(effective),
        spent,
        remaining: round2(effective - spent),
        progress_percent: if effective > 0.0 { round2(spent / effective * 100.0) } else { 0.0 },
        period_start: fmt_date(p_start),
        period_end: fmt_date(p_end),
        total_days,
        days_elapsed,
        days_remaining: total_days - days_elapsed,
        projected_spent: round2(projected),
        is_pace_warning: days_elapsed < total_days && spent <= effective && projected > effective,
    })
}

fn load_rows(conn: &Connection, id: Option<i64>) -> CmdResult<Vec<BudgetRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, amount, period, start_date, end_date, COALESCE(rollover, 0), COALESCE(is_active, 1)
             FROM budgets WHERE ?1 IS NULL OR id = ?1 ORDER BY id ASC",
        )
        .ctx("Failed to load budgets")?;
    let rows = stmt
        .query_map(params![id], |r| {
            Ok(BudgetRow {
                id: r.get(0)?,
                name: r.get(1)?,
                amount: r.get(2)?,
                period: r.get(3)?,
                start_date: r.get(4)?,
                end_date: r.get(5)?,
                rollover: r.get(6)?,
                is_active: r.get(7)?,
            })
        })
        .ctx("Failed to load budgets")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load budgets")
}

fn load_budget(conn: &Connection, id: i64) -> CmdResult<Budget> {
    let row = load_rows(conn, Some(id))?.into_iter().next().ok_or("Budget not found")?;
    build(conn, row)
}

#[tauri::command]
pub fn get_budgets(state: State<'_, AppState>) -> CmdResult<Vec<Budget>> {
    let conn = state.conn()?;
    load_rows(&conn, None)?.into_iter().map(|b| build(&conn, b)).collect()
}

#[derive(Deserialize)]
pub struct BudgetPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    period: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    start_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    end_date: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    rollover: Option<bool>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_active: Option<bool>,
    #[serde(default, deserialize_with = "vec_i64")]
    category_ids: Vec<i64>,
}

struct ValidBudget {
    name: String,
    amount: f64,
    period: String,
    start_date: String,
    end_date: Option<String>,
}

fn validate(p: &BudgetPayload) -> CmdResult<ValidBudget> {
    let name = p.name.clone().ok_or("Budget name cannot be empty")?;
    let amount = p.amount.filter(|a| *a > 0.0).ok_or("Budget amount must be greater than zero")?;
    let period = p.period.clone().unwrap_or_else(|| "monthly".into());
    if !matches!(period.as_str(), "weekly" | "monthly" | "custom") {
        return Err(format!("Invalid budget period: {period}"));
    }
    if p.category_ids.is_empty() {
        return Err("Please select at least one category for this budget".into());
    }
    let start_date = p.start_date.clone().unwrap_or_else(|| fmt_date(today()));
    let end_date = if period == "custom" {
        let end = p.end_date.clone().ok_or("End date is required for custom period budgets")?;
        if parse_date(&end) < parse_date(&start_date) {
            return Err("End date cannot be before the start date".into());
        }
        Some(end)
    } else {
        None
    };
    Ok(ValidBudget { name, amount: round2(amount), period, start_date, end_date })
}

fn set_categories(conn: &Connection, budget_id: i64, ids: &[i64]) -> CmdResult<()> {
    conn.execute("DELETE FROM budget_categories WHERE budget_id = ?1", params![budget_id])
        .ctx("Failed to save budget categories")?;
    for id in ids {
        conn.execute(
            "INSERT OR IGNORE INTO budget_categories (budget_id, category_id) VALUES (?1, ?2)",
            params![budget_id, id],
        )
        .ctx("Failed to save budget categories")?;
    }
    Ok(())
}

#[tauri::command]
pub fn create_budget(payload: BudgetPayload, state: State<'_, AppState>) -> CmdResult<Budget> {
    let v = validate(&payload)?;
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to create budget")?;
    tx.execute(
        "INSERT INTO budgets (name, amount, period, start_date, end_date, rollover, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
        params![v.name, v.amount, v.period, v.start_date, v.end_date, payload.rollover.unwrap_or(false)],
    )
    .ctx("Failed to create budget")?;
    let id = tx.last_insert_rowid();
    set_categories(&tx, id, &payload.category_ids)?;
    tx.commit().ctx("Failed to create budget")?;
    load_budget(&conn, id)
}

#[tauri::command]
pub fn update_budget(id: i64, payload: BudgetPayload, state: State<'_, AppState>) -> CmdResult<Budget> {
    let v = validate(&payload)?;
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to update budget")?;
    tx.execute(
        "UPDATE budgets SET name = ?1, amount = ?2, period = ?3, start_date = ?4, end_date = ?5,
             rollover = ?6, is_active = ?7
         WHERE id = ?8",
        params![
            v.name,
            v.amount,
            v.period,
            v.start_date,
            v.end_date,
            payload.rollover.unwrap_or(false),
            payload.is_active.unwrap_or(true),
            id
        ],
    )
    .ctx("Failed to update budget")?;
    set_categories(&tx, id, &payload.category_ids)?;
    tx.commit().ctx("Failed to update budget")?;
    load_budget(&conn, id)
}

#[tauri::command]
pub fn delete_budget(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.conn()?;
    conn.execute("DELETE FROM budget_categories WHERE budget_id = ?1", params![id]).ctx("Failed to delete budget")?;
    conn.execute("DELETE FROM budgets WHERE id = ?1", params![id]).ctx("Failed to delete budget")?;
    Ok(())
}

#[derive(Serialize)]
pub struct MonthSummary {
    month: String,
    total_income: f64,
    total_expense: f64,
    net_cashflow: f64,
    pending_count: i64,
    pending_expense_total: f64,
}

/// Confirmed income/expense for a month (`YYYY-MM`, default: current month).
#[tauri::command]
pub fn get_month_summary(month: Option<String>, state: State<'_, AppState>) -> CmdResult<MonthSummary> {
    let month = month
        .and_then(|m| m.get(..7).map(str::to_string))
        .unwrap_or_else(|| today().format("%Y-%m").to_string());
    let like = format!("{month}%");
    let conn = state.conn()?;
    let sum = |sql: &str| -> CmdResult<f64> { conn.query_row(sql, params![like], |r| r.get(0)).ctx("Failed to load summary") };
    let income = sum(
        "SELECT ROUND(COALESCE(SUM(base_amount), 0), 2) FROM transactions WHERE type = 'income' AND is_confirmed = 1 AND txn_date LIKE ?1",
    )?;
    let expense = sum(
        "SELECT ROUND(COALESCE(SUM(base_amount), 0), 2) FROM transactions WHERE type = 'expense' AND is_confirmed = 1 AND txn_date LIKE ?1",
    )?;
    let pending_expense = sum(
        "SELECT ROUND(COALESCE(SUM(base_amount), 0), 2) FROM transactions WHERE type = 'expense' AND is_confirmed = 0 AND txn_date LIKE ?1",
    )?;
    let pending_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM transactions WHERE is_confirmed = 0 AND txn_date LIKE ?1", params![like], |r| r.get(0))
        .ctx("Failed to load summary")?;
    Ok(MonthSummary {
        month,
        total_income: income,
        total_expense: expense,
        net_cashflow: round2(income - expense),
        pending_count,
        pending_expense_total: pending_expense,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn monthly_window_follows_anchor_day() {
        assert_eq!(window("monthly", d("2026-01-01"), None, d("2026-10-23")), (d("2026-10-01"), d("2026-10-31")));
        assert_eq!(window("monthly", d("2026-01-15"), None, d("2026-10-23")), (d("2026-10-15"), d("2026-11-14")));
        assert_eq!(window("monthly", d("2026-01-31"), None, d("2026-02-28")), (d("2026-02-28"), d("2026-03-30")));
        // Before the budget starts: the first window.
        assert_eq!(window("monthly", d("2026-12-01"), None, d("2026-10-23")), (d("2026-12-01"), d("2026-12-31")));
    }

    #[test]
    fn weekly_and_custom_windows() {
        assert_eq!(window("weekly", d("2026-10-01"), None, d("2026-10-23")), (d("2026-10-22"), d("2026-10-28")));
        assert_eq!(
            window("custom", d("2026-10-01"), Some(d("2026-12-31")), d("2026-10-23")),
            (d("2026-10-01"), d("2026-12-31"))
        );
    }
}
