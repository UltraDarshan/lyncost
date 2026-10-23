//! Bills: due dates, paying (creates an expense) and recurring occurrences.

use chrono::Datelike;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::transactions::{delete_txn, insert_txn, NewTxn};
use crate::db::AppState;
use crate::util::{
    add_months, fmt_date, opt_bool, opt_f64, opt_i64, opt_string, parse_date, round2, today, today_str, CmdResult, Ctx,
};

#[derive(Serialize)]
pub struct Bill {
    id: i64,
    name: String,
    amount: f64,
    due_date: String,
    account_id: Option<i64>,
    account_name: Option<String>,
    category_id: Option<i64>,
    category_name: Option<String>,
    is_paid: i64,
    recurrence: String,
    is_overdue: bool,
}

const SELECT_BILL: &str = "SELECT b.id, b.name, b.amount, b.due_date, b.account_id, a.name, b.category_id, c.name,
        COALESCE(b.is_paid, 0), COALESCE(b.recurrence, 'none')
     FROM bills b
     LEFT JOIN accounts a ON a.id = b.account_id
     LEFT JOIN categories c ON c.id = b.category_id";

fn map_bill(r: &Row) -> rusqlite::Result<Bill> {
    let due_date: String = r.get(3)?;
    let is_paid: i64 = r.get(8)?;
    Ok(Bill {
        id: r.get(0)?,
        name: r.get(1)?,
        amount: r.get(2)?,
        is_overdue: is_paid == 0 && parse_date(&due_date).is_some_and(|d| d < today()),
        due_date,
        account_id: r.get(4)?,
        account_name: r.get(5)?,
        category_id: r.get(6)?,
        category_name: r.get(7)?,
        is_paid,
        recurrence: r.get(9)?,
    })
}

fn load_bill(conn: &Connection, id: i64) -> CmdResult<Bill> {
    conn.query_row(&format!("{SELECT_BILL} WHERE b.id = ?1"), params![id], map_bill)
        .optional()
        .ctx("Failed to load bill")?
        .ok_or_else(|| "Bill not found".to_string())
}

#[tauri::command]
pub fn get_bills(state: State<'_, AppState>) -> CmdResult<Vec<Bill>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!("{SELECT_BILL} ORDER BY b.is_paid ASC, b.due_date ASC"))
        .ctx("Failed to load bills")?;
    let rows = stmt.query_map([], map_bill).ctx("Failed to load bills")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load bills")
}

#[derive(Deserialize)]
pub struct BillPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    due_date: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_paid: Option<bool>,
    #[serde(default, deserialize_with = "opt_string")]
    recurrence: Option<String>,
}

impl BillPayload {
    fn validated(&self) -> CmdResult<(String, f64, String, String)> {
        let name = self.name.clone().ok_or("Bill name cannot be empty")?;
        let amount = round2(self.amount.unwrap_or(0.0));
        if amount <= 0.0 {
            return Err("Bill amount must be greater than zero".into());
        }
        let due = self.due_date.clone().filter(|d| parse_date(d).is_some()).ok_or("Please choose a valid due date")?;
        let recurrence = self.recurrence.clone().unwrap_or_else(|| "none".into());
        if !matches!(recurrence.as_str(), "none" | "monthly" | "yearly") {
            return Err(format!("Invalid recurrence: {recurrence}"));
        }
        Ok((name, amount, due, recurrence))
    }
}

