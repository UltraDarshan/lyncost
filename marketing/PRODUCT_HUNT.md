# Product Hunt, Hacker News & Indie Hackers Launch Kit

---

## 🚀 1. Product Hunt Launch Kit

### Basic Listing Info:
- **Product Name:** Lyncost
- **Tagline (under 60 chars):** Private personal finance for Windows & Linux
- **Alternative Tagline:** 4 MB offline personal finance built with Rust & SQLite
- **Pricing Type:** Paid (One-time payment) / Free Trial / Free Linux Edition
- **Categories:** Personal Finance, Productivity, Developer Tools, Desktop Apps
- **Direct Link:** `https://wintershogun.com/products/lyncost`

### Short Description (up to 260 characters):
> Lyncost is a lightweight 4.1 MB desktop finance app built with Rust, Tauri v2, and local SQLite. Tracks expenses, cashflow runway, and net worth completely offline. Zero telemetry, no bank credentials, and no monthly subscriptions.

### Maker's First Comment (Post immediately on launch):
```markdown
Hey Product Hunt community! 👋

I'm Darshan, founder of Winter Shogun and the developer behind Lyncost.

Like many of you, I grew frustrated with modern personal finance apps. Almost everything on the market has become a cloud SaaS that asks for your private bank login, charges $100+ every single year, and runs as an Electron wrapper eating 600 MB of RAM.

Accounting is fundamentally local math. Your personal net worth and spending habits don't belong in a third-party cloud.

So I engineered **Lyncost**:
- **Native Rust & Tauri v2:** 4.1 MB binary that launches instantly and sips under 30 MB of RAM.
- **Air-Gapped & Local SQLite:** Every transaction is stored in an ACID SQLite database on your own SSD. No accounts, no analytics, no network calls.
- **Forward-Looking Cashflow:** Instead of just showing past history, Lyncost projects your safe cash runway 30 days ahead by blending scheduled recurring bills with your liquid balances.
- **Buy Once, Own Forever:** 14-day free trial on Windows, then a single lifetime license. The Linux edition is standalone and native (.rpm, .deb, AppImage).

I’d love to hear your thoughts, feedback on the UI, and feature requests. I’ll be here all day answering questions!

What’s your biggest frustration with current budgeting software?
```

---

## 🟧 2. Hacker News (Show HN) Kit

> **Important for Hacker News:** HN readers will downvote marketing hype, buzzwords ("revolutionary", "all-in-one", "AI-powered"), and aggressive sales copy. Keep it strictly technical, direct, and factual.

### Title Options:
1. `Show HN: Lyncost – A 4 MB offline personal finance desktop app (Rust, Tauri v2, SQLite)`
2. `Show HN: I built an air-gapped personal finance desktop app with Rust and SQLite`
3. `Show HN: Lyncost – Sub-30MB RAM personal accounting app for Windows and Linux`

### Body Text:
```markdown
Hi HN,

I built Lyncost because I wanted a fast, private desktop app to track my personal finances without handing my net-banking credentials to a cloud SaaS aggregator or running a 500 MB Electron instance.

The architecture:
- Backend: Rust with Tauri v2.
- Database: Embedded SQLite (WAL mode, foreign keys, local schema migrations on boot).
- Memory footprint: ~24 MB – 32 MB RSS idle on both Windows 11 and Fedora 41 (Wayland).
- Binary size: ~4.1 MB.
- Network policy: Strictly zero telemetry. The only outbound network call is an optional manual check against the GitHub releases API.

Data is stored locally in standard OS data directories (`%APPDATA%` on Windows, `$XDG_DATA_HOME` on Linux). You can inspect or export the SQLite database directly with the `sqlite3` CLI. For bank transactions, it includes a local CSV importer with automatic column detection and categorization rules.

Releases are available for Windows (installer) and Linux (.rpm, .deb, AppImage).

Source repository: https://github.com/UltraDarshan/lyncost
Website: https://wintershogun.com/products/lyncost

I would appreciate any feedback on the SQLite data model, memory usage, or UX workflows.
```

---

## 🛠️ 3. Indie Hackers Showcase

### Title:
*Why I chose to build a 1-time purchase desktop app in Rust instead of a B2B SaaS*

### Post Summary:
- Discussing subscription fatigue among tech enthusiasts.
- Why local-first desktop apps built with modern tooling (Tauri v2 + Rust) have lower maintenance overhead (no servers to keep up at 3 AM).
- Breaking down the numbers: Pricing at ₹799 / $9.99 lifetime license vs trying to charge $5/mo.
- Conversion takeaways from releasing a native Linux edition alongside Windows.
