use crate::AppState;
use crate::db::{row_to_json, rows_to_json};
use rusqlite::params;
use serde_json::{json, Value};
use tauri::State;

type CmdResult<T> = Result<T, String>;

// --- STARTUP & SETTINGS ---

#[tauri::command]
pub fn get_startup_status() -> Option<String> {
    None
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, base_currency, theme, pin_hash IS NOT NULL as has_pin, last_backup_at, last_networth_snapshot_at, 0 as notify_os, 1 as notify_advance_days, NULL as last_notification_check_at FROM app_settings WHERE id = 1")
        .map_err(|e| e.to_string())?;
    let setting = row_to_json(&mut stmt, &[]).map_err(|e| e.to_string())?;
    Ok(setting.unwrap_or_else(|| json!({
        "id": 1,
        "base_currency": "INR",
        "theme": "dark",
        "has_pin": false,
        "last_backup_at": null,
        "last_networth_snapshot_at": null,
        "notify_os": false,
        "notify_advance_days": 1,
        "last_notification_check_at": null
    })))
}

#[tauri::command]
pub fn set_app_theme(theme: String, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO app_settings (id, theme) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET theme = excluded.theme",
        params![theme],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_base_currency(newCurrency: String, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO app_settings (id, base_currency) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET base_currency = excluded.base_currency",
        params![newCurrency],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn set_initial_pin(pin: String, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE app_settings SET pin_hash = ?1 WHERE id = 1",
        params![pin],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn verify_pin(pin: String, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let pin_hash: Option<String> = conn
        .query_row("SELECT pin_hash FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let valid = match pin_hash {
        Some(expected) => expected == pin,
        None => true,
    };
    Ok(json!({ "valid": valid }))
}

#[tauri::command]
pub fn change_pin(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let old_pin = payload.get("oldPin").and_then(|v| v.as_str()).unwrap_or("");
    let new_pin = payload.get("newPin").and_then(|v| v.as_str()).unwrap_or("");
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let current_hash: Option<String> = conn
        .query_row("SELECT pin_hash FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if let Some(expected) = current_hash {
        if expected != old_pin {
            return Ok(json!({ "success": false, "message": "Incorrect current PIN" }));
        }
    }
    conn.execute("UPDATE app_settings SET pin_hash = ?1 WHERE id = 1", params![new_pin])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "success": true }))
}

#[tauri::command]
pub fn get_trial_status() -> Value {
    json!({
        "is_pro": true,
        "trial_active": false,
        "days_left": 999
    })
}

#[tauri::command]
pub fn verify_license_key(key: String) -> Value {
    json!({
        "valid": true,
        "message": format!("Activated: {}", key)
    })
}

