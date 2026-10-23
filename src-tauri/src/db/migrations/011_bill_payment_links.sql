-- Link a paid bill to the exact payment transaction (and next recurring bill)
-- it created, so "unmark paid" can undo precisely instead of guessing by note.
ALTER TABLE bills ADD COLUMN paid_transaction_id INTEGER REFERENCES transactions(id) ON DELETE SET NULL;
ALTER TABLE bills ADD COLUMN next_bill_id INTEGER;
