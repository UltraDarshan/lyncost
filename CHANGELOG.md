# Changelog

## 0.2.5 — 2026-10-24

Layout polish.

- Short labels never break across lines any more: badges and pills ("Grade B · Stable Growth",
  "Surplus Runway"), buttons, table headers, account and holding names, dates and amounts.
- Header rows move their badge or buttons to the next line when space runs out, instead of
  squeezing the title; tables that do not fit scroll sideways inside their card.
- Narrow windows: four-across summary cards become 2 x 2 and figures scale to fit.
- New automated layout check (every page, both themes, three window sizes) and a full UI feature
  test in `scripts/qa/`.

## 0.2.4 — 2026-10-24

- New accent color: Iris (blue-violet), replacing the muted steel blue. Chosen to stay clear of the
  money colors (green income, red expense, amber warnings) and to read well in light and dark mode.
- One-line installer (`install.sh`) and uninstaller (`uninstall.sh`): picks rpm / deb / AppImage,
  verifies SHA256 checksums, keeps user data on uninstall unless `--purge` is confirmed.

## 0.2.3 — 2026-10-23

Wintershogun house style.

- Fonts from wintershogun.com: Plus Jakarta Sans for text and Shippori Mincho for headings
  (bundled, offline). Fixed a bundle rule that silently forced the generic system font on all
  body text, which is why text looked thin and old.
- Larger, firmer small text (labels, table cells, captions), with large figures sized to fit.
- "Snow / frost" palette from the website: slate night backgrounds, frost-blue accents and charts.
- Dashboard: stat tiles reflow 2 x 2 in narrow cards so amounts never clip; hero card fixed in
  dark mode; soft dividers instead of hard black lines in light mode.

## 0.2.2 — 2026-10-23

- Fixed flicker when switching pages: every page now preloads at startup, so switching is
  instant (no blank spinner on first visit), and the page fade-in was removed.

## 0.2.1 — 2026-10-23

Design polish and full UI QA.

- New look: Manrope typeface (bundled, works offline) with aligned money figures; calmer, richer
  colors; soft shadows instead of neon glows; smooth page, dialog and theme transitions;
  themed scrollbars, focus rings and selection; content centered on wide screens.
- Branding: "Created by Wintershogun TechLabs", GitHub link, "Proudly built on Fedora".
- Clearer wording in place of technical jargon on the dashboard, transactions and the
  new-transaction dialog.
- Fixed: stray "0" on budgets without rollover and on loans without a minimum payment.
- Fixed: red/green/amber action buttons (Delete, Repay…) were unreadable in dark mode.
- Fixed: faint warning and red/green text in light mode; grey native dropdowns in light mode.
- Update banner "download" link now opens GitHub Releases.

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
