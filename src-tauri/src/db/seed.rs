//! Default rows for a fresh (or freshly wiped) database. Each group is only
//! seeded when its table is empty, so user edits are never overwritten.

use rusqlite::{params, Connection};

const CATEGORIES: &[(&str, &str, &str, &str)] = &[
    ("Salary", "income", "briefcase", "#10b981"),
    ("Business Income", "income", "trending-up", "#06b6d4"),
    ("Interest", "income", "percent", "#8b5cf6"),
    ("Other Income", "income", "plus-circle", "#64748b"),
    ("Food", "expense", "utensils", "#f97316"),
    ("Transport", "expense", "car", "#3b82f6"),
    ("Bills & Utilities", "expense", "zap", "#eab308"),
    ("Shopping", "expense", "shopping-bag", "#ec4899"),
    ("Entertainment", "expense", "film", "#a855f7"),
    ("Health", "expense", "heart-pulse", "#ef4444"),
    ("Rent", "expense", "home", "#14b8a6"),
    ("Investments", "expense", "line-chart", "#6366f1"),
    ("Other", "expense", "more-horizontal", "#64748b"),
];

const TAGS: &[&str] = &["essential", "discretionary", "vacation", "tax", "personal"];

/// Starter rates into INR so a multi-currency setup works out of the box.
/// They are approximate; users edit them under Settings → Exchange Rates.
const EXCHANGE_RATES_TO_INR: &[(&str, f64)] = &[
    ("USD", 88.0),
    ("EUR", 102.0),
    ("GBP", 118.0),
    ("CAD", 63.0),
    ("AUD", 58.0),
    ("SGD", 68.0),
    ("NZD", 51.0),
    ("AED", 24.0),
];

const PAYMENT_METHODS: &[(&str, &str, &str, &str, i64)] = &[
    ("Cash", "cash", "Coins", "#10b981", 1),
    ("Bank Account", "bank", "Building2", "#3b82f6", 0),
    ("Credit Card", "credit_card", "CreditCard", "#f43f5e", 0),
    ("Debit Card", "debit_card", "CreditCard", "#06b6d4", 0),
];

fn is_empty(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0))
        .map(|n| n == 0)
}

pub fn seed_defaults(conn: &Connection) -> Result<(), String> {
    seed(conn).map_err(|e| format!("Failed to seed default data: {e}"))
}

fn seed(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO app_settings (id, base_currency, theme) VALUES (1, 'INR', 'dark')",
        [],
    )?;

    if is_empty(conn, "categories")? {
        for (name, kind, icon, color) in CATEGORIES {
            conn.execute(
                "INSERT INTO categories (name, kind, icon, color) VALUES (?1, ?2, ?3, ?4)",
                params![name, kind, icon, color],
            )?;
        }
    }

    if is_empty(conn, "tags")? {
        for name in TAGS {
            conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![name])?;
        }
    }

    if is_empty(conn, "exchange_rates")? {
        for (from, rate) in EXCHANGE_RATES_TO_INR {
            conn.execute(
                "INSERT OR IGNORE INTO exchange_rates (from_currency, to_currency, rate) VALUES (?1, 'INR', ?2)",
                params![from, rate],
            )?;
        }
    }

    if is_empty(conn, "payment_methods")? {
        for (name, type_key, icon, color, is_default) in PAYMENT_METHODS {
            conn.execute(
                "INSERT OR IGNORE INTO payment_methods (name, type_key, icon, color, is_default, is_active)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![name, type_key, icon, color, is_default],
            )?;
        }
    }
    Ok(())
}