#[tauri::command]
pub fn check_app_update() -> Value {
    json!({
        "has_update": false,
        "version": "0.1.9"
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn install_app_update(_downloadUrl: Option<String>) -> CmdResult<()> {
    Ok(())
}

#[tauri::command]
pub fn restart_application() -> CmdResult<()> {
    Ok(())
}

#[tauri::command]
pub fn wipe_all_data(state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let tables = [
        "transactions", "transaction_tags", "account_ledger", "accounts",
        "budgets", "budget_categories", "goals", "goal_contributions",
        "debts", "debt_transactions", "investment_holdings", "investment_price_history",
        "bills", "warranties", "shopping_list_items", "recurring_rules",
        "exchange_rates", "networth_history"
    ];
    for tbl in tables {
        let _ = conn.execute(&format!("DELETE FROM {}", tbl), []);
    }
    let _ = conn.execute("UPDATE app_settings SET pin_hash = NULL WHERE id = 1", []);
    Ok(())
}

// --- ACCOUNTS ---

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_accounts(includeArchived: Option<bool>, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let sql = if includeArchived.unwrap_or(false) {
        "SELECT id, name, type, currency, opening_balance, current_balance, icon, color, is_archived, created_at FROM accounts ORDER BY is_archived ASC, name ASC"
    } else {
        "SELECT id, name, type, currency, opening_balance, current_balance, icon, color, is_archived, created_at FROM accounts WHERE is_archived = 0 ORDER BY name ASC"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_account(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Account");
    let acc_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("bank");
    let currency = payload.get("currency").and_then(|v| v.as_str()).unwrap_or("INR");
    let opening = payload.get("opening_balance").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO accounts (name, type, currency, opening_balance, current_balance, icon, color, is_archived) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, 0)",
        params![name, acc_type, currency, opening, icon, color],
    ).map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    if opening != 0.0 {
        let _ = conn.execute(
            "INSERT INTO account_ledger (account_id, txn_type, amount, balance_after, reference_type, reference_id) VALUES (?1, 'opening', ?2, ?2, 'opening', ?1)",
            params![id, opening],
        );
    }

    let mut stmt = conn.prepare("SELECT * FROM accounts WHERE id = ?1").map_err(|e| e.to_string())?;
    let acc = row_to_json(&mut stmt, &[&id]).map_err(|e| e.to_string())?;
    Ok(acc.unwrap_or(json!({ "id": id })))
}

#[tauri::command]
pub fn update_account(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let acc_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("bank");
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());

    conn.execute(
        "UPDATE accounts SET name = ?1, type = ?2, icon = ?3, color = ?4 WHERE id = ?5",
        params![name, acc_type, icon, color, id],
    ).map_err(|e| e.to_string())?;

    let mut stmt = conn.prepare("SELECT * FROM accounts WHERE id = ?1").map_err(|e| e.to_string())?;
    let acc = row_to_json(&mut stmt, &[&id]).map_err(|e| e.to_string())?;
    Ok(acc.unwrap_or(json!({ "id": id })))
}

#[tauri::command]
pub fn delete_account(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.execute("DELETE FROM account_ledger WHERE account_id = ?1", params![id]);
    let _ = conn.execute("DELETE FROM transactions WHERE account_id = ?1 OR transfer_to_account_id = ?1", params![id]);
    conn.execute("DELETE FROM accounts WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn archive_account(id: i64, archive: bool, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE accounts SET is_archived = ?1 WHERE id = ?2",
        params![if archive { 1 } else { 0 }, id],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_account_ledger(accountId: i64, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT * FROM account_ledger WHERE account_id = ?1 ORDER BY txn_date DESC, id DESC")
        .map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[&accountId]).map_err(|e| e.to_string())
}

// --- TRANSACTIONS & TAGS ---

#[tauri::command]
pub fn get_transactions(filter: Option<Value>, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let account_id = filter.as_ref().and_then(|f| f.get("account_id")).and_then(|v| v.as_i64());
    
    let mut sql = String::from(
        "SELECT t.id, t.account_id, t.type, t.category_id, t.transfer_to_account_id,
                t.amount, t.base_amount, t.exchange_rate_used, t.payment_type, t.txn_date,
                t.note, t.is_confirmed, t.recurring_rule_id, t.created_at,
                a.name as account_name, c.name as category_name
         FROM transactions t
         LEFT JOIN accounts a ON a.id = t.account_id
         LEFT JOIN categories c ON c.id = t.category_id "
    );

    if let Some(acc_id) = account_id {
        sql.push_str(&format!("WHERE t.account_id = {} OR t.transfer_to_account_id = {} ", acc_id, acc_id));
    }
    sql.push_str("ORDER BY t.txn_date DESC, t.id DESC LIMIT 1000");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut txns = rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())?;

    for txn in txns.iter_mut() {
        if let Some(id) = txn.get("id").and_then(|v| v.as_i64()) {
            let mut tag_stmt = conn.prepare(
                "SELECT tg.name FROM transaction_tags tt JOIN tags tg ON tg.id = tt.tag_id WHERE tt.transaction_id = ?1"
            ).map_err(|e| e.to_string())?;
            let tag_rows = rows_to_json(&mut tag_stmt, &[&id]).map_err(|e| e.to_string())?;
            let tags: Vec<Value> = tag_rows.into_iter().filter_map(|t| t.get("name").cloned()).collect();
            if let Some(obj) = txn.as_object_mut() {
                obj.insert("tags".to_string(), Value::Array(tags));
            }
        }
    }

    Ok(txns)
}

#[tauri::command]
pub fn create_transaction(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let acc_id = payload.get("account_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let txn_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("expense");
    let cat_id = payload.get("category_id").and_then(|v| v.as_i64());
    let to_acc_id = payload.get("transfer_to_account_id").and_then(|v| v.as_i64());
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let base_amount = payload.get("base_amount").and_then(|v| v.as_f64()).unwrap_or(amount);
    let rate = payload.get("exchange_rate_used").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let payment_type = payload.get("payment_type").and_then(|v| v.as_str());
    let txn_date = payload.get("txn_date").and_then(|v| v.as_str()).unwrap_or("");
    let note = payload.get("note").and_then(|v| v.as_str());

    tx.execute(
        "INSERT INTO transactions (account_id, type, category_id, transfer_to_account_id, amount, base_amount, exchange_rate_used, payment_type, txn_date, note, is_confirmed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CASE WHEN ?9 != '' THEN ?9 ELSE CURRENT_TIMESTAMP END, ?10, 1)",
        params![acc_id, txn_type, cat_id, to_acc_id, amount, base_amount, rate, payment_type, txn_date, note],
    ).map_err(|e| e.to_string())?;

    let txn_id = tx.last_insert_rowid();

    // Update account balances
    match txn_type {
        "income" => {
            tx.execute("UPDATE accounts SET current_balance = current_balance + ?1 WHERE id = ?2", params![amount, acc_id]).ok();
        }
        "expense" => {
            tx.execute("UPDATE accounts SET current_balance = current_balance - ?1 WHERE id = ?2", params![amount, acc_id]).ok();
        }
        "transfer" => {
            tx.execute("UPDATE accounts SET current_balance = current_balance - ?1 WHERE id = ?2", params![amount, acc_id]).ok();
            if let Some(to_id) = to_acc_id {
                tx.execute("UPDATE accounts SET current_balance = current_balance + ?1 WHERE id = ?2", params![amount, to_id]).ok();
            }
        }
        _ => {}
    }

    // Insert ledger
    let _ = tx.execute(
        "INSERT INTO account_ledger (account_id, txn_type, amount, balance_after, reference_type, reference_id) VALUES (?1, ?2, ?3, 0, 'transaction', ?4)",
        params![acc_id, txn_type, amount, txn_id],
    );

    // Tags
    if let Some(tags) = payload.get("tags").and_then(|v| v.as_array()) {
        for tag in tags {
            if let Some(tag_name) = tag.as_str() {
                let _ = tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", params![tag_name]);
                let tag_id: Result<i64, _> = tx.query_row("SELECT id FROM tags WHERE name = ?1", params![tag_name], |r| r.get(0));
                if let Ok(tid) = tag_id {
                    let _ = tx.execute("INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id) VALUES (?1, ?2)", params![txn_id, tid]);
                }
            }
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(json!({ "id": txn_id }))
}

#[tauri::command]
pub fn update_transaction(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let note = payload.get("note").and_then(|v| v.as_str());
    let cat_id = payload.get("category_id").and_then(|v| v.as_i64());
    conn.execute(
        "UPDATE transactions SET note = ?1, category_id = ?2 WHERE id = ?3",
        params![note, cat_id, id],
    ).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_transaction(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let row: Result<(i64, String, f64, Option<i64>), _> = tx.query_row(
        "SELECT account_id, type, amount, transfer_to_account_id FROM transactions WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    );

    if let Ok((acc_id, txn_type, amount, to_acc_id)) = row {
        match txn_type.as_str() {
            "income" => {
                tx.execute("UPDATE accounts SET current_balance = current_balance - ?1 WHERE id = ?2", params![amount, acc_id]).ok();
            }
            "expense" => {
                tx.execute("UPDATE accounts SET current_balance = current_balance + ?1 WHERE id = ?2", params![amount, acc_id]).ok();
            }
            "transfer" => {
                tx.execute("UPDATE accounts SET current_balance = current_balance + ?1 WHERE id = ?2", params![amount, acc_id]).ok();
                if let Some(to_id) = to_acc_id {
                    tx.execute("UPDATE accounts SET current_balance = current_balance - ?1 WHERE id = ?2", params![amount, to_id]).ok();
                }
            }
            _ => {}
        }
    }

    let _ = tx.execute("DELETE FROM account_ledger WHERE reference_type = 'transaction' AND reference_id = ?1", params![id]);
    let _ = tx.execute("DELETE FROM transaction_tags WHERE transaction_id = ?1", params![id]);
    tx.execute("DELETE FROM transactions WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_tags(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM tags ORDER BY name ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_csv_transactions(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    if let Some(items) = payload.as_array() {
        for item in items {
            let _ = create_transaction(item.clone(), state.clone());
        }
        Ok(json!({ "success": true, "count": items.len() }))
    } else {
        Ok(json!({ "success": false, "count": 0 }))
    }
}

// --- CATEGORIES ---

#[tauri::command]
pub fn get_categories(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM categories ORDER BY kind ASC, name ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_category(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Category");
    let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("expense");
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());
    let parent_id = payload.get("parent_id").and_then(|v| v.as_i64());

    conn.execute(
        "INSERT INTO categories (name, kind, parent_id, icon, color) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![name, kind, parent_id, icon, color],
    ).map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn update_category(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let kind = payload.get("kind").and_then(|v| v.as_str()).unwrap_or("expense");
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());
    let parent_id = payload.get("parent_id").and_then(|v| v.as_i64());

    conn.execute(
        "UPDATE categories SET name = ?1, kind = ?2, parent_id = ?3, icon = ?4, color = ?5 WHERE id = ?6",
        params![name, kind, parent_id, icon, color, id],
    ).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_category(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM categories WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// --- BUDGETS ---

#[tauri::command]
pub fn get_budgets(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM budgets WHERE is_active = 1 ORDER BY name ASC").map_err(|e| e.to_string())?;
    let mut budgets = rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())?;
    for b in budgets.iter_mut() {
        if let Some(obj) = b.as_object_mut() {
            obj.insert("spent".to_string(), json!(0.0));
            obj.insert("categories".to_string(), json!([]));
        }
    }
    Ok(budgets)
}

#[tauri::command]
pub fn create_budget(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Budget");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let period = payload.get("period").and_then(|v| v.as_str()).unwrap_or("monthly");
    let start_date = payload.get("start_date").and_then(|v| v.as_str()).unwrap_or("2026-01-01");
    let end_date = payload.get("end_date").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO budgets (name, amount, period, start_date, end_date, rollover, is_active) VALUES (?1, ?2, ?3, ?4, ?5, 0, 1)",
        params![name, amount, period, start_date, end_date],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_budget(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    conn.execute("UPDATE budgets SET name = ?1, amount = ?2 WHERE id = ?3", params![name, amount, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_budget(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM budgets WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn get_month_summary(month: Option<String>, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let m = month.unwrap_or_else(|| chrono::Local::now().format("%Y-%m").to_string());
    let pattern = format!("{}%", m);

    let income: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0.0) FROM transactions WHERE type = 'income' AND txn_date LIKE ?1",
        params![pattern],
        |r| r.get(0),
    ).unwrap_or(0.0);

    let expense: f64 = conn.query_row(
        "SELECT COALESCE(SUM(amount), 0.0) FROM transactions WHERE type = 'expense' AND txn_date LIKE ?1",
        params![pattern],
        |r| r.get(0),
    ).unwrap_or(0.0);

    Ok(json!({
        "total_income": income,
        "total_expense": expense,
        "net_savings": income - expense,
        "savings_rate": if income > 0.0 { ((income - expense) / income) * 100.0 } else { 0.0 }
    }))
}

// --- GOALS ---

#[tauri::command]
pub fn get_goals(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM goals ORDER BY created_at DESC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_goal(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Goal");
    let target = payload.get("target_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let current = payload.get("current_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let target_date = payload.get("target_date").and_then(|v| v.as_str());
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO goals (name, target_amount, current_amount, target_date, icon, color) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![name, target, current, target_date, icon, color],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_goal(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let target = payload.get("target_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let current = payload.get("current_amount").and_then(|v| v.as_f64()).unwrap_or(0.0);

    conn.execute(
        "UPDATE goals SET name = ?1, target_amount = ?2, current_amount = ?3 WHERE id = ?4",
        params![name, target, current, id],
    ).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_goal(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.execute("DELETE FROM goal_contributions WHERE goal_id = ?1", params![id]);
    conn.execute("DELETE FROM goals WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn contribute_to_goal(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let goal_id = payload.get("goal_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let note = payload.get("note").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO goal_contributions (goal_id, amount, note) VALUES (?1, ?2, ?3)",
        params![goal_id, amount, note],
    ).map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE goals SET current_amount = current_amount + ?1 WHERE id = ?2",
        params![amount, goal_id],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

// --- DEBTS ---

#[tauri::command]
pub fn get_debts(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM debts WHERE is_active = 1 ORDER BY due_date ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_debt(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Debt");
    let principal = payload.get("principal").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let rate = payload.get("interest_rate").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let due_date = payload.get("due_date").and_then(|v| v.as_str());
    let notes = payload.get("notes").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO debts (name, principal, current_balance, interest_rate, due_date, notes, is_active) VALUES (?1, ?2, ?2, ?3, ?4, ?5, 1)",
        params![name, principal, rate, due_date, notes],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_debt(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let balance = payload.get("current_balance").and_then(|v| v.as_f64()).unwrap_or(0.0);
    conn.execute("UPDATE debts SET name = ?1, current_balance = ?2 WHERE id = ?3", params![name, balance, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_debt(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.execute("DELETE FROM debt_transactions WHERE debt_id = ?1", params![id]);
    conn.execute("DELETE FROM debts WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn spend_from_debt(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let debt_id = payload.get("debt_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let txn_date = payload.get("txn_date").and_then(|v| v.as_str()).unwrap_or("");

    conn.execute(
        "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date) VALUES (?1, 'expense', ?2, ?3)",
        params![debt_id, amount, txn_date],
    ).map_err(|e| e.to_string())?;

    conn.execute("UPDATE debts SET current_balance = current_balance + ?1 WHERE id = ?2", params![amount, debt_id])
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

#[tauri::command]
pub fn draw_debt_funds(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    spend_from_debt(payload, state)
}

#[tauri::command]
pub fn record_debt_payment(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let debt_id = payload.get("debt_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let txn_date = payload.get("txn_date").and_then(|v| v.as_str()).unwrap_or("");

    conn.execute(
        "INSERT INTO debt_transactions (debt_id, txn_type, amount, txn_date) VALUES (?1, 'payment', ?2, ?3)",
        params![debt_id, amount, txn_date],
    ).map_err(|e| e.to_string())?;

    conn.execute("UPDATE debts SET current_balance = current_balance - ?1 WHERE id = ?2", params![amount, debt_id])
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_debt_transactions(debtId: i64, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM debt_transactions WHERE debt_id = ?1 ORDER BY txn_date DESC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[&debtId]).map_err(|e| e.to_string())
}

// --- INVESTMENTS & HOLDINGS ---

#[tauri::command]
pub fn get_holdings(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM investment_holdings WHERE is_archived = 0 ORDER BY symbol ASC").map_err(|e| e.to_string())?;
    let mut holdings = rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())?;
    for h in holdings.iter_mut() {
        if let Some(obj) = h.as_object_mut() {
            let qty = obj.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let buy = obj.get("avg_buy_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let last = obj.get("last_price").and_then(|v| v.as_f64()).unwrap_or(buy);
            let invested = qty * buy;
            let current = qty * last;
            let gain = current - invested;
            let percent = if invested > 0.0 { (gain / invested) * 100.0 } else { 0.0 };
            obj.insert("total_invested".to_string(), json!(invested));
            obj.insert("current_value".to_string(), json!(current));
            obj.insert("gain_loss".to_string(), json!(gain));
            obj.insert("gain_loss_percent".to_string(), json!(percent));
        }
    }
    Ok(holdings)
}

#[tauri::command]
pub fn create_holding(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let acc_id = payload.get("account_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let asset_type = payload.get("asset_type").and_then(|v| v.as_str()).unwrap_or("stock");
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let name = payload.get("name").and_then(|v| v.as_str());
    let qty = payload.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let buy = payload.get("avg_buy_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let last = payload.get("last_price").and_then(|v| v.as_f64()).unwrap_or(buy);
    let currency = payload.get("currency").and_then(|v| v.as_str()).unwrap_or("INR");

    conn.execute(
        "INSERT INTO investment_holdings (account_id, asset_type, symbol, name, quantity, avg_buy_price, currency, last_price, is_archived) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
        params![acc_id, asset_type, symbol, name, qty, buy, currency, last],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_holding(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let qty = payload.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let buy = payload.get("avg_buy_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let last = payload.get("last_price").and_then(|v| v.as_f64()).unwrap_or(0.0);

    conn.execute(
        "UPDATE investment_holdings SET quantity = ?1, avg_buy_price = ?2, last_price = ?3 WHERE id = ?4",
        params![qty, buy, last, id],
    ).map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_holding(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let _ = conn.execute("DELETE FROM investment_price_history WHERE holding_id = ?1", params![id]);
    conn.execute("DELETE FROM investment_holdings WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn archive_holding(id: i64, isArchived: bool, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE investment_holdings SET is_archived = ?1 WHERE id = ?2", params![if isArchived { 1 } else { 0 }, id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_holding_price(payload: Value, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let id = payload.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let price = payload.get("price").and_then(|v| v.as_f64()).unwrap_or(0.0);

    conn.execute("UPDATE investment_holdings SET last_price = ?1 WHERE id = ?2", params![price, id])
        .map_err(|e| e.to_string())?;
    let _ = conn.execute("INSERT INTO investment_price_history (holding_id, price) VALUES (?1, ?2)", params![id, price]);
    Ok(())
}

#[tauri::command]
pub fn bulk_update_holding_prices(updates: Vec<Value>, state: State<'_, AppState>) -> CmdResult<()> {
    for u in updates {
        let _ = update_holding_price(u, state.clone());
    }
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_holding_price_history(holdingId: i64, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM investment_price_history WHERE holding_id = ?1 ORDER BY recorded_at ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[&holdingId]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_portfolio_summary(state: State<'_, AppState>) -> CmdResult<Value> {
    let holdings = get_holdings(state)?;
    let mut total_invested = 0.0;
    let mut current_value = 0.0;
    for h in holdings {
        total_invested += h.get("total_invested").and_then(|v| v.as_f64()).unwrap_or(0.0);
        current_value += h.get("current_value").and_then(|v| v.as_f64()).unwrap_or(0.0);
    }
    let gain = current_value - total_invested;
    let percent = if total_invested > 0.0 { (gain / total_invested) * 100.0 } else { 0.0 };
    Ok(json!({
        "total_invested": total_invested,
        "current_value": current_value,
        "total_gain_loss": gain,
        "total_gain_loss_percent": percent
    }))
}

// --- BILLS ---

#[tauri::command]
pub fn get_bills(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM bills ORDER BY due_date ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_bill(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Bill");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let due_date = payload.get("due_date").and_then(|v| v.as_str()).unwrap_or("");
    let recurrence = payload.get("recurrence").and_then(|v| v.as_str()).unwrap_or("none");

    conn.execute(
        "INSERT INTO bills (name, amount, due_date, is_paid, recurrence) VALUES (?1, ?2, ?3, 0, ?4)",
        params![name, amount, due_date, recurrence],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_bill(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let due_date = payload.get("due_date").and_then(|v| v.as_str()).unwrap_or("");

    conn.execute("UPDATE bills SET name = ?1, amount = ?2, due_date = ?3 WHERE id = ?4", params![name, amount, due_date, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_bill(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM bills WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn mark_bill_paid(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE bills SET is_paid = 1 WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn unmark_bill_paid(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE bills SET is_paid = 0 WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// --- WARRANTIES ---

#[tauri::command]
pub fn get_warranties(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM warranties ORDER BY expires_on ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_warranty(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let item = payload.get("item_name").and_then(|v| v.as_str()).unwrap_or("Item");
    let purchase = payload.get("purchase_date").and_then(|v| v.as_str());
    let expires = payload.get("expires_on").and_then(|v| v.as_str());
    let notes = payload.get("notes").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO warranties (item_name, purchase_date, expires_on, notes) VALUES (?1, ?2, ?3, ?4)",
        params![item, purchase, expires, notes],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_warranty(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let item = payload.get("item_name").and_then(|v| v.as_str()).unwrap_or("");
    let expires = payload.get("expires_on").and_then(|v| v.as_str());
    let notes = payload.get("notes").and_then(|v| v.as_str());

    conn.execute("UPDATE warranties SET item_name = ?1, expires_on = ?2, notes = ?3 WHERE id = ?4", params![item, expires, notes, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_warranty(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM warranties WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// --- SHOPPING LIST ---

#[tauri::command]
pub fn get_shopping_items(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM shopping_list_items ORDER BY is_checked ASC, id DESC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_shopping_item(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Item");
    let note = payload.get("note").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO shopping_list_items (name, is_checked, note) VALUES (?1, 0, ?2)",
        params![name, note],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_shopping_item(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let note = payload.get("note").and_then(|v| v.as_str());

    conn.execute("UPDATE shopping_list_items SET name = ?1, note = ?2 WHERE id = ?3", params![name, note, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn toggle_shopping_item(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("UPDATE shopping_list_items SET is_checked = 1 - is_checked WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_shopping_item(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM shopping_list_items WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn clear_completed_shopping_items(state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM shopping_list_items WHERE is_checked = 1", []).map_err(|e| e.to_string())?;
    Ok(())
}

// --- RECURRING RULES ---

#[tauri::command]
pub fn get_recurring_rules(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM recurring_rules WHERE is_active = 1 ORDER BY next_due_date ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_recurring_rule(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Rule");
    let acc_id = payload.get("account_id").and_then(|v| v.as_i64()).unwrap_or(0);
    let r_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("expense");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let freq = payload.get("frequency").and_then(|v| v.as_str()).unwrap_or("monthly");
    let next_due = payload.get("next_due_date").and_then(|v| v.as_str()).unwrap_or("");

    conn.execute(
        "INSERT INTO recurring_rules (name, account_id, type, amount, frequency, next_due_date, is_active) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
        params![name, acc_id, r_type, amount, freq, next_due],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_recurring_rule(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let amount = payload.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    conn.execute("UPDATE recurring_rules SET name = ?1, amount = ?2 WHERE id = ?3", params![name, amount, id])
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_recurring_rule(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM recurring_rules WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn process_recurring_rules() -> CmdResult<()> {
    Ok(())
}

// --- EXCHANGE RATES ---

#[tauri::command]
pub fn get_exchange_rates(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM exchange_rates ORDER BY from_currency ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_exchange_rate(payload: Value, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let from = payload.get("from_currency").and_then(|v| v.as_str()).unwrap_or("");
    let to = payload.get("to_currency").and_then(|v| v.as_str()).unwrap_or("");
    let rate = payload.get("rate").and_then(|v| v.as_f64()).unwrap_or(1.0);

    conn.execute(
        "INSERT INTO exchange_rates (from_currency, to_currency, rate) VALUES (?1, ?2, ?3)
         ON CONFLICT(from_currency, to_currency) DO UPDATE SET rate = excluded.rate, updated_at = CURRENT_TIMESTAMP",
        params![from, to, rate],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn delete_exchange_rate(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM exchange_rates WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

// --- PAYMENT METHODS ---

#[tauri::command]
pub fn get_payment_methods(state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare("SELECT * FROM payment_methods WHERE is_active = 1 ORDER BY is_default DESC, name ASC").map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_payment_method(payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("Method");
    let type_key = payload.get("type_key").and_then(|v| v.as_str()).unwrap_or("custom");
    let icon = payload.get("icon").and_then(|v| v.as_str());
    let color = payload.get("color").and_then(|v| v.as_str());

    conn.execute(
        "INSERT INTO payment_methods (name, type_key, icon, color, is_default, is_active) VALUES (?1, ?2, ?3, ?4, 0, 1)",
        params![name, type_key, icon, color],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": conn.last_insert_rowid() }))
}

#[tauri::command]
pub fn update_payment_method(id: i64, payload: Value, state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let is_default = payload.get("is_default").and_then(|v| v.as_bool()).unwrap_or(false);

    if is_default {
        let _ = conn.execute("UPDATE payment_methods SET is_default = 0", []);
    }

    conn.execute(
        "UPDATE payment_methods SET name = ?1, is_default = ?2 WHERE id = ?3",
        params![name, if is_default { 1 } else { 0 }, id],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "id": id }))
}

#[tauri::command]
pub fn delete_payment_method(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM payment_methods WHERE id = ?1", params![id]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn apply_regional_payment_presets(currency: String, state: State<'_, AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if currency == "INR" {
        let presets = [
            ("UPI (GPay / PhonePe)", "wallet", "Smartphone", "#10b981"),
            ("Net Banking", "bank", "Building2", "#3b82f6"),
            ("RuPay / Debit Card", "card", "CreditCard", "#ec4899"),
        ];
        for (name, type_key, icon, color) in presets {
            let _ = conn.execute(
                "INSERT OR IGNORE INTO payment_methods (name, type_key, icon, color, is_default, is_active) VALUES (?1, ?2, ?3, ?4, 0, 1)",
                params![name, type_key, icon, color],
            );
        }
    }
    Ok(())
}

// --- NET WORTH & REPORTS ---

#[tauri::command]
pub fn get_net_worth_summary(state: State<'_, AppState>) -> CmdResult<Value> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let accounts: f64 = conn.query_row(
        "SELECT COALESCE(SUM(current_balance), 0.0) FROM accounts WHERE is_archived = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    let holdings: f64 = conn.query_row(
        "SELECT COALESCE(SUM(quantity * last_price), 0.0) FROM investment_holdings WHERE is_archived = 0",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    let debts: f64 = conn.query_row(
        "SELECT COALESCE(SUM(current_balance), 0.0) FROM debts WHERE is_active = 1",
        [],
        |r| r.get(0),
    ).unwrap_or(0.0);

    let net_worth = accounts + holdings - debts;
    Ok(json!({
        "total_accounts": accounts,
        "total_investments": holdings,
        "total_debts": debts,
        "net_worth": net_worth
    }))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_net_worth_history(limitDays: Option<i64>, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let limit = limitDays.unwrap_or(365);
    let mut stmt = conn
        .prepare("SELECT * FROM networth_history ORDER BY snapshot_date ASC LIMIT ?1")
        .map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[&limit]).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn record_net_worth_snapshot(snapshotDate: Option<String>, state: State<'_, AppState>) -> CmdResult<Value> {
    let summary = get_net_worth_summary(state.clone())?;
    let date = snapshotDate.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let accounts = summary.get("total_accounts").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let holdings = summary.get("total_investments").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let debts = summary.get("total_debts").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let net = summary.get("net_worth").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO networth_history (snapshot_date, total_accounts, total_investments, total_debts, net_worth) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![date, accounts, holdings, debts, net],
    ).map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

#[tauri::command]
pub fn check_and_snapshot_net_worth(state: State<'_, AppState>) -> CmdResult<()> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM networth_history WHERE snapshot_date = ?1",
        params![today],
        |r| r.get(0),
    ).unwrap_or(0);
    drop(conn);

    if count == 0 {
        let _ = record_net_worth_snapshot(Some(today), state);
    }
    Ok(())
}

#[tauri::command]
pub fn get_income_expense_trend(_filter: Option<Value>, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m', txn_date) as month,
                COALESCE(SUM(CASE WHEN type = 'income' THEN amount ELSE 0 END), 0) as income,
                COALESCE(SUM(CASE WHEN type = 'expense' THEN amount ELSE 0 END), 0) as expense
         FROM transactions
         GROUP BY strftime('%Y-%m', txn_date)
         ORDER BY month ASC LIMIT 12"
    ).map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_category_spending_report(_filter: Option<Value>, state: State<'_, AppState>) -> CmdResult<Vec<Value>> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.color, c.icon, COALESCE(SUM(t.amount), 0.0) as total_spent
         FROM categories c
         JOIN transactions t ON t.category_id = c.id
         WHERE t.type = 'expense'
         GROUP BY c.id
         ORDER BY total_spent DESC"
    ).map_err(|e| e.to_string())?;
    rows_to_json(&mut stmt, &[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_investment_performance_report(state: State<'_, AppState>) -> CmdResult<Value> {
    get_portfolio_summary(state)
}

// --- NOTIFICATIONS & BACKUPS ---

#[tauri::command]
pub fn check_and_send_due_reminders() -> CmdResult<()> {
    Ok(())
}

#[tauri::command]
pub fn send_os_desktop_notification(title: String, body: String) -> CmdResult<()> {
    println!("[Notification] {}: {}", title, body);
    Ok(())
}

#[tauri::command]
pub fn update_notification_settings(_payload: Value) -> CmdResult<()> {
    Ok(())
}

#[tauri::command]
pub fn check_and_run_daily_backup() -> CmdResult<()> {
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn create_backup(customDestFolder: Option<String>) -> CmdResult<Value> {
    let db_path = crate::db::get_db_path();
    let dest_folder = customDestFolder.map(std::path::PathBuf::from).unwrap_or_else(|| {
        dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    });
    let backup_name = format!("lyncost_backup_{}.db", chrono::Local::now().format("%Y%m%d_%H%M%S"));
    let backup_path = dest_folder.join(&backup_name);
    std::fs::copy(&db_path, &backup_path).map_err(|e| e.to_string())?;
    Ok(json!({
        "success": true,
        "filePath": backup_path.to_string_lossy()
    }))
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn restore_backup(backupFilePath: String, state: State<'_, AppState>) -> CmdResult<Value> {
    let db_path = crate::db::get_db_path();
    std::fs::copy(&backupFilePath, &db_path).map_err(|e| e.to_string())?;
    let mut conn = state.db.lock().map_err(|e| e.to_string())?;
    *conn = crate::db::init_db().map_err(|e| e.to_string())?;
    Ok(json!({ "success": true }))
}

#[tauri::command]
pub fn get_backups_list() -> CmdResult<Vec<Value>> {
    Ok(vec![])
}
