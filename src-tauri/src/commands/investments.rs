//! Investment holdings, manual price updates, portfolio summary and report.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::fx;
use crate::util::{now_str, opt_f64, opt_i64, opt_string, parse_date, round2, today, CmdResult, Ctx};

const ASSET_TYPES: &[&str] = &["stock", "crypto", "mutual_fund", "other"];

#[derive(Serialize)]
pub struct Holding {
    id: i64,
    account_id: i64,
    account_name: String,
    asset_type: String,
    symbol: String,
    name: Option<String>,
    quantity: f64,
    avg_buy_price: f64,
    currency: String,
    last_price: f64,
    last_price_updated_at: Option<String>,
    notes: Option<String>,
    is_archived: i64,
    invested_value: f64,
    current_value: f64,
    unrealized_pnl: f64,
    unrealized_pnl_percent: f64,
    exchange_rate_used: f64,
    base_invested_value: f64,
    base_current_value: f64,
    base_unrealized_pnl: f64,
    days_since_update: Option<i64>,
}

fn pct(gain: f64, cost: f64) -> f64 {
    if cost > 0.0 { round2(gain / cost * 100.0) } else { 0.0 }
}

fn load_holdings(conn: &Connection, only_active: bool) -> CmdResult<Vec<Holding>> {
    let mut stmt = conn
        .prepare(
            "SELECT h.id, h.account_id, a.name, h.asset_type, h.symbol, h.name,
                    h.quantity, h.avg_buy_price, h.currency, h.last_price,
                    h.last_price_updated_at, h.notes, COALESCE(h.is_archived, 0)
             FROM investment_holdings h
             JOIN accounts a ON a.id = h.account_id
             WHERE ?1 = 0 OR h.is_archived = 0
             ORDER BY h.is_archived ASC, h.asset_type ASC, h.symbol ASC",
        )
        .ctx("Failed to load holdings")?;
    let rows = stmt
        .query_map(params![only_active], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, f64>(7)?,
                r.get::<_, String>(8)?,
                r.get::<_, f64>(9)?,
                r.get::<_, Option<String>>(10)?,
                r.get::<_, Option<String>>(11)?,
                r.get::<_, i64>(12)?,
            ))
        })
        .ctx("Failed to load holdings")?
        .collect::<Result<Vec<_>, _>>()
        .ctx("Failed to load holdings")?;

    let today = today();
    Ok(rows
        .into_iter()
        .map(|(id, account_id, account_name, asset_type, symbol, name, qty, avg, currency, last, updated, notes, archived)| {
            let invested = qty * avg;
            let current = qty * last;
            let rate = fx::rate_to_base(conn, &currency);
            Holding {
                id,
                account_id,
                account_name,
                asset_type,
                symbol,
                name,
                quantity: qty,
                avg_buy_price: avg,
                currency,
                last_price: last,
                days_since_update: updated.as_deref().and_then(parse_date).map(|d| (today - d).num_days().max(0)),
                last_price_updated_at: updated,
                notes,
                is_archived: archived,
                invested_value: round2(invested),
                current_value: round2(current),
                unrealized_pnl: round2(current - invested),
                unrealized_pnl_percent: pct(current - invested, invested),
                exchange_rate_used: rate,
                base_invested_value: round2(invested * rate),
                base_current_value: round2(current * rate),
                base_unrealized_pnl: round2((current - invested) * rate),
            }
        })
        .collect())
}

#[tauri::command]
pub fn get_holdings(state: State<'_, AppState>) -> CmdResult<Vec<Holding>> {
    load_holdings(&*state.conn()?, false)
}

#[derive(Deserialize)]
pub struct HoldingPayload {
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    asset_type: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    symbol: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    quantity: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    avg_buy_price: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    currency: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    last_price: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    notes: Option<String>,
}

struct ValidHolding {
    asset_type: String,
    symbol: String,
    quantity: f64,
    avg_buy_price: f64,
    currency: String,
}

