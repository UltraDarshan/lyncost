-- Migration 007: Relax payment_type CHECK constraint on transactions table
-- NOTE: runs with foreign_keys OFF (toggled in migrations.rs, outside the transaction).
-- An interim rebuild left an empty transactions_v7 behind; start from a clean slate.
DROP TABLE IF EXISTS transactions_v7;
CREATE TABLE transactions_v7 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES accounts(id),
  type TEXT NOT NULL CHECK (type IN ('income','expense','transfer')),
  category_id INTEGER REFERENCES categories(id),
  transfer_to_account_id INTEGER REFERENCES accounts(id),
  amount REAL NOT NULL,
  base_amount REAL NOT NULL,
  exchange_rate_used REAL DEFAULT 1,
  payment_type TEXT,
  txn_date TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  note TEXT,
  is_confirmed INTEGER DEFAULT 1,
  recurring_rule_id INTEGER REFERENCES recurring_rules(id),
  created_at TEXT DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO transactions_v7 (id, account_id, type, category_id, transfer_to_account_id, amount, base_amount, exchange_rate_used, payment_type, txn_date, note, is_confirmed, recurring_rule_id, created_at)
SELECT id, account_id, type, category_id, transfer_to_account_id, amount, base_amount, exchange_rate_used, payment_type, txn_date, note, is_confirmed, recurring_rule_id, created_at
FROM transactions;
DROP TABLE transactions;
ALTER TABLE transactions_v7 RENAME TO transactions;
