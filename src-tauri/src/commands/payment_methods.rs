//! Payment methods (cash, cards, UPI, wallets…) with regional presets.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::AppState;
use crate::util::{opt_bool, opt_string, CmdResult, Ctx};

#[derive(Serialize)]
pub struct PaymentMethod {
    id: i64,
    name: String,
    type_key: String,
    currency: Option<String>,
    icon: Option<String>,
    color: Option<String>,
    is_default: i64,
    is_active: i64,
    created_at: Option<String>,
}

const SELECT_METHOD: &str =
    "SELECT id, name, type_key, currency, icon, color, is_default, is_active, created_at FROM payment_methods";

fn map_method(r: &Row) -> rusqlite::Result<PaymentMethod> {
    Ok(PaymentMethod {
        id: r.get(0)?,
        name: r.get(1)?,
        type_key: r.get(2)?,
        currency: r.get(3)?,
        icon: r.get(4)?,
        color: r.get(5)?,
        is_default: r.get(6)?,
        is_active: r.get(7)?,
        created_at: r.get(8)?,
    })
}

fn load_methods(conn: &Connection) -> CmdResult<Vec<PaymentMethod>> {
    let mut stmt = conn
        .prepare(&format!("{SELECT_METHOD} ORDER BY is_default DESC, id ASC"))
        .ctx("Failed to load payment methods")?;
    let rows = stmt.query_map([], map_method).ctx("Failed to load payment methods")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load payment methods")
}

#[tauri::command]
pub fn get_payment_methods(state: State<'_, AppState>) -> CmdResult<Vec<PaymentMethod>> {
    load_methods(&*state.conn()?)
}

#[derive(Deserialize)]
pub struct MethodPayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    type_key: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    currency: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    icon: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    color: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_default: Option<bool>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_active: Option<bool>,
}

#[tauri::command]
pub fn create_payment_method(payload: MethodPayload, state: State<'_, AppState>) -> CmdResult<PaymentMethod> {
    let name = payload.name.ok_or("Payment method name cannot be empty")?;
    let is_default = payload.is_default.unwrap_or(false);
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to create payment method")?;
    if is_default {
        tx.execute("UPDATE payment_methods SET is_default = 0", []).ctx("Failed to create payment method")?;
    }
    tx.execute(
        "INSERT INTO payment_methods (name, type_key, currency, icon, color, is_default, is_active)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
        params![
            name,
            payload.type_key.as_deref().unwrap_or("custom"),
            payload.currency,
            payload.icon.as_deref().unwrap_or("Wallet"),
            payload.color.as_deref().unwrap_or("#8b5cf6"),
            is_default
        ],
    )
    .ctx("Failed to create payment method")?;
    let id = tx.last_insert_rowid();
    tx.commit().ctx("Failed to create payment method")?;
    conn.query_row(&format!("{SELECT_METHOD} WHERE id = ?1"), params![id], map_method)
        .ctx("Failed to create payment method")
}

#[tauri::command]
pub fn update_payment_method(id: i64, payload: MethodPayload, state: State<'_, AppState>) -> CmdResult<()> {
    let name = payload.name.ok_or("Payment method name cannot be empty")?;
    let is_default = payload.is_default.unwrap_or(false);
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to update payment method")?;
    if is_default {
        tx.execute("UPDATE payment_methods SET is_default = 0 WHERE id != ?1", params![id])
            .ctx("Failed to update payment method")?;
    }
    tx.execute(
        "UPDATE payment_methods
         SET name = ?1, type_key = ?2, currency = ?3, icon = ?4, color = ?5, is_default = ?6, is_active = ?7
         WHERE id = ?8",
        params![
            name,
            payload.type_key.as_deref().unwrap_or("custom"),
            payload.currency,
            payload.icon.as_deref().unwrap_or("Wallet"),
            payload.color.as_deref().unwrap_or("#8b5cf6"),
            is_default,
            payload.is_active.unwrap_or(true),
            id
        ],
    )
    .ctx("Failed to update payment method")?;
    tx.commit().ctx("Failed to update payment method")
}

#[tauri::command]
pub fn delete_payment_method(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    state
        .conn()?
        .execute("DELETE FROM payment_methods WHERE id = ?1", params![id])
        .ctx("Failed to delete payment method")?;
    Ok(())
}

type Preset = (&'static str, &'static str, &'static str, &'static str);

