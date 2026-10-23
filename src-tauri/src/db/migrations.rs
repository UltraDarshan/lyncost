//! Versioned schema migrations, recorded in `schema_migrations`.
//!
//! 001-009 are the original Lyncost migrations, unchanged so databases created
//! by the original Windows/Linux builds upgrade cleanly. 010 repairs databases
//! written by the interim rebuild that skipped versioning (plaintext PIN,
//! unsigned ledger rows). 011+ are new.

use rusqlite::{params, Connection};

use crate::ledger;
use crate::security;

enum Step {
    Sql(&'static str),
    /// Hash a plaintext PIN and, for unversioned databases, rebuild the ledger.
    Repair,
}

const MIGRATIONS: &[(i64, Step)] = &[
    (1, Step::Sql(include_str!("migrations/001_initial_schema.sql"))),
    (2, Step::Sql(include_str!("migrations/002_goals.sql"))),
    (3, Step::Sql(include_str!("migrations/003_notification_settings.sql"))),
    (4, Step::Sql(include_str!("migrations/004_telegram_sync.sql"))),
    (5, Step::Sql(include_str!("migrations/005_debt_ledger.sql"))),
    (6, Step::Sql(include_str!("migrations/006_payment_methods.sql"))),
    (7, Step::Sql(include_str!("migrations/007_relax_payment_type.sql"))),
    (8, Step::Sql(include_str!("migrations/008_indexes_pin_lockout.sql"))),
    (9, Step::Sql(include_str!("migrations/009_trial_started_at.sql"))),
    (10, Step::Repair),
    (11, Step::Sql(include_str!("migrations/011_bill_payment_links.sql"))),
];

/// Migration 007 rebuilds `transactions`, which needs foreign keys off.
const FK_OFF_MIGRATION: i64 = 7;

pub fn run(conn: &mut Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT CURRENT_TIMESTAMP
        );",
    )
    .map_err(|e| format!("Failed to create schema_migrations table: {e}"))?;

    let current: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))
        .map_err(|e| format!("Failed to read schema version: {e}"))?;

    // Tables present but no recorded versions = database from the unversioned rebuild.
    let unversioned_existing = current == 0 && table_exists(conn, "accounts");

    for (version, step) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        match step {
            Step::Sql(sql) => apply(conn, *version, sql)?,
            Step::Repair => repair(conn, *version, unversioned_existing)?,
        }
    }
    Ok(())
}

fn repair(conn: &mut Connection, version: i64, rebuild_ledger: bool) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| format!("Failed to start migration {version:03}: {e}"))?;
    security::hash_plaintext_pin(&tx).map_err(|e| format!("Failed to secure stored PIN: {e}"))?;
    if rebuild_ledger {
        ledger::rebuild_all(&tx).map_err(|e| format!("Failed to rebuild account ledger: {e}"))?;
    }
    record(&tx, version)?;
    tx.commit().map_err(|e| format!("Failed to commit migration {version:03}: {e}"))
}

fn apply(conn: &mut Connection, version: i64, sql: &str) -> Result<(), String> {
    let fk_off = version == FK_OFF_MIGRATION;
    if fk_off {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(|e| format!("Failed to disable foreign keys for migration {version:03}: {e}"))?;
    }

    let result = (|| {
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed to start migration {version:03} transaction: {e}"))?;
        for stmt in split_statements(sql) {
            if let Err(e) = tx.execute_batch(&stmt) {
                // ALTER TABLE ADD COLUMN is not idempotent in SQLite; re-runs are fine.
                if !e.to_string().contains("duplicate column name") {
                    return Err(format!("Failed to execute migration {version:03} statement: {e}"));
                }
            }
        }
        record(&tx, version)?;
        tx.commit().map_err(|e| format!("Failed to commit migration {version:03}: {e}"))
    })();

    if fk_off {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("Failed to re-enable foreign keys: {e}"))?;
    }
    result
}

fn record(conn: &Connection, version: i64) -> Result<(), String> {
    conn.execute("INSERT OR REPLACE INTO schema_migrations (version) VALUES (?1)", params![version])
        .map(|_| ())
        .map_err(|e| format!("Failed to record migration {version:03}: {e}"))
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Split a migration file into statements. Our SQL has no string literals
/// containing `;`, so dropping `--` comments and splitting on `;` is enough.
fn split_statements(sql: &str) -> Vec<String> {
    let without_comments: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    without_comments
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_reaches_latest_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        let v: i64 = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.last().unwrap().0);
        // Columns from later migrations exist.
        conn.execute("UPDATE app_settings SET failed_pin_attempts = 0, trial_started_at = NULL", []).unwrap();
        conn.prepare("SELECT total_borrowed, debt_type FROM debts").unwrap();
        conn.prepare("SELECT anchor_day FROM recurring_rules").unwrap();
    }

    #[test]
    fn running_twice_is_a_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();
    }

    #[test]
    fn unversioned_rebuild_database_is_repaired() {
        let mut conn = Connection::open_in_memory().unwrap();
        // Shape written by the interim rebuild: v1 tables, no versions, plaintext PIN,
        // leftover empty transactions_v7, unsigned ledger rows.
        conn.execute_batch(include_str!("migrations/001_initial_schema.sql")).unwrap();
        conn.execute_batch(include_str!("migrations/002_goals.sql")).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT);
             CREATE TABLE transactions_v7 (id INTEGER PRIMARY KEY);
             INSERT INTO app_settings (id, base_currency, pin_hash, theme) VALUES (1, 'INR', '123456', 'light');
             INSERT INTO accounts (name, type, opening_balance, current_balance) VALUES ('Bank', 'bank', 1000, 1000);
             INSERT INTO transactions (account_id, type, amount, base_amount, txn_date) VALUES (1, 'expense', 200, 200, '2026-10-01');
             INSERT INTO account_ledger (account_id, txn_type, amount, balance_after, reference_type, reference_id)
                 VALUES (1, 'expense', 200, 0, 'transaction', 1);",
        )
        .unwrap();
        run(&mut conn).unwrap();

        let hash: String = conn.query_row("SELECT pin_hash FROM app_settings", [], |r| r.get(0)).unwrap();
        assert!(hash.starts_with("$2"), "PIN should be bcrypt-hashed");
        assert!(bcrypt::verify("123456", &hash).unwrap());
        let bal: f64 = conn.query_row("SELECT current_balance FROM accounts WHERE id = 1", [], |r| r.get(0)).unwrap();
        assert_eq!(bal, 800.0);
    }
}
