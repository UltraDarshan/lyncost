# Reddit Marketing & Community Distribution

> **Rule for Reddit:** Never sound like a corporate marketer or an AI bot. Speak developer-to-developer or user-to-user. Focus on the technical craft, the specific frustration you built this to solve (subscription fatigue, RAM bloat, privacy invasion), and invite direct feedback.

---

## 1. r/selfhosted & r/privacy

### Post Angle: Zero Cloud, 100% Local SQLite & Air-Gapped Accounting

**Title Options:**
1. *I built an air-gapped personal finance desktop app with Rust & SQLite because I refused to give bank passwords to cloud budget SaaS*
2. *Show r/selfhosted: Lyncost – A 4 MB, local-first finance manager with zero cloud sync and zero telemetry*
3. *Tired of subscriptions and privacy leaks: I built an offline personal finance app with local SQLite storage*

**Image to Attach:**  
`marketing/assets/screenshots/01-dashboard-dark.png`

**Body Copy:**
```markdown
Hey r/selfhosted,

Like many of you, I wanted a clean way to track my cashflow, recurring bills, and investments without handing over my bank credentials to a cloud aggregator or paying $100/year to SaaS tools that get breached every six months.

I didn't want a heavy Electron web wrapper either, so I spent the last few months building **Lyncost** — a native desktop app built with Rust, Tauri v2, and local SQLite.

Here is how it is built:
- **100% Local SQLite:** All transactions, categories, and account states live in a single ACID `.db` file on your machine. No accounts, no external database servers required, and automated daily backups to a local folder.
- **Air-Gapped by Design:** Zero analytics, no telemetry, no cookies, and no network calls except when you manually check for GitHub releases.
- **Sub-30 MB RAM & 4.1 MB Binary:** Because it compiles directly to native machine code without bundling a whole Chromium instance, it launches instantly and sips system resources.
- **CSV & Statement Friendly:** Import your existing statements via standard CSV with auto-column matching and automated categorization rules.
- **30-Day Cashflow Projection:** Combines scheduled recurring bills with liquid balances so you know your actual safe-to-spend buffer.

It is available as native packages for Linux (.rpm, .deb, AppImage) and Windows.

- GitHub: https://github.com/UltraDarshan/lyncost
- Website: https://wintershogun.com/products/lyncost

I would love your brutally honest feedback on the SQLite schema, UI workflow, and feature roadmap. Happy to answer any technical questions about the Tauri/Rust architecture below!
```

---

## 2. r/linux & r/opensource

### Post Angle: Native Packages, No Flatpak Bloat, Respects Distros

**Title Options:**
1. *I built a 4.1 MB personal finance app in Rust & Tauri v2 with native .rpm, .deb, and AppImage support*
2. *Showcase: Lyncost – Native desktop personal finance for Linux with local SQLite and zero telemetry*
3. *Sick of 500MB Electron budget apps: Here is a sub-30MB RAM Linux desktop finance tool*

**Image to Attach:**  
`marketing/assets/screenshots/01-dashboard-dark.png`

**Body Copy:**
```markdown
Hi r/linux,

Almost every modern personal finance tool today is either an Electron app eating 600 MB of RAM or a closed cloud service demanding your bank password.

I wanted something lightweight, privacy-respecting, and fast on my Fedora workstation, so I engineered **Lyncost**.

Key Linux details:
- **Tiny footprint:** 4.1 MB package size, starts in milliseconds, uses under 30 MB RAM idle.
- **Packages available:** Standalone `.rpm` (Fedora/RHEL/openSUSE), `.deb` (Ubuntu/Debian/Mint), and portable `.AppImage`.
- **One-line installer:** `curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh`
- **System Theme Integration:** Supports system dark/light mode and Wayland natively.
- **Data sovereignty:** Stored in your standard `$XDG_DATA_HOME` using an ACID SQLite database. You own the file, can query it with `sqlite3` CLI, and back it up easily with Borg/restic/rsync.

The source is hosted on GitHub: https://github.com/UltraDarshan/lyncost
Product page: https://wintershogun.com/products/lyncost

Let me know what distros you test it on or if you hit any packaging quirks!
```

---

## 3. r/rust & r/tauri

### Post Angle: Architecture, Performance Benchmarks & Tauri v2 Experience

**Title Options:**
1. *Built a 4 MB desktop finance app using Rust + Tauri v2 + SQLite: Architecture & performance takeaways*
2. *Why we picked Tauri v2 and SQLite over Electron for desktop accounting (benchmarks & findings)*

