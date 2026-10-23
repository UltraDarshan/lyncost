//! Net worth (live + daily snapshots) and the Reports page aggregations.

use chrono::Duration;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::fx;
use crate::util::{fmt_date, opt_i64, opt_string, parse_date, round2, today, today_str, CmdResult, Ctx};

/// Ranges up to this many days are charted per day instead of per month.
const DAILY_TREND_MAX_DAYS: i64 = 45;

#[derive(Serialize)]
pub struct NetWorthSummary {
    base_currency: String,
    total_bank: f64,
    total_cash: f64,
    total_investments: f64,
    total_other_accounts: f64,
    total_accounts: f64,
    total_debts: f64,
    net_worth: f64,
}

fn compute_net_worth(conn: &Connection) -> CmdResult<NetWorthSummary> {
    let base = fx::base_currency(conn);
    let (mut bank, mut cash, mut other) = (0.0, 0.0, 0.0);
    let accounts: Vec<(String, String, f64)> = conn
        .prepare("SELECT type, currency, current_balance FROM accounts WHERE is_archived = 0")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())
        .ctx("Failed to compute net worth")?;
    for (kind, currency, balance) in accounts {
        let value = fx::convert(conn, balance, &currency, &base);
        match kind.as_str() {
            "bank" => bank += value,
            "cash" => cash += value,
            _ => other += value,
        }
    }
    let holdings: Vec<(f64, f64, String)> = conn
        .prepare("SELECT quantity, last_price, currency FROM investment_holdings WHERE is_archived = 0")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())
        .ctx("Failed to compute net worth")?;
    let investments: f64 = holdings.iter().map(|(q, p, c)| fx::convert(conn, q * p, c, &base)).sum();
    // Debts are entered in the base currency.
    let debts: f64 = conn
        .query_row("SELECT COALESCE(SUM(current_balance), 0.0) FROM debts WHERE is_active = 1", [], |r| r.get(0))
        .ctx("Failed to compute net worth")?;
    let accounts_total = bank + cash + other;
    Ok(NetWorthSummary {
        base_currency: base,
        total_bank: round2(bank),
        total_cash: round2(cash),
        total_investments: round2(investments),
        total_other_accounts: round2(other),
        total_accounts: round2(accounts_total),
        total_debts: round2(debts),
        net_worth: round2(accounts_total + investments - debts),
    })
}

#[tauri::command]
pub fn get_net_worth_summary(state: State<'_, AppState>) -> CmdResult<NetWorthSummary> {
    compute_net_worth(&*state.conn()?)
}

#[derive(Serialize)]
pub struct NetWorthSnapshot {
    id: i64,
    snapshot_date: String,
    total_accounts: f64,
    total_investments: f64,
    total_debts: f64,
    net_worth: f64,
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_net_worth_history(limitDays: Option<i64>, state: State<'_, AppState>) -> CmdResult<Vec<NetWorthSnapshot>> {
    let since = limitDays.filter(|d| *d > 0).map(|d| fmt_date(today() - Duration::days(d)));
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, snapshot_date, total_accounts, total_investments, total_debts, net_worth
             FROM networth_history WHERE ?1 IS NULL OR snapshot_date >= ?1
             ORDER BY snapshot_date ASC, id ASC",
        )
        .ctx("Failed to load net worth history")?;
    let rows = stmt
        .query_map(params![since], |r| {
            Ok(NetWorthSnapshot {
                id: r.get(0)?,
                snapshot_date: r.get(1)?,
                total_accounts: r.get(2)?,
                total_investments: r.get(3)?,
                total_debts: r.get(4)?,
                net_worth: r.get(5)?,
            })
        })
        .ctx("Failed to load net worth history")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load net worth history")
}