const CASH: Preset = ("Cash", "cash", "Coins", "#10b981");
const BANK: Preset = ("Bank Account", "bank", "Building2", "#3b82f6");
const CREDIT: Preset = ("Credit Card", "credit_card", "CreditCard", "#f43f5e");
const DEBIT: Preset = ("Debit Card", "debit_card", "CreditCard", "#06b6d4");
const APPLE_PAY: Preset = ("Apple Pay", "wallet", "Smartphone", "#94a3b8");
const GOOGLE_PAY: Preset = ("Google Pay", "wallet", "Smartphone", "#4285f4");
const PAYPAL: Preset = ("PayPal", "wallet", "Globe", "#003087");

/// Popular payment methods for a currency's home market.
fn regional_presets(currency: &str) -> Vec<Preset> {
    match currency {
        "INR" => vec![
            CASH,
            BANK,
            CREDIT,
            DEBIT,
            ("UPI (GPay / PhonePe)", "upi", "Zap", "#10b981"),
            ("Paytm Wallet", "wallet", "Smartphone", "#0284c7"),
            ("Net Banking", "bank", "Globe", "#4f46e5"),
        ],
        "USD" => vec![
            CASH,
            ("Checking Account", "bank", "Building2", "#3b82f6"),
            CREDIT,
            DEBIT,
            ("Venmo", "wallet", "Send", "#008cff"),
            ("Zelle", "bank", "Send", "#7414ca"),
            APPLE_PAY,
            PAYPAL,
        ],
        "EUR" => vec![
            CASH,
            ("Bank Account (SEPA)", "bank", "Building2", "#3b82f6"),
            CREDIT,
            DEBIT,
            ("Revolut", "wallet", "Smartphone", "#0075eb"),
            PAYPAL,
            APPLE_PAY,
            GOOGLE_PAY,
        ],
        "GBP" => vec![
            CASH,
            BANK,
            CREDIT,
            DEBIT,
            APPLE_PAY,
            GOOGLE_PAY,
            ("Monzo / Revolut", "wallet", "Smartphone", "#ff3366"),
        ],
        "BRL" => vec![
            ("Dinheiro (Cash)", "cash", "Coins", "#10b981"),
            ("Conta Bancária", "bank", "Building2", "#3b82f6"),
            ("Pix", "upi", "Zap", "#32bcad"),
            ("Cartão de Crédito", "credit_card", "CreditCard", "#f43f5e"),
            ("Cartão de Débito", "debit_card", "CreditCard", "#06b6d4"),
            ("Boleto", "custom", "FileText", "#f59e0b"),
        ],
        "JPY" => vec![
            ("現金 (Cash)", "cash", "Coins", "#10b981"),
            ("銀行振込 (Bank)", "bank", "Building2", "#3b82f6"),
            ("クレジットカード (Credit Card)", "credit_card", "CreditCard", "#f43f5e"),
            ("PayPay", "wallet", "Smartphone", "#ff0033"),
            ("Suica / Pasmo (交通系IC)", "card", "CreditCard", "#22c55e"),
            ("LINE Pay", "wallet", "Smartphone", "#06c755"),
        ],
        _ => vec![
            CASH,
            BANK,
            CREDIT,
            DEBIT,
            ("Mobile Payment", "wallet", "Smartphone", "#8b5cf6"),
            ("Digital Wallet", "wallet", "Smartphone", "#0ea5e9"),
        ],
    }
}

/// Add (or re-enable) the presets for `currency`; existing methods are never duplicated.
#[tauri::command]
pub fn apply_regional_payment_presets(currency: String, state: State<'_, AppState>) -> CmdResult<Vec<PaymentMethod>> {
    let currency = currency.trim().to_uppercase();
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to apply presets")?;
    for (name, type_key, icon, color) in regional_presets(&currency) {
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM payment_methods WHERE LOWER(name) = LOWER(?1)", params![name], |r| r.get(0))
            .optional()
            .ctx("Failed to apply presets")?;
        match existing {
            Some(id) => tx.execute("UPDATE payment_methods SET is_active = 1 WHERE id = ?1", params![id]),
            None => tx.execute(
                "INSERT INTO payment_methods (name, type_key, currency, icon, color, is_default, is_active)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 1)",
                params![name, type_key, currency, icon, color],
            ),
        }
        .ctx("Failed to apply presets")?;
    }
    tx.commit().ctx("Failed to apply presets")?;
    load_methods(&conn)
}
