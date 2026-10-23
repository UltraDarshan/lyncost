# Lyncost

100% Offline, Privacy-First Personal Finance & Investment Tracker for Linux.

Built by **Wintershogun TechLabs**.

## Features

- **Accounts & Net Worth:** Bank, Cash, Credit Card, Investment portfolios with automated net worth tracking and snapshots.
- **Transactions & Ledger:** Categorized income and expenses, transfers, tags, and account ledgers.
- **Budgets & Goals:** Target amounts, progress tracking, and spending envelopes.
- **Debts & Loans:** Track principal, interest, payments, and draws.
- **Investments:** Track stocks, mutual funds, crypto, and calculate profit/loss with historical price snapshots.
- **Bills & Warranties:** Due date reminders and warranty expiry tracking.
- **Shopping Lists & Calculator:** Built-in productivity tools.
- **100% Offline & Private:** Powered by a local SQLite database (`lyncost.db`). Your financial data never leaves your computer.

## Installation on Fedora Linux

Install the RPM package:

```bash
sudo dnf install ./Lyncost-0.1.9-1.x86_64.rpm
```

Or run directly:

```bash
npm run tauri dev
```

## Building

```bash
npm install
npm run tauri build -- --bundles rpm
```
