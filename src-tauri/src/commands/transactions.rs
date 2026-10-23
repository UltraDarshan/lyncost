//! Income, expense and transfer transactions, tags and CSV import.
//!
//! Everything that moves money (manual entries, CSV rows, bill payments,
//! loan draws/repayments, recurring rules) goes through [`insert_txn`], so
//! validation, base-currency conversion and ledger posting live in one place.

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, ToSql};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::fx;
use crate::ledger;
use crate::util::{
    non_empty, opt_bool, opt_f64, opt_i64, opt_string, round2, today_str, vec_string, CmdResult, Ctx,
};

const TAG_SEPARATOR: char = '\u{1f}';

#[derive(Serialize)]
pub struct Transaction {
    id: i64,
    account_id: i64,
    account_name: String,
    account_currency: String,
    #[serde(rename = "type")]
    kind: String,
    category_id: Option<i64>,
    category_name: Option<String>,
    category_icon: Option<String>,
    category_color: Option<String>,
    transfer_to_account_id: Option<i64>,
    transfer_to_account_name: Option<String>,
    transfer_to_account_currency: Option<String>,
    amount: f64,
    base_amount: f64,
    exchange_rate_used: f64,
    payment_type: Option<String>,
    txn_date: String,
    note: Option<String>,
    is_confirmed: i64,
    recurring_rule_id: Option<i64>,
    created_at: Option<String>,
    tags: Vec<String>,
}

const SELECT_TXN: &str = "SELECT
        t.id, t.account_id, a.name, a.currency,
        t.type, t.category_id, c.name, c.icon, c.color,
        t.transfer_to_account_id, a_to.name, a_to.currency,
        t.amount, t.base_amount, COALESCE(t.exchange_rate_used, 1),
        t.payment_type, t.txn_date, t.note, COALESCE(t.is_confirmed, 1),
        t.recurring_rule_id, t.created_at,
        (SELECT GROUP_CONCAT(name, char(31)) FROM (
            SELECT tg.name FROM transaction_tags tt JOIN tags tg ON tg.id = tt.tag_id
            WHERE tt.transaction_id = t.id ORDER BY tg.name ASC
        )) AS tag_names
     FROM transactions t
     JOIN accounts a ON a.id = t.account_id
     LEFT JOIN accounts a_to ON a_to.id = t.transfer_to_account_id
     LEFT JOIN categories c ON c.id = t.category_id";

fn map_txn(r: &Row) -> rusqlite::Result<Transaction> {
    let tags: Option<String> = r.get(21)?;
    Ok(Transaction {
        id: r.get(0)?,
        account_id: r.get(1)?,
        account_name: r.get(2)?,
        account_currency: r.get(3)?,
        kind: r.get(4)?,
        category_id: r.get(5)?,
        category_name: r.get(6)?,
        category_icon: r.get(7)?,
        category_color: r.get(8)?,
        transfer_to_account_id: r.get(9)?,
        transfer_to_account_name: r.get(10)?,
        transfer_to_account_currency: r.get(11)?,
        amount: r.get(12)?,
        base_amount: r.get(13)?,
        exchange_rate_used: r.get(14)?,
        payment_type: r.get(15)?,
        txn_date: r.get(16)?,
        note: r.get(17)?,
        is_confirmed: r.get(18)?,
        recurring_rule_id: r.get(19)?,
        created_at: r.get(20)?,
        tags: tags
            .map(|s| s.split(TAG_SEPARATOR).map(String::from).collect())
            .unwrap_or_default(),
    })
}

pub fn load_txn(conn: &Connection, id: i64) -> CmdResult<Transaction> {
    conn.query_row(&format!("{SELECT_TXN} WHERE t.id = ?1"), params![id], map_txn)
        .map_err(|_| "Transaction not found".to_string())
}

/// A validated-on-insert transaction. Used by every module that moves money.
pub struct NewTxn<'a> {
    pub account_id: i64,
    pub kind: &'a str,
    pub category_id: Option<i64>,
    pub transfer_to_account_id: Option<i64>,
    pub amount: f64,
    pub payment_type: Option<&'a str>,
    pub txn_date: &'a str,
    pub note: Option<&'a str>,
    pub is_confirmed: bool,
    pub recurring_rule_id: Option<i64>,
}

impl<'a> NewTxn<'a> {
    pub fn simple(account_id: i64, kind: &'a str, amount: f64, txn_date: &'a str, note: Option<&'a str>) -> Self {
        Self {
            account_id,
            kind,
            category_id: None,
            transfer_to_account_id: None,
            amount,
            payment_type: None,
            txn_date,
            note,
            is_confirmed: true,
            recurring_rule_id: None,
        }
    }
}

