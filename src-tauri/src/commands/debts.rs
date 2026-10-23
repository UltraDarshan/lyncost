//! Loans and credit lines: drawdowns, repayments and direct spends.
//!
//! Money that lands in or leaves one of the user's accounts is recorded as a
//! regular transaction (so balances, reports and the ledger stay correct);
//! the debt's own history lives in `debt_transactions`.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::transactions::{insert_txn, NewTxn};
use crate::db::AppState;
use crate::util::{opt_bool, opt_f64, opt_i64, opt_string, round2, today_str, CmdResult, Ctx};

#[derive(Serialize)]
pub struct Debt {
    id: i64,
    name: String,
    principal: f64,
    current_balance: f64,
    total_borrowed: f64,
    total_paid: f64,
    debt_type: String,
    interest_rate: f64,
    due_date: Option<String>,
    min_payment: f64,
    account_id: Option<i64>,
    notes: Option<String>,
    is_active: i64,
}

const DEBT_COLUMNS: &str = "id, name, principal, current_balance, COALESCE(total_borrowed, 0), COALESCE(total_paid, 0),
    COALESCE(debt_type, 'personal_loan'), COALESCE(interest_rate, 0), due_date, COALESCE(min_payment, 0),
    account_id, notes, COALESCE(is_active, 1)";

fn map_debt(r: &Row) -> rusqlite::Result<Debt> {
    Ok(Debt {
        id: r.get(0)?,
        name: r.get(1)?,
        principal: r.get(2)?,
        current_balance: r.get(3)?,
        total_borrowed: r.get(4)?,
        total_paid: r.get(5)?,
        debt_type: r.get(6)?,
        interest_rate: r.get(7)?,
        due_date: r.get(8)?,
        min_payment: r.get(9)?,
        account_id: r.get(10)?,
        notes: r.get(11)?,
        is_active: r.get(12)?,
    })
}

fn load_debt(conn: &Connection, id: i64) -> CmdResult<Debt> {
    conn.query_row(&format!("SELECT {DEBT_COLUMNS} FROM debts WHERE id = ?1"), params![id], map_debt)
        .optional()
        .ctx("Failed to load debt")?
        .ok_or_else(|| "Debt not found".to_string())
}

#[tauri::command]
pub fn get_debts(state: State<'_, AppState>) -> CmdResult<Vec<Debt>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!("SELECT {DEBT_COLUMNS} FROM debts ORDER BY is_active DESC, id DESC"))
        .ctx("Failed to load debts")?;
    let rows = stmt.query_map([], map_debt).ctx("Failed to load debts")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load debts")
}

#[derive(Deserialize)]
pub struct DebtPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    principal: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    initial_draw: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    current_balance: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    total_borrowed: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    total_paid: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    debt_type: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    interest_rate: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    due_date: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    min_payment: Option<f64>,
    #[serde(default, deserialize_with = "opt_i64")]
    deposit_account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    notes: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_active: Option<bool>,
}

#[tauri::command]
pub fn create_debt(payload: DebtPayload, state: State<'_, AppState>) -> CmdResult<Debt> {
    let name = payload.name.clone().ok_or("Debt name is required")?;
    let principal = round2(payload.principal.unwrap_or(0.0).max(0.0));
    if principal <= 0.0 {
        return Err("Principal amount must be greater than zero".into());
    }
    let drawn = round2(payload.initial_draw.unwrap_or(principal).clamp(0.0, principal));
    let account_id = payload.deposit_account_id.filter(|_| drawn > 0.0);
    let today = today_str();

    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to create debt")?;
    tx.execute(
        "INSERT INTO debts (name, principal, current_balance, total_borrowed, total_paid,
                            debt_type, interest_rate, due_date, min_payment, account_id, notes, is_active)
         VALUES (?1, ?2, ?3, ?3, 0, ?4, ?5, ?6, ?7, ?8, ?9, 1)",
        params![
            name,
            principal,
            drawn,
            payload.debt_type.as_deref().unwrap_or("personal_loan"),
            payload.interest_rate.unwrap_or(0.0),
            payload.due_date,
            payload.min_payment.unwrap_or(0.0),
            account_id,
            payload.notes
        ],
    )
    .ctx("Failed to create debt")?;
    let id = tx.last_insert_rowid();
    if drawn > 0.0 {
        tx.execute(
            "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date, account_id, notes)
             VALUES (?1, 'draw', ?2, ?3, ?4, 'Initial loan disbursement / drawdown')",
            params![id, drawn, today, account_id],
        )
        .ctx("Failed to record drawdown")?;
        if let Some(acc) = account_id {
            let note = format!("Loan Disbursal: {name}");
            insert_txn(&tx, &NewTxn::simple(acc, "income", drawn, &today, Some(&note)))?;
        }
    }
    tx.commit().ctx("Failed to create debt")?;
    load_debt(&conn, id)
}