#[tauri::command]
pub fn create_bill(payload: BillPayload, state: State<'_, AppState>) -> CmdResult<Bill> {
    let (name, amount, due, recurrence) = payload.validated()?;
    let conn = state.conn()?;
    conn.execute(
        "INSERT INTO bills (name, amount, due_date, account_id, category_id, is_paid, recurrence)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![name, amount, due, payload.account_id, payload.category_id, recurrence],
    )
    .ctx("Failed to create bill")?;
    load_bill(&conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn update_bill(id: i64, payload: BillPayload, state: State<'_, AppState>) -> CmdResult<Bill> {
    let (name, amount, due, recurrence) = payload.validated()?;
    let conn = state.conn()?;
    let current = load_bill(&conn, id)?;
    conn.execute(
        "UPDATE bills SET name = ?1, amount = ?2, due_date = ?3, account_id = ?4, category_id = ?5,
             is_paid = ?6, recurrence = ?7
         WHERE id = ?8",
        params![
            name,
            amount,
            due,
            payload.account_id,
            payload.category_id,
            payload.is_paid.unwrap_or(current.is_paid != 0),
            recurrence,
            id
        ],
    )
    .ctx("Failed to update bill")?;
    load_bill(&conn, id)
}

/// Deleting a bill keeps any payment already recorded: that money really moved.
#[tauri::command]
pub fn delete_bill(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.conn()?;
    conn.execute("UPDATE bills SET next_bill_id = NULL WHERE next_bill_id = ?1", params![id])
        .ctx("Failed to delete bill")?;
    conn.execute("DELETE FROM bills WHERE id = ?1", params![id]).ctx("Failed to delete bill")?;
    Ok(())
}

fn next_due(due: &str, recurrence: &str) -> Option<String> {
    let d = parse_date(due)?;
    let months = match recurrence {
        "monthly" => 1,
        "yearly" => 12,
        _ => return None,
    };
    Some(fmt_date(add_months(d, months, Some(d.day()))))
}

/// Pay a bill: records an expense from its account and schedules the next occurrence.
#[tauri::command]
pub fn mark_bill_paid(id: i64, state: State<'_, AppState>) -> CmdResult<Bill> {
    let mut conn = state.conn()?;
    let bill = load_bill(&conn, id)?;
    if bill.is_paid != 0 {
        return Ok(bill);
    }
    let account_id = match bill.account_id {
        Some(acc) => acc,
        None => conn
            .query_row(
                "SELECT id FROM accounts WHERE is_archived = 0 AND type IN ('bank', 'cash') ORDER BY id ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
            .ctx("Failed to find an account")?
            .ok_or("No active bank or cash account available to pay this bill")?,
    };

    let tx = conn.transaction().ctx("Failed to update bill status")?;
    let note = format!("Bill Payment: {}", bill.name);
    let today = today_str();
    let txn_id = insert_txn(
        &tx,
        &NewTxn {
            account_id,
            kind: "expense",
            category_id: bill.category_id,
            transfer_to_account_id: None,
            amount: bill.amount,
            payment_type: Some("bank_transfer"),
            txn_date: &today,
            note: Some(&note),
            is_confirmed: true,
            recurring_rule_id: None,
        },
    )
    .map_err(|e| format!("Failed to create transaction for bill: {e}"))?;

    let next_bill_id = match next_due(&bill.due_date, &bill.recurrence) {
        Some(next) => {
            tx.execute(
                "INSERT INTO bills (name, amount, due_date, account_id, category_id, is_paid, recurrence)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
                params![bill.name, bill.amount, next, bill.account_id, bill.category_id, bill.recurrence],
            )
            .ctx("Failed to generate next recurring bill occurrence")?;
            Some(tx.last_insert_rowid())
        }
        None => None,
    };
    tx.execute(
        "UPDATE bills SET is_paid = 1, paid_transaction_id = ?1, next_bill_id = ?2 WHERE id = ?3",
        params![txn_id, next_bill_id, id],
    )
    .ctx("Failed to update bill status")?;
    tx.commit().ctx("Failed to update bill status")?;
    load_bill(&conn, id)
}

/// Undo a payment: removes the expense it created and the untouched next occurrence.
#[tauri::command]
pub fn unmark_bill_paid(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let (paid_txn, next_bill): (Option<i64>, Option<i64>) = conn
        .query_row("SELECT paid_transaction_id, next_bill_id FROM bills WHERE id = ?1", params![id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()
        .ctx("Failed to load bill")?
        .ok_or("Bill not found")?;
    let tx = conn.transaction().ctx("Failed to update bill status")?;
    if let Some(txn_id) = paid_txn {
        delete_txn(&tx, txn_id)?;
    }
    if let Some(next_id) = next_bill {
        tx.execute("DELETE FROM bills WHERE id = ?1 AND is_paid = 0", params![next_id])
            .ctx("Failed to update bill status")?;
    }
    tx.execute(
        "UPDATE bills SET is_paid = 0, paid_transaction_id = NULL, next_bill_id = NULL WHERE id = ?1",
        params![id],
    )
    .ctx("Failed to update bill status")?;
    tx.commit().ctx("Failed to update bill status")
}
