# Lyncost

[![Built with Rust](https://img.shields.io/badge/built_with-Rust-dca282?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Tauri v2](https://img.shields.io/badge/framework-Tauri_v2-24c8db?logo=tauri&logoColor=white)](https://tauri.app/)
[![Database](https://img.shields.io/badge/database-SQLite_ACID-003B57?logo=sqlite&logoColor=white)](https://www.sqlite.org/)
[![License: Source-Available](https://img.shields.io/badge/license-Source--Available-blue)](LICENSE)

Private, offline-first personal finance and investment tracker for Windows and Linux.  
Built by **UltraDarshan** & **Wintershogun TechLabs** · [Official Website](https://wintershogun.com/products/lyncost)

All financial data lives in a local SQLite database on your own machine. Zero cloud dependencies, zero analytics, and zero bank logins.

---

## Key Features

- **Accounts & Net Worth:** Bank accounts, cash wallets, credit cards, investments, and loan facilities with a cryptographically signed running ledger and multi-currency conversion.
- **Transactions & Statements:** Record income, expenses, and transfers with tags. Drag-and-drop CSV statements to import hundreds of transactions in seconds.
- **Budgets & Cashflow:** Weekly, monthly, and custom category budgets with pace warnings, rollover, and 30-day forward cashflow projections.
- **Investments & Holdings:** Stocks, mutual funds, gold, and crypto with real-time profit and loss calculations in your base currency.
- **Debts & Liabilities:** Track loan balances, EMIs, and credit lines with payoff simulators and drawdown history.
- **Financial Health Cockpit:** Real-time metrics including liquid emergency runway, debt burden ratio, and savings rate.
- **Security & Backups:** 6-digit PIN with salted bcrypt hashing and brute-force lockout timers. Automatic daily database snapshots and verified restore.

---

## Installation

### Windows 10 & 11 (64-bit)

Download the official installer directly from the website:
- **[Download Lyncost for Windows (Installer)](https://wintershogun.com/products/lyncost)**
- Or download the standalone setup executable (`Lyncost_1.1.1_x64-setup.exe`) from [Releases](https://github.com/UltraDarshan/lyncost/releases/latest).

Includes a fully featured 14-day free trial, followed by an optional one-time lifetime license key.

### Linux (Fedora, Ubuntu, Debian, Arch, openSUSE)

Run the one-line automated installer:

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh
```

The script automatically detects your distribution and installs the native package (`.rpm` on Fedora/RHEL/openSUSE, `.deb` on Debian/Ubuntu/Mint) or places an AppImage in `~/.local/bin` (no sudo required).

#### Manual Package Downloads

Download pre-built packages from [Releases](https://github.com/UltraDarshan/lyncost/releases/latest):
- **Fedora / RHEL / openSUSE:** `sudo dnf install ./Lyncost-1.1.1-1.x86_64.rpm`
- **Ubuntu / Debian / Mint:** `sudo apt install ./Lyncost_1.1.1_amd64.deb`
- **Universal AppImage:** `chmod +x Lyncost_1.1.1_amd64.AppImage && ./Lyncost_1.1.1_amd64.AppImage`

---

## Uninstall

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/uninstall.sh | sh
```

To purge the application along with all local databases and backups (prompts for confirmation):

```bash
curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/uninstall.sh | sh -s -- --purge
```

---

## Local Data Storage

All data is stored strictly on your local disk in standard application directories:

| Platform | Database Path | Automatic Backups |
| :--- | :--- | :--- |
| **Windows** | `%APPDATA%\com.lyncost.desktop\lyncost.db` | `%APPDATA%\com.lyncost.desktop\backups\` |
| **Linux** | `~/.local/share/com.lyncost.desktop/lyncost.db` | `~/.local/share/com.lyncost.desktop/backups/` |

---

## Architecture & Development

Lyncost uses **Tauri v2** with a native **Rust** backend engine and a bundled React/Tailwind frontend shell.

### Prerequisites

- Rust (stable 1.80+)
- Node.js 18+
- System build dependencies (on Fedora):
  ```bash
  sudo dnf install webkit2gtk4.1-devel gtk3-devel libsoup3-devel openssl-devel librsvg2-devel
  ```

### Build & Test

```bash
# Install dependencies
npm install

# Run the app in development mode
npm run dev

# Run unit tests and IPC contract checks
npm test

# Build release packages
npm run build:linux    # builds rpm, deb, and appimage
```

### Directory Structure

```
dist/                     Bundled UI assets (React / Tailwind)
src-tauri/
  src/lib.rs              App initialization, Tauri plugins, and IPC command registration
  src/commands/           Domain command handlers (accounts, transactions, budgets, etc.)
  src/db/                 SQLite connection, schema migrations, and seed data
  src/ledger.rs           Financial double-entry signed ledger engine
  src/fx.rs               Currency conversion engine
  src/security.rs         PIN verification and lockout protection
  src/contract_tests.rs   IPC contract verification tests
packaging/linux/          AppStream metainfo and .desktop entry templates
scripts/
  check-ipc-contract.py   Validates IPC signature contracts between frontend and Rust
  set-version.py          Automates version bumps across Cargo, packages, and manifests
```

---

## Security & Verification

- Every binary release is cryptographically signed and published with verifiable `SHA256SUMS`.
- Dependencies are scanned against the RustSec Advisory Database prior to every release tag.
- For vulnerability disclosures, review [SECURITY.md](SECURITY.md).

---

## License

Lyncost is published as **source-available** software under the [Lyncost License](LICENSE).  
The source code is provided openly for auditability, privacy verification, and community review.

- **Source Audit:** You may inspect and review the code to verify offline operation and privacy guarantees.
- **Windows Edition:** Distributed via [wintershogun.com/products/lyncost](https://wintershogun.com/products/lyncost).
- **Linux Builds:** Distributed directly via GitHub Releases and the official install script.

© 2026 UltraDarshan · Wintershogun TechLabs. All rights reserved.