fn account_currency(conn: &Connection, id: i64) -> CmdResult<Option<String>> {
    conn.query_row("SELECT currency FROM accounts WHERE id = ?1", params![id], |r| r.get(0))
        .optional()
        .ctx("Failed to look up account")
}

/// Validate a transaction and return (category_id, transfer_to, base_amount, rate).
fn validate(conn: &Connection, t: &NewTxn) -> CmdResult<(Option<i64>, Option<i64>, f64, f64)> {
    if !matches!(t.kind, "income" | "expense" | "transfer") {
        return Err(format!("Invalid transaction type: {}", t.kind));
    }
    if !t.amount.is_finite() || t.amount <= 0.0 {
        return Err("Transaction amount must be greater than zero".into());
    }
    let currency = account_currency(conn, t.account_id)?.ok_or("Source account not found")?;
    let (category_id, to_account) = if t.kind == "transfer" {
        let to = t.transfer_to_account_id.ok_or("Destination account is required for a transfer")?;
        if to == t.account_id {
            return Err("Source and destination accounts cannot be the same".into());
        }
        account_currency(conn, to)?.ok_or("Destination account not found")?;
        (None, Some(to))
    } else {
        (t.category_id, None)
    };
    if let Some(cat) = category_id {
        let exists = conn
            .query_row("SELECT 1 FROM categories WHERE id = ?1", params![cat], |_| Ok(()))
            .optional()
            .ctx("Failed to look up category")?;
        if exists.is_none() {
            return Err("Selected category no longer exists".into());
        }
    }
    let rate = fx::rate_to_base(conn, &currency);
    Ok((category_id, to_account, round2(t.amount * rate), rate))
}

pub fn insert_txn(conn: &Connection, t: &NewTxn) -> CmdResult<i64> {
    let (category_id, to_account, base_amount, rate) = validate(conn, t)?;
    let date = if t.txn_date.trim().is_empty() { today_str() } else { t.txn_date.trim().to_string() };
    conn.execute(
        "INSERT INTO transactions (
            account_id, type, category_id, transfer_to_account_id,
            amount, base_amount, exchange_rate_used, payment_type,
            txn_date, note, is_confirmed, recurring_rule_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            t.account_id, t.kind, category_id, to_account,
            round2(t.amount), base_amount, rate, t.payment_type,
            date, t.note, t.is_confirmed, t.recurring_rule_id
        ],
    )
    .ctx("Failed to insert transaction")?;
    let id = conn.last_insert_rowid();
    ledger::post_transaction(conn, id).ctx("Failed to insert ledger row")?;
    Ok(id)
}

/// Remove a transaction and every balance effect it had.
pub fn delete_txn(conn: &Connection, id: i64) -> CmdResult<()> {
    ledger::unpost_transaction(conn, id).ctx("Failed to update balances")?;
    conn.execute("UPDATE warranties SET transaction_id = NULL WHERE transaction_id = ?1", params![id])
        .ctx("Failed to unlink warranty")?;
    conn.execute("DELETE FROM transaction_tags WHERE transaction_id = ?1", params![id])
        .ctx("Failed to delete transaction")?;
    conn.execute("DELETE FROM transactions WHERE id = ?1", params![id]).ctx("Failed to delete transaction")?;
    Ok(())
}

fn set_tags(conn: &Connection, txn_id: i64, tags: &[String]) -> CmdResult<()> {
    conn.execute("DELETE FROM transaction_tags WHERE transaction_id = ?1", params![txn_id])
        .ctx("Failed to save tags")?;
    for tag in tags {
        let name = tag.trim();
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![name]).ctx("Failed to save tags")?;
        let tag_id: i64 = conn
            .query_row("SELECT id FROM tags WHERE name = ?1", params![name], |r| r.get(0))
            .ctx("Failed to save tags")?;
        conn.execute(
            "INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id) VALUES (?1, ?2)",
            params![txn_id, tag_id],
        )
        .ctx("Failed to save tags")?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct WarrantyInput {
    #[serde(default, deserialize_with = "opt_string")]
    item_name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    purchase_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    expires_on: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    notes: Option<String>,
}

/// Create or update the warranty linked to a transaction.
fn upsert_warranty(conn: &Connection, txn_id: i64, w: &WarrantyInput, fallback_date: &str) -> CmdResult<()> {
    let item = w.item_name.clone().unwrap_or_else(|| "Item".into());
    let purchase = w.purchase_date.clone().unwrap_or_else(|| fallback_date.to_string());
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM warranties WHERE transaction_id = ?1", params![txn_id], |r| r.get(0))
        .optional()
        .ctx("Failed to link warranty")?;
    match existing {
        Some(id) => conn.execute(
            "UPDATE warranties SET item_name = ?1, purchase_date = ?2, expires_on = ?3, notes = ?4 WHERE id = ?5",
            params![item, purchase, w.expires_on, w.notes, id],
        ),
        None => conn.execute(
            "INSERT INTO warranties (item_name, purchase_date, expires_on, transaction_id, notes)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![item, purchase, w.expires_on, txn_id, w.notes],
        ),
    }
    .ctx("Failed to link warranty")?;
    Ok(())
}

#[derive(Deserialize)]
pub struct TransactionPayload {
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(rename = "type", default, deserialize_with = "opt_string")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    transfer_to_account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    payment_type: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    txn_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_confirmed: Option<bool>,
    #[serde(default, deserialize_with = "vec_string")]
    tags: Vec<String>,
    #[serde(default)]
    warranty: Option<WarrantyInput>,
}

impl TransactionPayload {
    fn as_new(&self) -> CmdResult<NewTxn<'_>> {
        Ok(NewTxn {
            account_id: self.account_id.ok_or("Source account not found")?,
            kind: self.kind.as_deref().unwrap_or("expense"),
            category_id: self.category_id,
            transfer_to_account_id: self.transfer_to_account_id,
            amount: self.amount.unwrap_or(0.0),
            payment_type: self.payment_type.as_deref(),
            txn_date: self.txn_date.as_deref().unwrap_or(""),
            note: self.note.as_deref(),
            is_confirmed: self.is_confirmed.unwrap_or(true),
            recurring_rule_id: None,
        })
    }
}

