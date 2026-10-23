# Changelog

## 0.2.0 — 2026-10-23

Rebuilt the Rust backend for the recovered frontend, matching the original app's behaviour.

- Restored the original database schema and all migrations (001–009), so databases from every earlier
  build upgrade in place. Databases from the interim 0.1.9 rebuild are repaired on first launch.
- PIN is stored as a bcrypt hash with persistent brute-force lockout (it was plain text in the interim build).
- Balances come from a signed account ledger; edits and deletions reverse exactly.
- Budgets show real spending, rollover and pace warnings; recurring rules post automatically.
- Paying a bill records the expense and schedules the next occurrence; un-paying undoes both precisely.
- Loans/credit lines: drawdowns and repayments move money in your accounts; credit-line spending is tracked.
- Investments: P/L and totals in the base currency; price history.
- Backups: daily automatic backups (14 kept), verified restore with automatic safety copy and rollback.
- CSV export saves to Downloads; CSV import matches categories case-insensitively and reports failed rows.
- Restored strict Content-Security-Policy; single-instance; desktop notifications for due bills.
- RPM: AppStream metadata, proper icon sizes, license/URL metadata, search keywords.

## 0.1.9

Original release (source lost) and an interim rebuild.
