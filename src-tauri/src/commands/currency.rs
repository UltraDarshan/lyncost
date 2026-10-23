//! Exchange rates used to convert accounts and holdings into the base currency.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{now_str, opt_f64, opt_string, CmdResult, Ctx};

#[derive(Serialize)]
pub struct ExchangeRate {
    id: i64,
    from_currency: String,
    to_currency: String,
    rate: f64,
    updated_at: Option<String>,
}

#[tauri::command]
pub fn get_exchange_rates(state: State<'_, AppState>) -> CmdResult<Vec<ExchangeRate>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, from_currency, to_currency, rate, updated_at
             FROM exchange_rates ORDER BY from_currency ASC, to_currency ASC",
        )
        .ctx("Failed to load exchange rates")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ExchangeRate {
                id: r.get(0)?,
                from_currency: r.get(1)?,
                to_currency: r.get(2)?,
                rate: r.get(3)?,
                updated_at: r.get(4)?,
            })
        })
        .ctx("Failed to load exchange rates")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load exchange rates")
}

#[derive(Deserialize)]
pub struct RatePayload {
    #[serde(default, deserialize_with = "opt_string")]
    from_currency: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    to_currency: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    rate: Option<f64>,
}

#[tauri::command]
pub fn set_exchange_rate(payload: RatePayload, state: State<'_, AppState>) -> CmdResult<()> {
    let from = payload.from_currency.map(|c| c.to_uppercase()).ok_or("Currencies cannot be empty")?;
    let to = payload.to_currency.map(|c| c.to_uppercase()).ok_or("Currencies cannot be empty")?;
    if from == to {
        return Err("Currencies must be different".into());
    }
    let rate = payload.rate.filter(|r| *r > 0.0).ok_or("Exchange rate must be greater than zero")?;
    let conn = state.conn()?;
    // Keep one row per pair: a newer A→B rate replaces an older B→A entry.
    conn.execute("DELETE FROM exchange_rates WHERE from_currency = ?1 AND to_currency = ?2", params![to, from])
        .ctx("Failed to save exchange rate")?;
    conn.execute(
        "INSERT INTO exchange_rates (from_currency, to_currency, rate, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(from_currency, to_currency) DO UPDATE SET rate = ?3, updated_at = ?4",
        params![from, to, rate, now_str()],
    )
    .ctx("Failed to save exchange rate")?;
    Ok(())
}

#[tauri::command]
pub fn delete_exchange_rate(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("DELETE FROM exchange_rates WHERE id = ?1", params![id])
        .ctx("Failed to delete exchange rate")?;
    Ok(())
}
