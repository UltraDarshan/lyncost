use rusqlite::{Connection, Result, params};
use serde_json::{Map, Value};
use std::fs;
use std::path::PathBuf;

pub fn get_db_path() -> PathBuf {
    let base_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.lyncost.desktop");
    fs::create_dir_all(&base_dir).ok();
    base_dir.join("lyncost.db")
}

pub fn init_db() -> Result<Connection> {
    let path = get_db_path();
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    let schema_sql = include_str!("schema.sql");
    for statement in schema_sql.split(';') {
        let stmt = statement.trim();
        if !stmt.is_empty() {
            let _ = conn.execute_batch(stmt);
        }
    }

    // Default app settings
    conn.execute(
        "INSERT OR IGNORE INTO app_settings (id, base_currency, theme) VALUES (1, 'INR', 'dark')",
        [],
    )?;

    // Default categories if empty
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
    if count == 0 {
        let default_categories = [
            ("Salary", "income", "Briefcase", "#10b981"),
            ("Freelance", "income", "Laptop", "#06b6d4"),
            ("Investments", "income", "TrendingUp", "#3b82f6"),
            ("Other Income", "income", "PlusCircle", "#8b5cf6"),
            ("Food & Dining", "expense", "Utensils", "#f97316"),
            ("Groceries", "expense", "ShoppingCart", "#eab308"),
            ("Housing & Rent", "expense", "Home", "#ef4444"),
            ("Utilities", "expense", "Zap", "#06b6d4"),
            ("Transportation", "expense", "Car", "#3b82f6"),
            ("Shopping", "expense", "ShoppingBag", "#ec4899"),
            ("Entertainment", "expense", "Film", "#8b5cf6"),
            ("Healthcare", "expense", "HeartPulse", "#14b8a6"),
            ("Education", "expense", "GraduationCap", "#6366f1"),
            ("Personal Care", "expense", "Sparkles", "#f43f5e"),
            ("Debt & Loans", "expense", "CreditCard", "#64748b"),
        ];
        for (name, kind, icon, color) in default_categories {
            let _ = conn.execute(
                "INSERT INTO categories (name, kind, icon, color) VALUES (?1, ?2, ?3, ?4)",
                params![name, kind, icon, color],
            );
        }
    }

    // Default payment methods if empty
    let pm_count: i64 = conn.query_row("SELECT COUNT(*) FROM payment_methods", [], |r| r.get(0))?;
    if pm_count == 0 {
        let defaults = [
            ("Cash", "cash", "Wallet", "#10b981", 1),
            ("Bank Account", "bank", "Building2", "#3b82f6", 0),
            ("Credit Card", "credit_card", "CreditCard", "#ec4899", 0),
            ("UPI / Wallet", "wallet", "Smartphone", "#8b5cf6", 0),
        ];
        for (name, type_key, icon, color, is_default) in defaults {
            let _ = conn.execute(
                "INSERT INTO payment_methods (name, type_key, icon, color, is_default, is_active) VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                params![name, type_key, icon, color, is_default],
            );
        }
    }

    Ok(conn)
}

pub fn rows_to_json(stmt: &mut rusqlite::Statement, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<Value>> {
    let column_names: Vec<String> = stmt.column_names().into_iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query(params)?;
    let mut result = Vec::new();

    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        for (i, col) in column_names.iter().enumerate() {
            let val_ref = row.get_ref(i)?;
            let json_val = match val_ref {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(v) => Value::from(v),
                rusqlite::types::ValueRef::Real(v) => Value::from(v),
                rusqlite::types::ValueRef::Text(v) => Value::from(String::from_utf8_lossy(v).to_string()),
                rusqlite::types::ValueRef::Blob(v) => Value::from(String::from_utf8_lossy(v).to_string()),
            };
            map.insert(col.clone(), json_val);
        }
        result.push(Value::Object(map));
    }

    Ok(result)
}

pub fn row_to_json(stmt: &mut rusqlite::Statement, params: &[&dyn rusqlite::ToSql]) -> Result<Option<Value>> {
    let rows = rows_to_json(stmt, params)?;
    Ok(rows.into_iter().next())
}