fn validate(p: &HoldingPayload) -> CmdResult<ValidHolding> {
    let symbol = p.symbol.clone().ok_or("Symbol is required")?.to_uppercase();
    let asset_type = p.asset_type.clone().unwrap_or_else(|| "stock".into());
    if !ASSET_TYPES.contains(&asset_type.as_str()) {
        return Err(format!("Invalid asset type: {asset_type}"));
    }
    let quantity = p.quantity.unwrap_or(0.0);
    if quantity <= 0.0 {
        return Err("Quantity must be greater than zero".into());
    }
    let avg_buy_price = p.avg_buy_price.unwrap_or(0.0);
    if avg_buy_price < 0.0 {
        return Err("Average buy price cannot be negative".into());
    }
    let currency = p.currency.clone().unwrap_or_else(|| "INR".into()).to_uppercase();
    Ok(ValidHolding { asset_type, symbol, quantity, avg_buy_price, currency })
}

#[tauri::command]
pub fn create_holding(payload: HoldingPayload, state: State<'_, AppState>) -> CmdResult<i64> {
    let v = validate(&payload)?;
    let account_id = payload.account_id.ok_or("Holdings must be linked to an investment-type account")?;
    let last_price = payload.last_price.unwrap_or(v.avg_buy_price);
    if last_price < 0.0 {
        return Err("Price cannot be negative".into());
    }
    let mut conn = state.conn()?;
    let account_type: Option<String> = conn
        .query_row("SELECT type FROM accounts WHERE id = ?1", params![account_id], |r| r.get(0))
        .optional()
        .ctx("Failed to look up account")?;
    if account_type.as_deref() != Some("investment") {
        return Err("Holdings must be linked to an investment-type account".into());
    }
    let now = now_str();
    let tx = conn.transaction().ctx("Failed to create holding")?;
    tx.execute(
        "INSERT INTO investment_holdings (
            account_id, asset_type, symbol, name, quantity, avg_buy_price,
            currency, last_price, last_price_updated_at, notes, is_archived
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
        params![
            account_id, v.asset_type, v.symbol, payload.name, v.quantity, v.avg_buy_price,
            v.currency, last_price, now, payload.notes
        ],
    )
    .ctx("Failed to create holding")?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO investment_price_history (holding_id, price, recorded_at) VALUES (?1, ?2, ?3)",
        params![id, last_price, now],
    )
    .ctx("Failed to create holding")?;
    tx.commit().ctx("Failed to create holding")?;
    Ok(id)
}

#[tauri::command]
pub fn update_holding(id: i64, payload: HoldingPayload, state: State<'_, AppState>) -> CmdResult<()> {
    let v = validate(&payload)?;
    let conn = state.conn()?;
    let changed = conn
        .execute(
            "UPDATE investment_holdings
             SET asset_type = ?1, symbol = ?2, name = ?3, quantity = ?4,
                 avg_buy_price = ?5, currency = ?6, notes = ?7
             WHERE id = ?8",
            params![v.asset_type, v.symbol, payload.name, v.quantity, v.avg_buy_price, v.currency, payload.notes, id],
        )
        .ctx("Failed to update holding")?;
    if changed == 0 {
        return Err("Holding not found".into());
    }
    Ok(())
}