#[tauri::command]
pub fn create_transaction(payload: TransactionPayload, state: State<'_, AppState>) -> CmdResult<Transaction> {
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to insert transaction")?;
    let id = insert_txn(&tx, &payload.as_new()?)?;
    set_tags(&tx, id, &payload.tags)?;
    if let Some(w) = &payload.warranty {
        upsert_warranty(&tx, id, w, payload.txn_date.as_deref().unwrap_or(&today_str()))?;
    }
    tx.commit().ctx("Failed to insert transaction")?;
    load_txn(&conn, id)
}

#[tauri::command]
pub fn update_transaction(id: i64, payload: TransactionPayload, state: State<'_, AppState>) -> CmdResult<Transaction> {
    let new = payload.as_new()?;
    let mut conn = state.conn()?;
    load_txn(&conn, id)?;
    let tx = conn.transaction().ctx("Failed to update transaction")?;
    let (category_id, to_account, base_amount, rate) = validate(&tx, &new)?;
    ledger::unpost_transaction(&tx, id).ctx("Failed to update balances")?;
    let date = if new.txn_date.trim().is_empty() { today_str() } else { new.txn_date.trim().to_string() };
    tx.execute(
        "UPDATE transactions SET
            account_id = ?1, type = ?2, category_id = ?3, transfer_to_account_id = ?4,
            amount = ?5, base_amount = ?6, exchange_rate_used = ?7, payment_type = ?8,
            txn_date = ?9, note = ?10, is_confirmed = ?11
         WHERE id = ?12",
        params![
            new.account_id, new.kind, category_id, to_account,
            round2(new.amount), base_amount, rate, new.payment_type,
            date, new.note, new.is_confirmed, id
        ],
    )
    .ctx("Failed to update transaction")?;
    ledger::post_transaction(&tx, id).ctx("Failed to update balances")?;
    set_tags(&tx, id, &payload.tags)?;
    if let Some(w) = &payload.warranty {
        upsert_warranty(&tx, id, w, &date)?;
    }
    tx.commit().ctx("Failed to update transaction")?;
    load_txn(&conn, id)
}

#[tauri::command]
pub fn delete_transaction(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to delete transaction")?;
    delete_txn(&tx, id)?;
    tx.commit().ctx("Failed to delete transaction")
}

#[derive(Deserialize, Default)]
pub struct TransactionFilter {
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_string")]
    txn_type: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    start_date: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    end_date: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_confirmed: Option<bool>,
    #[serde(default, deserialize_with = "opt_string")]
    tag: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    search_query: Option<String>,
}

