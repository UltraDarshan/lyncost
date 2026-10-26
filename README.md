# Lyncost

Private, offline-first personal finance & investment tracker for Linux (Windows version coming soon).
Built by **Wintershogun TechLabs** · Website: **https://lyncost.wintershogun.com**

Your data lives in a local SQLite database and never leaves your computer.

## Features

- **Accounts & net worth** — bank, cash, credit-card, investment and other accounts with a signed ledger, daily net-worth snapshots and multi-currency conversion.
- **Transactions** — income, expenses and transfers with tags, pending/confirmed status, linked warranties and CSV import/export.
- **Budgets & goals** — weekly/monthly/custom budgets with rollover and pace warnings; savings goals with deposits and withdrawals.
- **Debts & loans** — drawdowns, repayments and credit-line spending with full history.
- **Investments** — stocks, mutual funds and crypto with P/L in your base currency and price history.
- **Bills, warranties, shopping list, recurring rules, calculators and reports.**
- **Security & safety** — bcrypt-hashed 6-digit PIN with brute-force lockout, automatic daily backups, verified restore with a safety copy.

## Install

One command for any Linux distribution (x86_64):

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh
```

It installs the latest release, verified against its SHA256 checksum, using the best format for
your system: the `.rpm` on Fedora / RHEL / openSUSE, the `.deb` on Debian / Ubuntu / Mint, or
otherwise an AppImage in your home folder (no sudo needed). Options:

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh -s -- --appimage        # always use the AppImage
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh -s -- --version 1.0.0   # a specific release
```

Or download a package from [Releases](https://github.com/UltraDarshan/lyncost/releases/latest) and install it yourself,
e.g. `sudo dnf install ./Lyncost-1.0.0-1.x86_64.rpm`.

Upgrading from an older Lyncost keeps your data: the database is migrated in place on first launch
(existing PIN, accounts and history are preserved).

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/uninstall.sh | sh
```

Your data and backups are kept. To delete them too (asks you to type DELETE to confirm):

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/uninstall.sh | sh -s -- --purge
```

## Where your data is

| What | Linux path |
| --- | --- |
| Database | `~/.local/share/com.lyncost.desktop/lyncost.db` |
| Automatic & manual backups | `~/.local/share/com.lyncost.desktop/backups/` |
| CSV exports | `~/Downloads/` |

## Development

Requirements: Rust (stable), Node 18+, and on Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel gtk3-devel libsoup3-devel openssl-devel librsvg2-devel
npm install
npm run dev          # run the app
npm test             # Rust unit + IPC contract tests, then the frontend/backend contract check
npm run build:rpm    # -> src-tauri/target/release/bundle/rpm/
```

### Project layout

```
dist/                     Prebuilt frontend (React/Vite bundle). Its source was lost; see note below.
src-tauri/
  src/lib.rs              App setup, plugins and the list of commands exposed to the frontend
  src/commands/           One file per feature (accounts, transactions, budgets, bills, …)
  src/db/                 Connection, versioned migrations (migrations/*.sql) and seed data
  src/ledger.rs           Account ledger: every balance change is a signed ledger row
  src/fx.rs               Currency conversion through the exchange-rate table
  src/security.rs         PIN hashing and lockout
  src/contract_tests.rs   End-to-end tests through Tauri IPC with the frontend's exact payloads
packaging/linux/          Desktop entry template and AppStream metainfo
scripts/
  check-ipc-contract.py   Verifies every frontend invoke() matches a Rust command and its arguments
  set-version.py          Bumps the version everywhere (incl. labels inside dist/)
  qa/                     Hidden-display UI QA: env.sh (Xvfb + tauri-driver), seed.py,
                          flows.py (every feature), layout_check.py (wraps/overflow), capture.py
version.json              Update manifest read by the in-app update check
```

**About `dist/`:** the original frontend source is gone; `dist/` is the original production build
recovered from the released app (its asset hashes match the original Windows release exactly).
The backend was rebuilt to match it. Treat `dist/` as read-only except through `scripts/set-version.py`,
and run `python3 scripts/check-ipc-contract.py` after any backend change.

## Releasing

Follow [RELEASING.md](RELEASING.md): the full checklist from testing to the published release and the website.

## License

Lyncost is **source-available, not open source**. The code is public so you can see how it works
and check that it keeps your data private, but all rights belong to Wintershogun TechLabs.

- The official **Linux** builds are free for everyone, for personal and business use.
- The **Windows** edition (coming soon) is a one-time purchase: ₹799 in India, US$10 elsewhere.
- You may not copy, modify, build, or redistribute the code. Pull requests are not accepted unless invited.

See [LICENSE](LICENSE) or https://lyncost.wintershogun.com/license for the full terms.
Also see [PRIVACY.md](PRIVACY.md), [SECURITY.md](SECURITY.md), and [CONTRIBUTING.md](CONTRIBUTING.md).