#[tauri::command]
pub fn delete_holding(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.conn()?;
    conn.execute("DELETE FROM investment_price_history WHERE holding_id = ?1", params![id])
        .ctx("Failed to delete holding")?;
    conn.execute("DELETE FROM investment_holdings WHERE id = ?1", params![id]).ctx("Failed to delete holding")?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn archive_holding(id: i64, isArchived: bool, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("UPDATE investment_holdings SET is_archived = ?1 WHERE id = ?2", params![isArchived, id])
        .ctx("Failed to archive holding")?;
    Ok(())
}

#[derive(Deserialize)]
pub struct PriceUpdate {
    #[serde(default, deserialize_with = "opt_i64")]
    holding_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_f64")]
    price: Option<f64>,
}

fn apply_price(conn: &Connection, update: &PriceUpdate, now: &str) -> CmdResult<()> {
    let id = update.holding_id.ok_or("Holding not found")?;
    let price = update.price.ok_or("Price is required")?;
    if price < 0.0 {
        return Err("Price cannot be negative".into());
    }
    let changed = conn
        .execute(
            "UPDATE investment_holdings SET last_price = ?1, last_price_updated_at = ?2 WHERE id = ?3",
            params![price, now, id],
        )
        .ctx("Failed to update price")?;
    if changed == 0 {
        return Err("Holding not found".into());
    }
    conn.execute(
        "INSERT INTO investment_price_history (holding_id, price, recorded_at) VALUES (?1, ?2, ?3)",
        params![id, price, now],
    )
    .ctx("Failed to record price history")?;
    Ok(())
}

#[tauri::command]
pub fn update_holding_price(payload: PriceUpdate, state: State<'_, AppState>) -> CmdResult<()> {
    apply_price(&*state.conn()?, &payload, &now_str())
}

#[tauri::command]
pub fn bulk_update_holding_prices(updates: Vec<PriceUpdate>, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let now = now_str();
    let tx = conn.transaction().ctx("Failed to update prices")?;
    for update in &updates {
        apply_price(&tx, update, &now)?;
    }
    tx.commit().ctx("Failed to update prices")
}

#[derive(Serialize)]
pub struct PricePoint {
    id: i64,
    holding_id: i64,
    price: f64,
    recorded_at: String,
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_holding_price_history(holdingId: i64, state: State<'_, AppState>) -> CmdResult<Vec<PricePoint>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, holding_id, price, recorded_at FROM investment_price_history
             WHERE holding_id = ?1 ORDER BY recorded_at ASC, id ASC",
        )
        .ctx("Failed to load price history")?;
    let rows = stmt
        .query_map(params![holdingId], |r| {
            Ok(PricePoint { id: r.get(0)?, holding_id: r.get(1)?, price: r.get(2)?, recorded_at: r.get(3)? })
        })
        .ctx("Failed to load price history")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load price history")
}

#[derive(Serialize)]
pub struct PortfolioSummary {
    total_invested_base: f64,
    total_current_value_base: f64,
    total_unrealized_pnl_base: f64,
    total_unrealized_pnl_percent: f64,
    holdings_count: usize,
}

#[tauri::command]
pub fn get_portfolio_summary(state: State<'_, AppState>) -> CmdResult<PortfolioSummary> {
    let holdings = load_holdings(&*state.conn()?, true)?;
    let invested: f64 = holdings.iter().map(|h| h.base_invested_value).sum();
    let current: f64 = holdings.iter().map(|h| h.base_current_value).sum();
    Ok(PortfolioSummary {
        total_invested_base: round2(invested),
        total_current_value_base: round2(current),
        total_unrealized_pnl_base: round2(current - invested),
        total_unrealized_pnl_percent: pct(current - invested, invested),
        holdings_count: holdings.len(),
    })
}

#[derive(Serialize)]
pub struct PerformanceRow {
    id: i64,
    symbol: String,
    name: Option<String>,
    asset_type: String,
    account_name: String,
    quantity: f64,
    avg_buy_price: f64,
    last_price: f64,
    currency: String,
    cost_basis_base: f64,
    current_value_base: f64,
    unrealized_pnl_base: f64,
    pnl_percent: f64,
}

#[derive(Serialize)]
pub struct PerformanceReport {
    total_cost_basis: f64,
    total_current_value: f64,
    total_unrealized_pnl: f64,
    total_pnl_percent: f64,
    holdings: Vec<PerformanceRow>,
}

#[tauri::command]
pub fn get_investment_performance_report(state: State<'_, AppState>) -> CmdResult<PerformanceReport> {
    let holdings = load_holdings(&*state.conn()?, true)?;
    let cost: f64 = holdings.iter().map(|h| h.base_invested_value).sum();
    let value: f64 = holdings.iter().map(|h| h.base_current_value).sum();
    Ok(PerformanceReport {
        total_cost_basis: round2(cost),
        total_current_value: round2(value),
        total_unrealized_pnl: round2(value - cost),
        total_pnl_percent: pct(value - cost, cost),
        holdings: holdings
            .into_iter()
            .map(|h| PerformanceRow {
                pnl_percent: h.unrealized_pnl_percent,
                id: h.id,
                symbol: h.symbol,
                name: h.name,
                asset_type: h.asset_type,
                account_name: h.account_name,
                quantity: h.quantity,
                avg_buy_price: h.avg_buy_price,
                last_price: h.last_price,
                currency: h.currency,
                cost_basis_base: h.base_invested_value,
                current_value_base: h.base_current_value,
                unrealized_pnl_base: h.base_unrealized_pnl,
            })
            .collect(),
    })
}