#[tauri::command]
pub fn update_debt(id: i64, payload: DebtPayload, state: State<'_, AppState>) -> CmdResult<Debt> {
    let name = payload.name.clone().ok_or("Debt name is required")?;
    let conn = state.conn()?;
    let existing = load_debt(&conn, id)?;
    let balance = round2(payload.current_balance.unwrap_or(existing.current_balance).max(0.0));
    conn.execute(
        "UPDATE debts
         SET name = ?1, principal = ?2, current_balance = ?3, total_borrowed = ?4, total_paid = ?5,
             debt_type = ?6, interest_rate = ?7, due_date = ?8, min_payment = ?9, account_id = ?10,
             notes = ?11, is_active = ?12
         WHERE id = ?13",
        params![
            name,
            round2(payload.principal.unwrap_or(existing.principal)),
            balance,
            round2(payload.total_borrowed.unwrap_or(existing.total_borrowed)),
            round2(payload.total_paid.unwrap_or(existing.total_paid)),
            payload.debt_type.as_deref().unwrap_or(&existing.debt_type),
            payload.interest_rate.unwrap_or(existing.interest_rate),
            payload.due_date,
            payload.min_payment.unwrap_or(existing.min_payment),
            payload.account_id,
            payload.notes,
            payload.is_active.unwrap_or(existing.is_active != 0),
            id
        ],
    )
    .ctx("Failed to update debt")?;
    load_debt(&conn, id)
}

#[tauri::command]
pub fn delete_debt(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to delete debt")?;
    tx.execute("DELETE FROM debt_transactions WHERE debt_id = ?1", params![id]).ctx("Failed to delete debt")?;
    tx.execute("DELETE FROM debts WHERE id = ?1", params![id]).ctx("Failed to delete debt")?;
    tx.commit().ctx("Failed to delete debt")
}

#[derive(Deserialize)]
pub struct DebtMovement {
    #[serde(default, deserialize_with = "opt_i64")]
    debt_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    txn_date: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    notes: Option<String>,
}

impl DebtMovement {
    fn parts(&self, zero_msg: &str) -> CmdResult<(i64, f64, String)> {
        let debt_id = self.debt_id.ok_or("Debt not found")?;
        let amount = round2(self.amount.unwrap_or(0.0));
        if amount <= 0.0 {
            return Err(zero_msg.into());
        }
        Ok((debt_id, amount, self.txn_date.clone().unwrap_or_else(today_str)))
    }
}

/// Borrow more against a loan/credit line, optionally depositing it into an account.
#[tauri::command]
pub fn draw_debt_funds(payload: DebtMovement, state: State<'_, AppState>) -> CmdResult<Debt> {
    let (debt_id, amount, date) = payload.parts("Draw amount must be greater than zero")?;
    let mut conn = state.conn()?;
    let debt = load_debt(&conn, debt_id)?;
    let tx = conn.transaction().ctx("Failed to record drawdown")?;
    tx.execute(
        "UPDATE debts SET current_balance = ?1, total_borrowed = ?2, is_active = 1 WHERE id = ?3",
        params![round2(debt.current_balance + amount), round2(debt.total_borrowed + amount), debt_id],
    )
    .ctx("Failed to record drawdown")?;
    tx.execute(
        "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date, account_id, notes)
         VALUES (?1, 'draw', ?2, ?3, ?4, ?5)",
        params![debt_id, amount, date, payload.account_id, payload.notes],
    )
    .ctx("Failed to record drawdown")?;
    if let Some(acc) = payload.account_id {
        let note = format!("Loan Drawdown: {}", debt.name);
        insert_txn(&tx, &NewTxn::simple(acc, "income", amount, &date, Some(&note)))?;
    }
    tx.commit().ctx("Failed to record drawdown")?;
    load_debt(&conn, debt_id)
}

