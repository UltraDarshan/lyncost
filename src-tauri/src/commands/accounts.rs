//! Bank, cash, investment, credit-card and other accounts plus their ledger.

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::fx;
use crate::ledger;
use crate::util::{opt_f64, opt_string, round2, CmdResult, Ctx};

const ACCOUNT_TYPES: &[&str] = &["bank", "cash", "investment", "credit_card", "other"];
const ACCOUNT_COLUMNS: &str =
    "id, name, type, currency, opening_balance, current_balance, icon, color, is_archived, created_at";

#[derive(Serialize)]
pub struct Account {
    id: i64,
    name: String,
    #[serde(rename = "type")]
    kind: String,
    currency: String,
    opening_balance: f64,
    current_balance: f64,
    icon: Option<String>,
    color: Option<String>,
    is_archived: i64,
    created_at: Option<String>,
}

fn map_account(r: &Row) -> rusqlite::Result<Account> {
    Ok(Account {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        currency: r.get(3)?,
        opening_balance: r.get(4)?,
        current_balance: r.get(5)?,
        icon: r.get(6)?,
        color: r.get(7)?,
        is_archived: r.get::<_, Option<i64>>(8)?.unwrap_or(0),
        created_at: r.get(9)?,
    })
}

fn load_account(conn: &Connection, id: i64) -> CmdResult<Account> {
    conn.query_row(&format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = ?1"), params![id], map_account)
        .map_err(|_| "Account not found".to_string())
}

#[derive(Deserialize)]
pub struct AccountPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(rename = "type", default, deserialize_with = "opt_string")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    currency: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    opening_balance: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    icon: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    color: Option<String>,
}

