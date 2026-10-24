//! Startup, app settings, PIN lock and full data wipe.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{seed_defaults, AppState};
use crate::fx;
use crate::security::{self, PinCheck};
use crate::util::{opt_bool, opt_i64, CmdResult, Ctx};

#[derive(Serialize)]
pub struct AppSettings {
    id: i64,
    base_currency: String,
    has_pin: bool,
    theme: String,
    last_backup_at: Option<String>,
    last_networth_snapshot_at: Option<String>,
    notify_os: bool,
    notify_advance_days: i64,
    last_notification_check_at: Option<String>,
}

pub fn load_settings(conn: &Connection) -> CmdResult<AppSettings> {
    conn.query_row(
        "SELECT id, base_currency, pin_hash IS NOT NULL, COALESCE(theme, 'dark'), last_backup_at,
                last_networth_snapshot_at, COALESCE(notify_os, 1), COALESCE(notify_advance_days, 1),
                last_notification_check_at
         FROM app_settings WHERE id = 1",
        [],
        |r| {
            Ok(AppSettings {
                id: r.get(0)?,
                base_currency: r.get(1)?,
                has_pin: r.get(2)?,
                theme: r.get(3)?,
                last_backup_at: r.get(4)?,
                last_networth_snapshot_at: r.get(5)?,
                notify_os: r.get::<_, i64>(6)? != 0,
                notify_advance_days: r.get(7)?,
                last_notification_check_at: r.get(8)?,
            })
        },
    )
    .ctx("Failed to load settings")
}

/// `Some(message)` when the database could not be opened at launch.
#[tauri::command]
pub fn get_startup_status(state: State<'_, AppState>) -> Option<String> {
    state.startup_error.clone()
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> CmdResult<AppSettings> {
    load_settings(&*state.conn()?)
}

#[tauri::command]
pub fn set_app_theme(theme: String, state: State<'_, AppState>) -> CmdResult<()> {
    if theme != "dark" && theme != "light" {
        return Err("Theme must be 'dark' or 'light'".into());
    }
    state
        .conn()?
        .execute(
            "INSERT INTO app_settings (id, theme) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET theme = excluded.theme",
            params![theme],
        )
        .ctx("Failed to save theme")?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_base_currency(newCurrency: String, state: State<'_, AppState>) -> CmdResult<()> {
    let code = newCurrency.trim().to_uppercase();
    if !(3..=4).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err("Currency code must be 3-4 letters (e.g. INR, USD, EUR)".into());
    }
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to start update")?;
    tx.execute(
        "INSERT INTO app_settings (id, base_currency) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET base_currency = excluded.base_currency",
        params![code],
    )
    .ctx("Failed to save base currency")?;
    restate_base_amounts(&tx).ctx("Failed to convert transactions to the new base currency")?;
    tx.commit().ctx("Failed to save base currency")
}

/// Reports sum `base_amount`, so it must follow the base currency.
fn restate_base_amounts(conn: &Connection) -> rusqlite::Result<()> {
    let currencies: Vec<String> = conn
        .prepare("SELECT DISTINCT a.currency FROM transactions t JOIN accounts a ON a.id = t.account_id")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for currency in currencies {
        let rate = fx::rate_to_base(conn, &currency);
        conn.execute(
            "UPDATE transactions SET exchange_rate_used = ?1, base_amount = ROUND(amount * ?1, 2)
             WHERE account_id IN (SELECT id FROM accounts WHERE currency = ?2)",
            params![rate, currency],
        )?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_initial_pin(pin: String, state: State<'_, AppState>) -> CmdResult<()> {
    if !security::is_valid_pin(&pin) {
        return Err("PIN must be exactly 6 digits (0-9)".into());
    }
    let conn = state.conn()?;
    let existing: Option<String> = conn
        .query_row("SELECT pin_hash FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .ctx("Failed to read PIN settings")?;
    if existing.is_some() {
        return Err("A PIN is already configured. Use PIN verification or reset.".into());
    }
    security::store_pin(&conn, &pin)
}

#[tauri::command]
pub fn verify_pin(pin: String, state: State<'_, AppState>) -> CmdResult<PinCheck> {
    security::check_pin(&*state.conn()?, &pin)
}

#[derive(Deserialize)]
pub struct ChangePinPayload {
    current_pin: String,
    new_pin: String,
}

#[tauri::command]
pub fn change_pin(payload: ChangePinPayload, state: State<'_, AppState>) -> CmdResult<()> {
    if !security::is_valid_pin(&payload.new_pin) {
        return Err("New PIN must be exactly 6 digits".into());
    }
    let conn = state.conn()?;
    let check = security::check_pin(&conn, &payload.current_pin)?;
    if !check.valid {
        return Err(if check.lockout_seconds > 0 {
            format!("Too many incorrect attempts. Try again in {} seconds.", check.lockout_seconds)
        } else {
            "Current PIN is incorrect".into()
        });
    }
    security::store_pin(&conn, &payload.new_pin)
}

#[derive(Deserialize)]
pub struct NotificationSettingsPayload {
    #[serde(default, deserialize_with = "opt_bool")]
    notify_os: Option<bool>,
    #[serde(default, deserialize_with = "opt_i64")]
    notify_advance_days: Option<i64>,
}

#[tauri::command]
pub fn update_notification_settings(
    payload: NotificationSettingsPayload,
    state: State<'_, AppState>,
) -> CmdResult<AppSettings> {
    let conn = state.conn()?;
    let days = payload.notify_advance_days.unwrap_or(1).clamp(0, 30);
    conn.execute(
        "INSERT INTO app_settings (id, notify_os, notify_advance_days) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
            notify_os = excluded.notify_os,
            notify_advance_days = excluded.notify_advance_days",
        params![payload.notify_os.unwrap_or(true), days],
    )
    .ctx("Failed to update notification settings")?;
    load_settings(&conn)
}

/// Erase every record, including the PIN, then restore first-run defaults.
#[tauri::command]
pub fn wipe_all_data(state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    super::backup::write_pre_wipe_backup(&state, &conn)
        .map_err(|e| format!("Could not save a safety backup, nothing was erased: {e}"))?;
    let tx = conn.transaction().ctx("Failed to wipe database")?;
    // Wiping data must not restart the Windows trial.
    let trial_started_at: Option<String> = tx
        .query_row("SELECT trial_started_at FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .unwrap_or(None);
    tx.execute_batch(
        "DELETE FROM transaction_tags;
         DELETE FROM warranties;
         DELETE FROM transactions;
         DELETE FROM account_ledger;
         DELETE FROM budget_categories;
         DELETE FROM budgets;
         DELETE FROM bills;
         DELETE FROM investment_price_history;
         DELETE FROM investment_holdings;
         DELETE FROM exchange_rates;
         DELETE FROM debt_transactions;
         DELETE FROM debts;
         DELETE FROM networth_history;
         DELETE FROM shopping_list_items;
         DELETE FROM recurring_rules;
         DELETE FROM goal_contributions;
         DELETE FROM goals;
         DELETE FROM payment_methods;
         DELETE FROM tags;
         DELETE FROM categories;
         DELETE FROM accounts;
         DELETE FROM app_settings;
         DELETE FROM sqlite_sequence;",
    )
    .ctx("Failed to wipe database")?;
    seed_defaults(&tx)?;
    tx.execute("UPDATE app_settings SET trial_started_at = ?1 WHERE id = 1", params![trial_started_at])
        .ctx("Failed to wipe database")?;
    tx.commit().ctx("Failed to wipe database")
}