/// Repay a debt, optionally paying from one of the user's accounts.
#[tauri::command]
pub fn record_debt_payment(payload: DebtMovement, state: State<'_, AppState>) -> CmdResult<Debt> {
    let (debt_id, amount, date) = payload.parts("Payment amount must be greater than zero")?;
    let mut conn = state.conn()?;
    let debt = load_debt(&conn, debt_id)?;
    let remaining = round2((debt.current_balance - amount).max(0.0));
    let tx = conn.transaction().ctx("Failed to record payment")?;
    tx.execute(
        "UPDATE debts SET current_balance = ?1, total_paid = ?2, is_active = ?3 WHERE id = ?4",
        params![remaining, round2(debt.total_paid + amount), remaining > 0.0, debt_id],
    )
    .ctx("Failed to record payment")?;
    tx.execute(
        "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date, account_id, notes)
         VALUES (?1, 'payment', ?2, ?3, ?4, ?5)",
        params![debt_id, amount, date, payload.account_id, payload.notes],
    )
    .ctx("Failed to record payment")?;
    if let Some(acc) = payload.account_id {
        let note = format!("Debt Repayment: {}", debt.name);
        insert_txn(&tx, &NewTxn::simple(acc, "expense", amount, &date, Some(&note)))?;
    }
    tx.commit().ctx("Failed to record payment")?;
    load_debt(&conn, debt_id)
}

/// Spend directly on a credit line (e.g. a credit-card purchase) without touching an account.
#[tauri::command]
pub fn spend_from_debt(payload: DebtMovement, state: State<'_, AppState>) -> CmdResult<Debt> {
    let (debt_id, amount, date) = payload.parts("Spend amount must be greater than zero")?;
    let mut conn = state.conn()?;
    let debt = load_debt(&conn, debt_id)?;
    let tx = conn.transaction().ctx("Failed to record spend")?;
    tx.execute(
        "UPDATE debts SET current_balance = ?1, total_borrowed = ?2, is_active = 1 WHERE id = ?3",
        params![round2(debt.current_balance + amount), round2(debt.total_borrowed + amount), debt_id],
    )
    .ctx("Failed to record spend")?;
    tx.execute(
        "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date, category_id, notes)
         VALUES (?1, 'expense', ?2, ?3, ?4, ?5)",
        params![debt_id, amount, date, payload.category_id, payload.notes],
    )
    .ctx("Failed to record spend")?;
    tx.commit().ctx("Failed to record spend")?;
    load_debt(&conn, debt_id)
}

#[derive(Serialize)]
pub struct DebtTransaction {
    id: i64,
    debt_id: i64,
    txn_type: String,
    amount: f64,
    txn_date: String,
    account_id: Option<i64>,
    account_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    notes: Option<String>,
    created_at: Option<String>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_debt_transactions(debtId: i64, state: State<'_, AppState>) -> CmdResult<Vec<DebtTransaction>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(
            "SELECT dt.id, dt.debt_id, dt.txn_type, dt.amount, dt.txn_date,
                    dt.account_id, a.name, dt.category_id, c.name, dt.notes, dt.created_at
             FROM debt_transactions dt
             LEFT JOIN accounts a ON dt.account_id = a.id
             LEFT JOIN categories c ON dt.category_id = c.id
             WHERE dt.debt_id = ?1
             ORDER BY dt.txn_date DESC, dt.id DESC",
        )
        .ctx("Failed to load debt history")?;
    let rows = stmt
        .query_map(params![debtId], |r| {
            Ok(DebtTransaction {
                id: r.get(0)?,
                debt_id: r.get(1)?,
                txn_type: r.get(2)?,
                amount: r.get(3)?,
                txn_date: r.get(4)?,
                account_id: r.get(5)?,
                account_name: r.get(6)?,
                category_id: r.get(7)?,
                category_name: r.get(8)?,
                notes: r.get(9)?,
                created_at: r.get(10)?,
            })
        })
        .ctx("Failed to load debt history")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load debt history")
}
