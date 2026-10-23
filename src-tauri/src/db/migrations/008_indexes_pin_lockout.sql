-- Indexes for the hot query paths (transaction lists, ledger recompute, reports)
CREATE INDEX IF NOT EXISTS idx_transactions_date ON transactions(txn_date);
CREATE INDEX IF NOT EXISTS idx_transactions_account ON transactions(account_id);
CREATE INDEX IF NOT EXISTS idx_transactions_transfer_to ON transactions(transfer_to_account_id);
CREATE INDEX IF NOT EXISTS idx_transactions_category ON transactions(category_id);
CREATE INDEX IF NOT EXISTS idx_transactions_type_date ON transactions(type, txn_date);
CREATE INDEX IF NOT EXISTS idx_ledger_account ON account_ledger(account_id, id);
CREATE INDEX IF NOT EXISTS idx_ledger_reference ON account_ledger(reference_type, reference_id);
CREATE INDEX IF NOT EXISTS idx_transaction_tags_tag ON transaction_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_bills_due ON bills(is_paid, due_date);
CREATE INDEX IF NOT EXISTS idx_price_history_holding ON investment_price_history(holding_id);
CREATE INDEX IF NOT EXISTS idx_networth_date ON networth_history(snapshot_date);

-- Persistent PIN brute-force protection (survives app restarts)
ALTER TABLE app_settings ADD COLUMN failed_pin_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE app_settings ADD COLUMN pin_locked_until INTEGER NOT NULL DEFAULT 0;

-- Day-of-month a monthly/yearly recurring rule was created for, so a rule on
-- the 31st returns to the 31st after passing through a shorter month
ALTER TABLE recurring_rules ADD COLUMN anchor_day INTEGER;
UPDATE recurring_rules SET anchor_day = CAST(strftime('%d', next_due_date) AS INTEGER) WHERE anchor_day IS NULL;
