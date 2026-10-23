//! Account ledger: every balance change is a signed row in `account_ledger`,
//! and `accounts.current_balance` is always the sum of an account's rows.
//!
//! Transactions are the only source of ledger rows besides the opening
//! balance, so deleting or editing a transaction can always be undone exactly.

use rusqlite::{params, Connection, OptionalExtension};

use crate::fx;

pub const REF_TRANSACTION: &str = "transaction";
pub const REF_ACCOUNT: &str = "account";

fn insert(
    conn: &Connection,
    account_id: i64,
    txn_type: &str,
    amount: f64,
    reference_type: &str,
    reference_id: i64,
    txn_date: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO account_ledger (account_id, txn_type, amount, balance_after, reference_type, reference_id, txn_date)
         VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6)",
        params![account_id, txn_type, amount, reference_type, reference_id, txn_date],
    )?;
    Ok(())
}

pub fn post_opening(conn: &Connection, account_id: i64, opening: f64) -> rusqlite::Result<()> {
    if opening != 0.0 {
        conn.execute(
            "INSERT INTO account_ledger (account_id, txn_type, amount, balance_after, reference_type, reference_id)
             VALUES (?1, 'opening', ?2, 0, ?3, ?1)",
            params![account_id, opening, REF_ACCOUNT],
        )?;
    }
    recompute(conn, account_id)
}

/// Write the ledger rows for one transaction and refresh the touched balances.
pub fn post_transaction(conn: &Connection, txn_id: i64) -> rusqlite::Result<()> {
    let row = conn
        .query_row(
            "SELECT t.account_id, t.type, t.transfer_to_account_id, t.amount, t.txn_date, a.currency
             FROM transactions t JOIN accounts a ON a.id = t.account_id WHERE t.id = ?1",
            params![txn_id],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((account_id, kind, to_account, amount, date, currency)) = row else {
        return Ok(());
    };

    match kind.as_str() {
        "income" => insert(conn, account_id, "income", amount, REF_TRANSACTION, txn_id, &date)?,
        "expense" => insert(conn, account_id, "expense", -amount, REF_TRANSACTION, txn_id, &date)?,
        "transfer" => {
            insert(conn, account_id, "transfer_out", -amount, REF_TRANSACTION, txn_id, &date)?;
            if let Some(to_id) = to_account {
                let to_currency: String =
                    conn.query_row("SELECT currency FROM accounts WHERE id = ?1", params![to_id], |r| r.get(0))?;
                let received = fx::convert(conn, amount, &currency, &to_currency);
                insert(conn, to_id, "transfer_in", received, REF_TRANSACTION, txn_id, &date)?;
                recompute(conn, to_id)?;
            }
        }
        _ => {}
    }
    recompute(conn, account_id)
}

/// Remove a transaction's ledger rows and refresh the touched balances.
pub fn unpost_transaction(conn: &Connection, txn_id: i64) -> rusqlite::Result<()> {
    let accounts: Vec<i64> = conn
        .prepare("SELECT DISTINCT account_id FROM account_ledger WHERE reference_type = ?1 AND reference_id = ?2")?
        .query_map(params![REF_TRANSACTION, txn_id], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    conn.execute(
        "DELETE FROM account_ledger WHERE reference_type = ?1 AND reference_id = ?2",
        params![REF_TRANSACTION, txn_id],
    )?;
    for id in accounts {
        recompute(conn, id)?;
    }
    Ok(())
}

/// Refresh running balances and the account's current balance from its rows.
pub fn recompute(conn: &Connection, account_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE account_ledger
         SET balance_after = running.bal
         FROM (
             SELECT id, ROUND(SUM(amount) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING), 2) AS bal
             FROM account_ledger
             WHERE account_id = ?1
         ) AS running
         WHERE account_ledger.id = running.id
           AND account_ledger.balance_after IS NOT running.bal",
        params![account_id],
    )?;
    let total: f64 = conn.query_row(
        "SELECT ROUND(COALESCE(SUM(amount), 0), 2) FROM account_ledger WHERE account_id = ?1",
        params![account_id],
        |r| r.get(0),
    )?;
    conn.execute("UPDATE accounts SET current_balance = ?1 WHERE id = ?2", params![total, account_id])?;
    Ok(())
}

/// Rebuild every ledger row from opening balances and transactions.
pub fn rebuild_all(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM account_ledger", [])?;
    let accounts: Vec<(i64, f64)> = conn
        .prepare("SELECT id, opening_balance FROM accounts ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (id, opening) in &accounts {
        post_opening(conn, *id, *opening)?;
    }
    let txns: Vec<i64> = conn
        .prepare("SELECT id FROM transactions ORDER BY txn_date ASC, id ASC")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for id in txns {
        post_transaction(conn, id)?;
    }
    for (id, _) in accounts {
        recompute(conn, id)?;
    }
    Ok(())
}