/// Store today's (or `date`'s) net worth, replacing an existing snapshot for that day.
fn snapshot(conn: &Connection, date: &str) -> CmdResult<()> {
    let s = compute_net_worth(conn)?;
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM networth_history WHERE snapshot_date = ?1", params![date], |r| r.get(0))
        .optional()
        .ctx("Failed to save snapshot")?;
    match existing {
        Some(id) => conn.execute(
            "UPDATE networth_history SET total_accounts = ?1, total_investments = ?2, total_debts = ?3, net_worth = ?4
             WHERE id = ?5",
            params![s.total_accounts, s.total_investments, s.total_debts, s.net_worth, id],
        ),
        None => conn.execute(
            "INSERT INTO networth_history (snapshot_date, total_accounts, total_investments, total_debts, net_worth)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![date, s.total_accounts, s.total_investments, s.total_debts, s.net_worth],
        ),
    }
    .ctx("Failed to save snapshot")?;
    conn.execute("UPDATE app_settings SET last_networth_snapshot_at = CURRENT_TIMESTAMP WHERE id = 1", [])
        .ctx("Failed to save snapshot")?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn record_net_worth_snapshot(snapshotDate: Option<String>, state: State<'_, AppState>) -> CmdResult<()> {
    let date = snapshotDate.as_deref().and_then(parse_date).map(fmt_date).unwrap_or_else(today_str);
    snapshot(&*state.conn()?, &date)
}

/// Take at most one automatic snapshot per day. Returns true when one was taken.
#[tauri::command]
pub fn check_and_snapshot_net_worth(state: State<'_, AppState>) -> CmdResult<bool> {
    let conn = state.conn()?;
    let today = today_str();
    let exists: i64 = conn
        .query_row("SELECT COUNT(*) FROM networth_history WHERE snapshot_date = ?1", params![today], |r| r.get(0))
        .ctx("Failed to check snapshots")?;
    if exists > 0 {
        return Ok(false);
    }
    snapshot(&conn, &today)?;
    Ok(true)
}

#[derive(Deserialize, Default)]
pub struct ReportFilter {
    #[serde(default, deserialize_with = "opt_string")]
    start_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    end_date: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    group_by: Option<String>,
}

impl ReportFilter {
    /// SQL conditions (prefixed with AND) and their arguments.
    fn conditions(&self) -> (String, Vec<Box<dyn ToSql>>) {
        let mut sql = String::new();
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        if let Some(start) = &self.start_date {
            sql.push_str(" AND DATE(t.txn_date) >= DATE(?)");
            args.push(Box::new(start.clone()));
        }
        if let Some(end) = &self.end_date {
            sql.push_str(" AND DATE(t.txn_date) <= DATE(?)");
            args.push(Box::new(end.clone()));
        }
        if let Some(acc) = self.account_id {
            sql.push_str(" AND t.account_id = ?");
            args.push(Box::new(acc));
        }
        (sql, args)
    }

    fn daily(&self) -> bool {
        match self.group_by.as_deref() {
            Some("day") => true,
            Some("month") => false,
            _ => match (
                self.start_date.as_deref().and_then(parse_date),
                self.end_date.as_deref().and_then(parse_date),
            ) {
                (Some(s), Some(e)) => (e - s).num_days() <= DAILY_TREND_MAX_DAYS,
                _ => false,
            },
        }
    }
}

#[derive(Serialize)]
pub struct CategorySpending {
    category_id: Option<i64>,
    category_name: String,
    kind: String,
    icon: Option<String>,
    color: Option<String>,
    total_spent: f64,
    percentage: f64,
    transaction_count: i64,
}

#[tauri::command]
pub fn get_category_spending_report(
    filter: Option<ReportFilter>,
    state: State<'_, AppState>,
) -> CmdResult<Vec<CategorySpending>> {
    let filter = filter.unwrap_or_default();
    let (cond, args) = filter.conditions();
    let sql = format!(
        "SELECT t.category_id, COALESCE(c.name, 'Uncategorized'), COALESCE(c.kind, 'expense'), c.icon, c.color,
                ROUND(SUM(t.base_amount), 2), COUNT(t.id)
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         WHERE t.type = 'expense' AND t.is_confirmed = 1{cond}
         GROUP BY t.category_id
         ORDER BY 6 DESC"
    );
    let conn = state.conn()?;
    let mut rows: Vec<CategorySpending> = conn
        .prepare(&sql)
        .and_then(|mut s| {
            s.query_map(params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
                Ok(CategorySpending {
                    category_id: r.get(0)?,
                    category_name: r.get(1)?,
                    kind: r.get(2)?,
                    icon: r.get(3)?,
                    color: r.get(4)?,
                    total_spent: r.get(5)?,
                    percentage: 0.0,
                    transaction_count: r.get(6)?,
                })
            })?
            .collect()
        })
        .ctx("Failed to load spending report")?;
    let total: f64 = rows.iter().map(|r| r.total_spent).sum();
    for row in &mut rows {
        row.percentage = if total > 0.0 { round2(row.total_spent / total * 100.0) } else { 0.0 };
    }
    Ok(rows)
}

#[derive(Serialize)]
pub struct TrendPoint {
    period: String,
    period_label: String,
    income: f64,
    expense: f64,
    net: f64,
    total_income: f64,
    total_expense: f64,
    net_cashflow: f64,
}

fn period_label(period: &str, daily: bool) -> String {
    let parsed = if daily { parse_date(period) } else { parse_date(&format!("{period}-01")) };
    match parsed {
        Some(d) if daily => d.format("%b %d").to_string(),
        Some(d) => d.format("%b %Y").to_string(),
        None => period.to_string(),
    }
}

#[tauri::command]
pub fn get_income_expense_trend(filter: Option<ReportFilter>, state: State<'_, AppState>) -> CmdResult<Vec<TrendPoint>> {
    let filter = filter.unwrap_or_default();
    let daily = filter.daily();
    let bucket = if daily { "substr(t.txn_date, 1, 10)" } else { "substr(t.txn_date, 1, 7)" };
    let (cond, args) = filter.conditions();
    let sql = format!(
        "SELECT {bucket} AS period,
                COALESCE(SUM(CASE WHEN t.type = 'income' THEN t.base_amount ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN t.type = 'expense' THEN t.base_amount ELSE 0 END), 0)
         FROM transactions t
         WHERE t.is_confirmed = 1 AND t.type IN ('income', 'expense'){cond}
         GROUP BY period ORDER BY period ASC"
    );
    let conn = state.conn()?;
    conn.prepare(&sql)
        .and_then(|mut s| {
            s.query_map(params_from_iter(args.iter().map(|a| a.as_ref())), |r| {
                let period: String = r.get(0)?;
                let income = round2(r.get(1)?);
                let expense = round2(r.get(2)?);
                Ok(TrendPoint {
                    period_label: period_label(&period, daily),
                    period,
                    income,
                    expense,
                    net: round2(income - expense),
                    total_income: income,
                    total_expense: expense,
                    net_cashflow: round2(income - expense),
                })
            })?
            .collect()
        })
        .ctx("Failed to load income/expense trend")
}