**Image to Attach:**  
`marketing/assets/screenshots/01-dashboard-dark.png` and `marketing/assets/screenshots/05-investments-dark.png`

**Body Copy:**
```markdown
Hey everyone,

I recently launched **Lyncost**, a personal finance desktop app built entirely on Rust and Tauri v2. I wanted to share a few technical decisions and performance metrics from the project.

### Architecture Highlights
1. **State Management & Persistence:**
   - Database: Bundled SQLite using `rusqlite` with WAL mode enabled.
   - Migrations: Automated sequential SQL schema migrations running on startup.
   - IPC: Tauri `#[tauri::command]` handlers validate incoming inputs with serde and execute transactional queries off the main thread.

2. **Binary & Memory Metrics:**
   - Linux binary: ~4.1 MB
   - Windows installer: ~4.1 MB
   - Idle RSS memory: 24 MB – 32 MB (compared to 350 MB+ on equivalent Electron setups).
   - Cold startup: ~180ms to interactive.

3. **Why Tauri v2:**
   The multi-window capabilities, improved IPC performance, and streamlined cross-platform permissions in Tauri v2 made it a no-brainer over bundling Chromium. The native OS webview handles rendering while the heavy lifting (database indexing, CSV statement parsing, financial math) happens in pure Rust.

Repository: https://github.com/UltraDarshan/lyncost
Site: https://wintershogun.com/products/lyncost

Curious to hear your thoughts on Tauri v2 desktop architecture and any optimization tricks you’ve used in similar local-first Rust apps!
```

---

## 4. r/personalfinance & r/financialindependence

### Post Angle: Cashflow Runway, Zero Subscriptions, Private Net Worth

**Title Options:**
1. *I got tired of $100/yr budget app subscriptions, so I built a 1-time purchase desktop cashflow tracker*
2. *How knowing your '30-Day Safe Cash Runway' changes budgeting (and why we built a local tool for it)*
3. *A private, zero-cloud desktop alternative to YNAB / Mint for tracking expenses & net worth*

**Image to Attach:**  
`marketing/assets/screenshots/03-budgets-dark.png` or `marketing/assets/screenshots/01-dashboard-dark.png`

**Body Copy:**
```markdown
Hey everyone,

When Mint shut down and YNAB pushed another subscription hike, I realized how absurd it is to pay $100+ every single year just to see where your own money goes. Worse, connecting banking passwords to third-party aggregators always left a bad taste in my mouth.

I spent the past few months building **Lyncost** as a clean, local desktop alternative.

What it does differently:
1. **The 30-Day Cashflow Projection:** Most apps tell you what you spent *last month*. Lyncost looks forward: it blends your confirmed recurring bills (rent, utilities, subscriptions) with your current liquid cash to show your exact projected runway 30 days from now.
2. **Category Budget Burn Rates:** Real-time progress bars show whether your spending pace for dining, groceries, and shopping is on track to survive the month without overdrawing.
3. **Net Worth & Asset Allocation:** Tracks liquid cash, savings, stock/crypto portfolios, and liabilities in one unified cockpit.
4. **No Monthly Fees:** 14-day free trial on Windows, then a one-time lifetime license. Zero monthly fees, zero subscriptions.
5. **Completely Private:** Runs entirely on your PC. No bank logins, no accounts, and no data leaving your machine.

Check it out here: https://wintershogun.com/products/lyncost
GitHub: https://github.com/UltraDarshan/lyncost

Hope this helps anyone looking to take back control of their financial data without monthly SaaS overhead!
```

---

## 5. Battle-Tested Reddit Comment Templates

### When someone asks: *"How do I get my bank transactions in if there is no Plaid/bank sync?"*
> "Great question! We built an instant CSV statement importer. Most banks let you export monthly CSV statements in one click. You drop the file into Lyncost, and its column-matcher auto-detects date, amount, and payee. You can also set recurring rules so regular paychecks and utility bills log automatically with zero typing."

### When someone asks: *"Why not just use a spreadsheet (Excel / Google Sheets)?"*
> "Spreadsheets are great, but maintaining recurring rules, split transactions, category burn-rate calculations, and 30-day cash projections manually in Excel gets messy fast. Lyncost gives you the structure and speed of a dedicated app without the cloud lock-in."

### When someone asks: *"Is it open source?"*
> "The Linux version is free and the full source code is available on GitHub under a source-available license: https://github.com/UltraDarshan/lyncost. You can inspect the Rust backend, audit the SQLite schema, or build it yourself."