#[tauri::command]
pub fn get_transactions(filter: Option<TransactionFilter>, state: State<'_, AppState>) -> CmdResult<Vec<Transaction>> {
    let f = filter.unwrap_or_default();
    let mut sql = format!("{SELECT_TXN} WHERE 1=1");
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(acc) = f.account_id {
        sql.push_str(" AND (t.account_id = ? OR t.transfer_to_account_id = ?)");
        args.push(Box::new(acc));
        args.push(Box::new(acc));
    }
    if let Some(cat) = f.category_id {
        sql.push_str(" AND t.category_id = ?");
        args.push(Box::new(cat));
    }
    if let Some(kind) = f.txn_type {
        sql.push_str(" AND t.type = ?");
        args.push(Box::new(kind));
    }
    if let Some(start) = f.start_date {
        sql.push_str(" AND DATE(t.txn_date) >= DATE(?)");
        args.push(Box::new(start));
    }
    if let Some(end) = f.end_date {
        sql.push_str(" AND DATE(t.txn_date) <= DATE(?)");
        args.push(Box::new(end));
    }
    if let Some(confirmed) = f.is_confirmed {
        sql.push_str(" AND t.is_confirmed = ?");
        args.push(Box::new(confirmed));
    }
    if let Some(tag) = f.tag {
        sql.push_str(
            " AND t.id IN (SELECT tt.transaction_id FROM transaction_tags tt JOIN tags tg ON tg.id = tt.tag_id WHERE tg.name = ?)",
        );
        args.push(Box::new(tag));
    }
    if let Some(q) = f.search_query {
        sql.push_str(" AND (t.note LIKE ? OR c.name LIKE ? OR a.name LIKE ? OR t.payment_type LIKE ?)");
        let like = format!("%{q}%");
        for _ in 0..4 {
            args.push(Box::new(like.clone()));
        }
    }
    sql.push_str(" ORDER BY t.txn_date DESC, t.id DESC");

    let conn = state.conn()?;
    let mut stmt = conn.prepare(&sql).ctx("Failed to load transactions")?;
    let rows = stmt
        .query_map(params_from_iter(args.iter().map(|a| a.as_ref())), map_txn)
        .ctx("Failed to load transactions")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load transactions")
}

#[derive(Serialize)]
pub struct Tag {
    id: i64,
    name: String,
}

#[tauri::command]
pub fn get_tags(state: State<'_, AppState>) -> CmdResult<Vec<Tag>> {
    let conn = state.conn()?;
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name ASC").ctx("Failed to load tags")?;
    let rows = stmt
        .query_map([], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)? }))
        .ctx("Failed to load tags")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load tags")
}

// --- CSV import ---------------------------------------------------------

#[derive(Deserialize)]
pub struct CsvRow {
    #[serde(default, deserialize_with = "opt_string")]
    date: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    txn_type: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    category_name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    payment_type: Option<String>,
}

#[derive(Deserialize)]
pub struct CsvImportPayload {
    #[serde(default, deserialize_with = "opt_i64")]
    target_account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    default_category_id: Option<i64>,
    #[serde(default)]
    rows: Vec<CsvRow>,
}

#[derive(Serialize)]
pub struct CsvImportResult {
    imported_count: i64,
    failed_rows: Vec<String>,
}

fn find_or_create_category(conn: &Connection, name: &str, kind: &str) -> CmdResult<i64> {
    if let Some(id) = conn
        .query_row("SELECT id FROM categories WHERE LOWER(name) = LOWER(?1) LIMIT 1", params![name], |r| r.get(0))
        .optional()
        .ctx("Failed to look up category")?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO categories (name, kind, icon, color) VALUES (?1, ?2, 'tag', '#10b981')",
        params![name, kind],
    )
    .ctx("Failed to create category")?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn import_csv_transactions(payload: CsvImportPayload, state: State<'_, AppState>) -> CmdResult<CsvImportResult> {
    let account_id = payload.target_account_id.ok_or("Please select a target account.")?;
    let mut conn = state.conn()?;
    let mut tx = conn.transaction().ctx("Failed to start import")?;
    let mut imported = 0;
    let mut failed = Vec::new();

    for (i, row) in payload.rows.iter().enumerate() {
        let label = format!("Row {}", i + 1);
        let kind = row.txn_type.as_deref().unwrap_or("expense").to_lowercase();
        if kind != "income" && kind != "expense" {
            failed.push(format!("{label}: Invalid transaction type '{kind}' (must be 'income' or 'expense')"));
            continue;
        }
        let amount = row.amount.unwrap_or(0.0);
        if amount <= 0.0 {
            failed.push(format!("{label}: Amount must be greater than 0 (got {amount})"));
            continue;
        }
        // Each row is all-or-nothing without aborting the rest of the import.
        let sp = tx.savepoint().ctx("Failed to import row")?;
        let result = (|| -> CmdResult<()> {
            let category_id = match non_empty(row.category_name.clone()) {
                Some(name) => Some(find_or_create_category(&sp, &name, &kind)?),
                None => payload.default_category_id,
            };
            insert_txn(
                &sp,
                &NewTxn {
                    account_id,
                    kind: &kind,
                    category_id,
                    transfer_to_account_id: None,
                    amount,
                    payment_type: row.payment_type.as_deref(),
                    txn_date: row.date.as_deref().unwrap_or(""),
                    note: row.note.as_deref(),
                    is_confirmed: true,
                    recurring_rule_id: None,
                },
            )?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                sp.commit().ctx("Failed to import row")?;
                imported += 1;
            }
            Err(e) => failed.push(format!("{label}: {e}")),
        }
    }
    tx.commit().ctx("Failed to finish import")?;
    Ok(CsvImportResult { imported_count: imported, failed_rows: failed })
}