impl AccountPayload {
    fn validated(&self) -> CmdResult<(String, String, String)> {
        let name = self.name.clone().ok_or("Account name cannot be empty")?;
        let kind = self.kind.clone().unwrap_or_else(|| "bank".into());
        if !ACCOUNT_TYPES.contains(&kind.as_str()) {
            return Err(format!("Invalid account type: {kind}"));
        }
        let currency = self.currency.clone().unwrap_or_else(|| "INR".into()).to_uppercase();
        Ok((name, kind, currency))
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_accounts(includeArchived: Option<bool>, state: State<'_, AppState>) -> CmdResult<Vec<Account>> {
    let conn = state.conn()?;
    let sql = if includeArchived.unwrap_or(false) {
        format!("SELECT {ACCOUNT_COLUMNS} FROM accounts ORDER BY is_archived ASC, id ASC")
    } else {
        format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE is_archived = 0 ORDER BY id ASC")
    };
    let mut stmt = conn.prepare(&sql).ctx("Failed to load accounts")?;
    let rows = stmt.query_map([], map_account).ctx("Failed to load accounts")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load accounts")
}

#[tauri::command]
pub fn create_account(payload: AccountPayload, state: State<'_, AppState>) -> CmdResult<Account> {
    let (name, kind, currency) = payload.validated()?;
    let opening = round2(payload.opening_balance.unwrap_or(0.0));
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to insert account")?;
    tx.execute(
        "INSERT INTO accounts (name, type, currency, opening_balance, current_balance, icon, color, is_archived)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, 0)",
        params![name, kind, currency, opening, payload.icon, payload.color],
    )
    .ctx("Failed to insert account")?;
    let id = tx.last_insert_rowid();
    ledger::post_opening(&tx, id, opening).ctx("Failed to insert opening ledger row")?;
    tx.commit().ctx("Failed to insert account")?;
    load_account(&conn, id)
}

#[tauri::command]
pub fn update_account(id: i64, payload: AccountPayload, state: State<'_, AppState>) -> CmdResult<Account> {
    let (name, kind, currency) = payload.validated()?;
    let mut conn = state.conn()?;
    let old_currency = load_account(&conn, id)?.currency;
    let tx = conn.transaction().ctx("Failed to update account")?;
    tx.execute(
        "UPDATE accounts SET name = ?1, type = ?2, currency = ?3, icon = COALESCE(?4, icon), color = COALESCE(?5, color)
         WHERE id = ?6",
        params![name, kind, currency, payload.icon, payload.color, id],
    )
    .ctx("Failed to update account")?;
    if old_currency != currency {
        // Amounts stay as entered; only their value in the base currency changes.
        let rate = fx::rate_to_base(&tx, &currency);
        tx.execute(
            "UPDATE transactions SET exchange_rate_used = ?1, base_amount = ROUND(amount * ?1, 2) WHERE account_id = ?2",
            params![rate, id],
        )
        .ctx("Failed to update account")?;
    }
    tx.commit().ctx("Failed to update account")?;
    load_account(&conn, id)
}

#[tauri::command]
pub fn archive_account(id: i64, archive: bool, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("UPDATE accounts SET is_archived = ?1 WHERE id = ?2", params![archive, id])
        .ctx("Failed to archive account")?;
    Ok(())
}

#[tauri::command]
pub fn delete_account(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let count = |sql: &str| -> CmdResult<i64> { conn.query_row(sql, params![id], |r| r.get(0)).ctx("Failed to delete account") };
    let blockers = [
        (
            "SELECT COUNT(*) FROM transactions WHERE account_id = ?1 OR transfer_to_account_id = ?1",
            "Cannot delete account with existing transactions. Please archive it instead to preserve financial history.",
        ),
        (
            "SELECT COUNT(*) FROM bills WHERE account_id = ?1",
            "Cannot delete account associated with existing bills. Please reassign or delete those bills first.",
        ),
        (
            "SELECT COUNT(*) FROM recurring_rules WHERE account_id = ?1",
            "Cannot delete account associated with recurring rules. Please reassign or delete those rules first.",
        ),
        (
            "SELECT COUNT(*) FROM debts WHERE account_id = ?1",
            "Cannot delete account associated with debt tracking. Please settle or delete those debts first.",
        ),
        (
            "SELECT COUNT(*) FROM investment_holdings WHERE account_id = ?1",
            "Cannot delete account that still has investment holdings. Please delete or move those holdings first.",
        ),
    ];
    for (sql, message) in blockers {
        if count(sql)? > 0 {
            return Err(message.into());
        }
    }
    let tx = conn.transaction().ctx("Failed to delete account")?;
    tx.execute("DELETE FROM account_ledger WHERE account_id = ?1", params![id]).ctx("Failed to clean account ledger")?;
    tx.execute("UPDATE goals SET account_id = NULL WHERE account_id = ?1", params![id]).ctx("Failed to delete account")?;
    tx.execute("UPDATE debt_transactions SET account_id = NULL WHERE account_id = ?1", params![id])
        .ctx("Failed to delete account")?;
    tx.execute("DELETE FROM accounts WHERE id = ?1", params![id]).ctx("Failed to delete account")?;
    tx.commit().ctx("Failed to delete account")
}

#[derive(Serialize)]
pub struct LedgerEntry {
    id: i64,
    account_id: i64,
    txn_type: String,
    amount: f64,
    balance_after: f64,
    reference_type: Option<String>,
    reference_id: Option<i64>,
    txn_date: Option<String>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_account_ledger(accountId: i64, state: State<'_, AppState>) -> CmdResult<Vec<LedgerEntry>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT id, account_id, txn_type, amount, balance_after, reference_type, reference_id, txn_date
             FROM account_ledger WHERE account_id = ?1 ORDER BY id DESC",
        )
        .ctx("Failed to load ledger")?;
    let rows = stmt
        .query_map(params![accountId], |r| {
            Ok(LedgerEntry {
                id: r.get(0)?,
                account_id: r.get(1)?,
                txn_type: r.get(2)?,
                amount: r.get(3)?,
                balance_after: r.get(4)?,
                reference_type: r.get(5)?,
                reference_id: r.get(6)?,
                txn_date: r.get(7)?,
            })
        })
        .ctx("Failed to load ledger")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load ledger")
}
