//! End-to-end tests through Tauri's IPC layer, using the exact argument names
//! and payload shapes the shipped frontend (`dist/`) sends. These catch the
//! classic Tauri bug of a Rust parameter name not matching the JS call.

use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewWindow, WebviewWindowBuilder};

use crate::db::AppState;
use crate::util::today_str;

struct Harness {
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    dir: tempfile::TempDir,
}

impl Harness {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let app = mock_builder()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_notification::init())
            .manage(AppState::new(dir.path().to_path_buf()))
            .invoke_handler(crate::handlers())
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default()).build().unwrap();
        let h = Self { _app: app, webview, dir };
        // Keep tests from popping real desktop notifications.
        h.ok("update_notification_settings", json!({"payload": {"notify_os": false, "notify_advance_days": 3}}));
        h
    }

    fn call(&self, cmd: &str, args: Value) -> Result<Value, String> {
        let res = get_ipc_response(
            &self.webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        );
        match res {
            Ok(body) => Ok(body.deserialize::<Value>().unwrap()),
            Err(e) => Err(e.as_str().map(String::from).unwrap_or_else(|| e.to_string())),
        }
    }

    #[track_caller]
    fn ok(&self, cmd: &str, args: Value) -> Value {
        self.call(cmd, args).unwrap_or_else(|e| panic!("{cmd} failed: {e}"))
    }

    #[track_caller]
    fn err(&self, cmd: &str, args: Value) -> String {
        match self.call(cmd, args) {
            Ok(v) => panic!("{cmd} should have failed but returned {v}"),
            Err(e) => e,
        }
    }

    fn balance(&self, id: i64) -> f64 {
        let accounts = self.ok("get_accounts", json!({"includeArchived": true}));
        accounts.as_array().unwrap().iter().find(|a| a["id"] == id).unwrap()["current_balance"].as_f64().unwrap()
    }

    fn account(&self, name: &str, kind: &str, currency: &str, opening: f64) -> i64 {
        let acc = self.ok(
            "create_account",
            json!({"payload": {"name": name, "type": kind, "currency": currency, "opening_balance": opening, "color": "#10b981"}}),
        );
        acc["id"].as_i64().unwrap()
    }

    fn category(&self, name: &str) -> i64 {
        let cats = self.ok("get_categories", json!({}));
        cats.as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()["id"].as_i64().unwrap()
    }
}

fn days_from_today(days: i64) -> String {
    (chrono::Local::now().date_naive() + chrono::Duration::days(days)).format("%Y-%m-%d").to_string()
}

#[test]
fn startup_settings_and_pin() {
    let h = Harness::new();
    assert_eq!(h.ok("get_startup_status", json!({})), Value::Null);
    let s = h.ok("get_app_settings", json!({}));
    assert_eq!(s["base_currency"], "INR");
    assert_eq!(s["has_pin"], false);
    assert_eq!(s["notify_os"], false);

    assert!(h.err("set_initial_pin", json!({"pin": "12ab"})).contains("6 digits"));
    h.ok("set_initial_pin", json!({"pin": "123456"}));
    assert!(h.err("set_initial_pin", json!({"pin": "654321"})).contains("already configured"));
    assert_eq!(h.ok("get_app_settings", json!({}))["has_pin"], true);

    let bad = h.ok("verify_pin", json!({"pin": "000000"}));
    assert_eq!(bad["valid"], false);
    assert_eq!(bad["remaining_attempts"], 4);
    assert_eq!(h.ok("verify_pin", json!({"pin": "123456"}))["valid"], true);

    assert!(h.err("change_pin", json!({"payload": {"current_pin": "999999", "new_pin": "111111"}})).contains("incorrect"));
    h.ok("change_pin", json!({"payload": {"current_pin": "123456", "new_pin": "111111"}}));
    assert_eq!(h.ok("verify_pin", json!({"pin": "111111"}))["valid"], true);

    h.ok("set_app_theme", json!({"theme": "light"}));
    assert!(h.err("set_app_theme", json!({"theme": "neon"})).contains("Theme"));
    assert_eq!(h.ok("get_app_settings", json!({}))["theme"], "light");
}

#[test]
fn accounts_transactions_and_ledger() {
    let h = Harness::new();
    let bank = h.account("HDFC", "bank", "inr", 10_000.0);
    let cash = h.account("Wallet", "cash", "INR", 0.0);
    assert_eq!(h.balance(bank), 10_000.0);
    let food = h.category("Food");

    // Expense with tags and a linked warranty, exactly as the transaction modal sends it.
    let txn = h.ok(
        "create_transaction",
        json!({"payload": {
            "account_id": bank, "type": "expense", "category_id": food, "transfer_to_account_id": null,
            "amount": 1500, "payment_type": "upi", "txn_date": today_str(), "note": "Headphones",
            "is_confirmed": true, "tags": ["personal", "gadgets"],
            "warranty": {"item_name": "Headphones", "purchase_date": today_str(), "expires_on": days_from_today(365), "notes": null}
        }}),
    );
    assert_eq!(txn["account_name"], "HDFC");
    assert_eq!(txn["category_name"], "Food");
    assert_eq!(txn["is_confirmed"], 1);
    assert_eq!(txn["tags"], json!(["gadgets", "personal"]));
    assert_eq!(h.balance(bank), 8_500.0);
    let txn_id = txn["id"].as_i64().unwrap();
    let warranties = h.ok("get_warranties", json!({}));
    assert_eq!(warranties[0]["transaction_id"], txn_id);
    assert_eq!(warranties[0]["is_expired"], false);

    // Transfer moves money between accounts.
    h.ok(
        "create_transaction",
        json!({"payload": {"account_id": bank, "type": "transfer", "category_id": null, "transfer_to_account_id": cash,
            "amount": 1000, "payment_type": "bank_transfer", "txn_date": today_str(), "note": null, "is_confirmed": true, "tags": [], "warranty": null}}),
    );
    assert_eq!(h.balance(bank), 7_500.0);
    assert_eq!(h.balance(cash), 1_000.0);
    assert!(h
        .err("create_transaction", json!({"payload": {"account_id": bank, "type": "transfer", "transfer_to_account_id": bank, "amount": 5}}))
        .contains("cannot be the same"));
    assert!(h.err("create_transaction", json!({"payload": {"account_id": bank, "type": "expense", "amount": 0}})).contains("greater than zero"));

    // Editing re-posts the ledger; deleting reverses it.
    h.ok(
        "update_transaction",
        json!({"id": txn_id, "payload": {"account_id": bank, "type": "expense", "category_id": food, "amount": 2000,
            "payment_type": "card", "txn_date": today_str(), "note": "Headphones", "is_confirmed": true, "tags": ["personal"], "warranty": null}}),
    );
    assert_eq!(h.balance(bank), 7_000.0);
    let ledger = h.ok("get_account_ledger", json!({"accountId": bank}));
    assert_eq!(ledger[0]["balance_after"], 7_000.0);
    h.ok("delete_transaction", json!({"id": txn_id}));
    assert_eq!(h.balance(bank), 9_000.0);
    assert_eq!(h.ok("get_warranties", json!({}))[0]["transaction_id"], Value::Null);

    // Filters used by the Transactions page.
    let all = h.ok("get_transactions", json!({"filter": {}}));
    assert_eq!(all.as_array().unwrap().len(), 1);
    let filtered = h.ok("get_transactions", json!({"filter": {"txn_type": "expense"}}));
    assert_eq!(filtered.as_array().unwrap().len(), 0);
    let by_account = h.ok("get_transactions", json!({"filter": {"account_id": cash, "is_confirmed": true}}));
    assert_eq!(by_account.as_array().unwrap().len(), 1);

    // Accounts with history can only be archived.
    assert!(h.err("delete_account", json!({"id": bank})).contains("archive"));
    h.ok("archive_account", json!({"id": bank, "archive": true}));
    assert_eq!(h.ok("get_accounts", json!({"includeArchived": false})).as_array().unwrap().len(), 1);
    let empty = h.account("Spare", "other", "INR", 0.0);
    h.ok("delete_account", json!({"id": empty}));
}

#[test]
fn categories_budgets_and_month_summary() {
    let h = Harness::new();
    let bank = h.account("Bank", "bank", "INR", 50_000.0);
    let cat = h.ok("create_category", json!({"payload": {"name": "Coffee", "kind": "expense", "parent_id": null, "icon": "coffee", "color": "#f97316"}}));
    let coffee = cat["id"].as_i64().unwrap();
    assert!(h.err("create_category", json!({"payload": {"name": "", "kind": "expense"}})).contains("empty"));

    let budget = h.ok(
        "create_budget",
        json!({"payload": {"name": "Treats", "amount": 1000, "period": "monthly", "start_date": format!("{}-01", &today_str()[..7]),
            "end_date": null, "rollover": false, "category_ids": [coffee]}}),
    );
    assert_eq!(budget["spent"], 0.0);
    assert_eq!(budget["rollover"], false, "rollover must be a bool for the UI");
    assert!(h.err("create_budget", json!({"payload": {"name": "X", "amount": 10, "period": "monthly", "category_ids": []}})).contains("category"));

    h.ok("create_transaction", json!({"payload": {"account_id": bank, "type": "expense", "category_id": coffee, "amount": 250, "txn_date": today_str(), "is_confirmed": true, "tags": []}}));
    h.ok("create_transaction", json!({"payload": {"account_id": bank, "type": "expense", "category_id": coffee, "amount": 100, "txn_date": today_str(), "is_confirmed": false, "tags": []}}));
    h.ok("create_transaction", json!({"payload": {"account_id": bank, "type": "income", "category_id": h.category("Salary"), "amount": 5000, "txn_date": today_str(), "is_confirmed": true, "tags": []}}));

    let budgets = h.ok("get_budgets", json!({}));
    assert_eq!(budgets[0]["spent"], 250.0, "pending transactions are excluded from budgets");
    assert_eq!(budgets[0]["remaining"], 750.0);
    assert_eq!(budgets[0]["category_names"], json!(["Coffee"]));

    let summary = h.ok("get_month_summary", json!({"month": null}));
    assert_eq!(summary["total_income"], 5000.0);
    assert_eq!(summary["total_expense"], 250.0);
    assert_eq!(summary["net_cashflow"], 4750.0);
    assert_eq!(summary["pending_count"], 1);
    assert_eq!(summary["pending_expense_total"], 100.0);
    // Pending transactions still move the balance.
    assert_eq!(h.balance(bank), 54_650.0);

    assert!(h.err("delete_category", json!({"id": coffee})).contains("transaction"));
    let id = budgets[0]["id"].as_i64().unwrap();
    h.ok(
        "update_budget",
        json!({"id": id, "payload": {"name": "Treats", "amount": 2000, "period": "weekly", "start_date": today_str(), "end_date": null, "rollover": true, "is_active": 1, "category_ids": [coffee]}}),
    );
    h.ok("delete_budget", json!({"id": id}));
    assert_eq!(h.ok("get_budgets", json!({})).as_array().unwrap().len(), 0);
}

#[test]
fn bills_pay_and_undo() {
    let h = Harness::new();
    let bank = h.account("Bank", "bank", "INR", 5_000.0);
    let bill = h.ok(
        "create_bill",
        json!({"payload": {"name": "Internet", "amount": 999, "due_date": days_from_today(-2), "account_id": bank, "category_id": h.category("Bills & Utilities"), "recurrence": "monthly"}}),
    );
    assert_eq!(bill["is_overdue"], true);
    let reminders = h.ok("check_and_send_due_reminders", json!({}));
    assert_eq!(reminders[0]["status"], "overdue");
    assert_eq!(reminders[0]["days_until_due"], -2);

    let id = bill["id"].as_i64().unwrap();
    h.ok("mark_bill_paid", json!({"id": id}));
    assert_eq!(h.balance(bank), 4_001.0);
    let bills = h.ok("get_bills", json!({}));
    assert_eq!(bills.as_array().unwrap().len(), 2, "next monthly occurrence is scheduled");

    h.ok("unmark_bill_paid", json!({"id": id}));
    assert_eq!(h.balance(bank), 5_000.0);
    let bills = h.ok("get_bills", json!({}));
    assert_eq!(bills.as_array().unwrap().len(), 1);
    assert_eq!(bills[0]["is_paid"], 0);

    h.ok("update_bill", json!({"id": id, "payload": {"name": "Fiber", "amount": 1099, "due_date": days_from_today(5), "account_id": bank, "category_id": null, "is_paid": false, "recurrence": "none"}}));
    h.ok("delete_bill", json!({"id": id}));
}

#[test]
fn debts_goals_and_investments() {
    let h = Harness::new();
    let bank = h.account("Bank", "bank", "INR", 1_000.0);
    let debt = h.ok(
        "create_debt",
        json!({"payload": {"name": "Car Loan", "principal": 100000, "initial_draw": 60000, "current_balance": 60000, "debt_type": "auto_loan",
            "interest_rate": 9.5, "due_date": null, "min_payment": 5000, "deposit_account_id": bank, "notes": null}}),
    );
    let debt_id = debt["id"].as_i64().unwrap();
    assert_eq!(debt["current_balance"], 60_000.0);
    assert_eq!(h.balance(bank), 61_000.0);

    h.ok("record_debt_payment", json!({"payload": {"debt_id": debt_id, "amount": 10000, "txn_date": today_str(), "account_id": bank, "notes": null}}));
    h.ok("draw_debt_funds", json!({"payload": {"debt_id": debt_id, "amount": 5000, "txn_date": today_str(), "account_id": null, "notes": "top-up"}}));
    h.ok("spend_from_debt", json!({"payload": {"debt_id": debt_id, "amount": 500, "txn_date": today_str(), "category_id": h.category("Shopping"), "notes": null}}));
    let debts = h.ok("get_debts", json!({}));
    assert_eq!(debts[0]["current_balance"], 55_500.0);
    assert_eq!(debts[0]["total_paid"], 10_000.0);
    assert_eq!(debts[0]["total_borrowed"], 65_500.0);
    assert_eq!(h.balance(bank), 51_000.0);
    assert_eq!(h.ok("get_debt_transactions", json!({"debtId": debt_id})).as_array().unwrap().len(), 4);
    h.ok("update_debt", json!({"id": debt_id, "payload": {"name": "Car Loan", "principal": 100000, "current_balance": 55500, "total_borrowed": 65500,
        "total_paid": 10000, "debt_type": "auto_loan", "interest_rate": 9.5, "due_date": null, "min_payment": 5000, "account_id": bank, "notes": "", "is_active": 0}}));
    assert_eq!(h.ok("get_debts", json!({}))[0]["is_active"], 0);
    let no_min = h.ok("create_debt", json!({"payload": {"name": "Friend", "principal": 500, "initial_draw": 500, "debt_type": "friend_family", "min_payment": 0}}));
    assert_eq!(no_min["min_payment"], Value::Null, "unset minimum must be null for the UI");
    h.ok("delete_debt", json!({"id": no_min["id"]}));

    let goal = h.ok("create_goal", json!({"payload": {"name": "Trip", "target_amount": 1000, "current_amount": 200, "target_date": null, "account_id": null, "color": "#9333ea", "icon": "target", "note": null}}));
    let goal_id = goal["id"].as_i64().unwrap();
    assert_eq!(goal["percentage"], 20.0);
    h.ok("contribute_to_goal", json!({"payload": {"goal_id": goal_id, "amount": 900, "note": "Deposit"}}));
    let goals = h.ok("get_goals", json!({}));
    assert_eq!(goals[0]["is_reached"], 1);
    assert_eq!(goals[0]["remaining_amount"], 0.0);
    h.ok("contribute_to_goal", json!({"payload": {"goal_id": goal_id, "amount": -5000, "note": "Withdrawal"}}));
    assert_eq!(h.ok("get_goals", json!({}))[0]["current_amount"], 0.0);

    // Holdings must sit in an investment account; USD values convert to base INR.
    assert!(h
        .err("create_holding", json!({"payload": {"account_id": bank, "asset_type": "stock", "symbol": "AAPL", "quantity": 1, "avg_buy_price": 1, "currency": "USD", "last_price": 1}}))
        .contains("investment"));
    let broker = h.account("Broker", "investment", "USD", 0.0);
    let holding = h.ok(
        "create_holding",
        json!({"payload": {"account_id": broker, "asset_type": "stock", "symbol": "aapl", "name": "Apple", "quantity": 10, "avg_buy_price": 100, "currency": "USD", "last_price": 100, "notes": null}}),
    );
    let hid = holding.as_i64().unwrap();
    h.ok("update_holding_price", json!({"payload": {"holding_id": hid, "price": 120}}));
    h.ok("bulk_update_holding_prices", json!({"updates": [{"holding_id": hid, "price": 150}]}));
    let holdings = h.ok("get_holdings", json!({}));
    assert_eq!(holdings[0]["symbol"], "AAPL");
    assert_eq!(holdings[0]["unrealized_pnl"], 500.0);
    assert_eq!(holdings[0]["base_current_value"], 1500.0 * 88.0);
    assert_eq!(h.ok("get_holding_price_history", json!({"holdingId": hid})).as_array().unwrap().len(), 3);
    let summary = h.ok("get_portfolio_summary", json!({}));
    assert_eq!(summary["holdings_count"], 1);
    assert_eq!(summary["total_unrealized_pnl_percent"], 50.0);
    let report = h.ok("get_investment_performance_report", json!({}));
    assert_eq!(report["holdings"][0]["pnl_percent"], 50.0);

    let nw = h.ok("get_net_worth_summary", json!({}));
    assert_eq!(nw["total_investments"], 132_000.0);
    assert_eq!(nw["total_debts"], 0.0, "inactive debts are excluded");
    assert_eq!(nw["net_worth"], 51_000.0 + 132_000.0);
    h.ok("update_holding", json!({"id": hid, "payload": {"asset_type": "stock", "symbol": "AAPL", "name": null, "quantity": 5, "avg_buy_price": 100, "currency": "USD", "notes": null}}));
    h.ok("archive_holding", json!({"id": hid, "isArchived": true}));
    h.ok("delete_holding", json!({"id": hid}));
}

#[test]
fn recurring_csv_reports_and_payment_methods() {
    let h = Harness::new();
    let bank = h.account("Bank", "bank", "INR", 0.0);
    let rule = h.ok(
        "create_recurring_rule",
        json!({"payload": {"name": "Salary", "account_id": bank, "type": "income", "category_id": h.category("Salary"), "transfer_to_account_id": null,
            "amount": 1000, "payment_type": "bank_transfer", "frequency": "weekly", "next_due_date": days_from_today(-14), "note": null}}),
    );
    assert_eq!(h.ok("process_recurring_rules", json!({})), 3);
    assert_eq!(h.ok("process_recurring_rules", json!({})), 0, "already caught up");
    assert_eq!(h.balance(bank), 3_000.0);
    let rules = h.ok("get_recurring_rules", json!({}));
    assert_eq!(rules[0]["next_due_date"], days_from_today(7));
    h.ok("update_recurring_rule", json!({"id": rule["id"], "payload": {"name": "Salary", "account_id": bank, "type": "income", "category_id": null,
        "amount": 2000, "payment_type": "upi", "frequency": "monthly", "next_due_date": days_from_today(30), "is_active": false, "note": null}}));
    h.ok("delete_recurring_rule", json!({"id": rule["id"]}));

    let imported = h.ok(
        "import_csv_transactions",
        json!({"payload": {"target_account_id": bank, "default_category_id": null, "rows": [
            {"date": today_str(), "amount": 120.5, "txn_type": "expense", "category_name": "Groceries", "note": "Market", "payment_type": "cash"},
            {"date": today_str(), "amount": 80, "txn_type": "expense", "category_name": "groceries", "note": null, "payment_type": "other"},
            {"date": today_str(), "amount": -5, "txn_type": "expense", "category_name": null, "note": null, "payment_type": "other"},
            {"date": today_str(), "amount": 5, "txn_type": "refund", "category_name": null, "note": null, "payment_type": "other"}
        ]}}),
    );
    assert_eq!(imported["imported_count"], 2);
    assert_eq!(imported["failed_rows"].as_array().unwrap().len(), 2);
    let groceries: Vec<_> = h.ok("get_categories", json!({})).as_array().unwrap().iter().filter(|c| c["name"] == "Groceries").cloned().collect();
    assert_eq!(groceries.len(), 1, "CSV categories are matched case-insensitively");

    let range = json!({"filter": {"start_date": format!("{}-01", &today_str()[..7]), "end_date": today_str(), "account_id": null}});
    let spending = h.ok("get_category_spending_report", range.clone());
    assert_eq!(spending[0]["category_name"], "Groceries");
    assert_eq!(spending[0]["total_spent"], 200.5);
    assert_eq!(spending[0]["percentage"], 100.0);
    assert_eq!(spending[0]["kind"], "expense");
    let trend = h.ok("get_income_expense_trend", json!({"filter": {"start_date": null, "end_date": null, "account_id": null}}));
    let last = trend.as_array().unwrap().last().unwrap();
    assert!(last["period_label"].as_str().unwrap().len() >= 8);
    assert!(last["total_income"].as_f64().unwrap() > 0.0);

    assert_eq!(h.ok("check_and_snapshot_net_worth", json!({})), true);
    assert_eq!(h.ok("check_and_snapshot_net_worth", json!({})), false);
    h.ok("record_net_worth_snapshot", json!({"snapshotDate": null}));
    assert_eq!(h.ok("get_net_worth_history", json!({"limitDays": 30})).as_array().unwrap().len(), 1);

    let methods = h.ok("apply_regional_payment_presets", json!({"currency": "INR"}));
    assert!(methods.as_array().unwrap().iter().any(|m| m["name"] == "UPI (GPay / PhonePe)"));
    let again = h.ok("apply_regional_payment_presets", json!({"currency": "INR"}));
    assert_eq!(methods.as_array().unwrap().len(), again.as_array().unwrap().len(), "presets are not duplicated");
    let pm = h.ok("create_payment_method", json!({"payload": {"name": "Crypto Card", "type_key": "crypto", "icon": "Wallet", "color": "#8b5cf6", "is_default": true}}));
    h.ok("update_payment_method", json!({"id": pm["id"], "payload": {"name": "Crypto Card", "type_key": "crypto", "icon": "Wallet", "color": "#000000", "is_default": true, "is_active": 1}}));
    let defaults: Vec<_> = h.ok("get_payment_methods", json!({})).as_array().unwrap().iter().filter(|m| m["is_default"] == 1).cloned().collect();
    assert_eq!(defaults.len(), 1);
    h.ok("delete_payment_method", json!({"id": pm["id"]}));

    h.ok("set_exchange_rate", json!({"payload": {"from_currency": "usd", "to_currency": "INR", "rate": 90}}));
    let rates = h.ok("get_exchange_rates", json!({}));
    assert!(rates.as_array().unwrap().iter().any(|r| r["from_currency"] == "USD" && r["rate"] == 90.0));
    assert!(h.err("set_exchange_rate", json!({"payload": {"from_currency": "INR", "to_currency": "INR", "rate": 1}})).contains("different"));
    h.ok("delete_exchange_rate", json!({"id": rates[0]["id"]}));

    h.ok("set_base_currency", json!({"newCurrency": "usd"}));
    let txns = h.ok("get_transactions", json!({"filter": {}}));
    let inr_amount = txns[0]["amount"].as_f64().unwrap();
    assert!((txns[0]["base_amount"].as_f64().unwrap() - (inr_amount / 90.0 * 100.0).round() / 100.0).abs() < 0.01);
}

#[test]
fn shopping_backup_restore_export_and_wipe() {
    let h = Harness::new();
    let item = h.ok("create_shopping_item", json!({"payload": {"name": "Milk", "note": null}}));
    h.ok("update_shopping_item", json!({"id": item["id"], "payload": {"name": "Oat milk", "note": "2L"}}));
    assert_eq!(h.ok("toggle_shopping_item", json!({"id": item["id"]}))["is_checked"], 1);
    h.ok("clear_completed_shopping_items", json!({}));
    assert_eq!(h.ok("get_shopping_items", json!({})).as_array().unwrap().len(), 0);

    let bank = h.account("Bank", "bank", "INR", 777.0);
    let first = h.ok("check_and_run_daily_backup", json!({}));
    assert!(first.as_str().unwrap().ends_with(".db"));
    assert_eq!(h.ok("check_and_run_daily_backup", json!({})), Value::Null, "only one automatic backup per day");
    let manual = h.ok("create_backup", json!({"customDestFolder": null})).as_str().unwrap().to_string();
    assert!(h.err("create_backup", json!({"customDestFolder": "relative/dir"})).contains("absolute"));
    let backups = h.ok("get_backups_list", json!({}));
    assert!(!backups.as_array().unwrap().is_empty());
    assert!(backups[0]["size_bytes"].as_u64().unwrap() > 0);

    // Change data, then restore the backup taken before the change.
    h.ok("create_transaction", json!({"payload": {"account_id": bank, "type": "expense", "amount": 77, "txn_date": today_str(), "is_confirmed": true, "tags": []}}));
    assert_eq!(h.balance(bank), 700.0);
    h.ok("restore_backup", json!({"backupFilePath": manual}));
    assert_eq!(h.balance(bank), 777.0);
    let names: Vec<String> = h.ok("get_backups_list", json!({})).as_array().unwrap().iter().map(|b| b["filename"].as_str().unwrap().to_string()).collect();
    assert!(names.iter().any(|n| n.starts_with("lyncost_pre_restore_")), "safety copy is listed");

    let junk = h.dir.path().join("not-a-db.db");
    std::fs::write(&junk, b"hello").unwrap();
    assert!(h.err("restore_backup", json!({"backupFilePath": junk.to_string_lossy()})).contains("not a valid"));
    assert_eq!(h.balance(bank), 777.0, "failed restore leaves data untouched");

    assert!(h.err("save_export_file", json!({"fileName": "../evil.csv", "contents": "x"})).contains("Invalid"));

    h.ok("set_initial_pin", json!({"pin": "123456"}));
    h.ok("wipe_all_data", json!({}));
    let safety: Vec<String> = h.ok("get_backups_list", json!({})).as_array().unwrap().iter()
        .map(|b| b["filename"].as_str().unwrap().to_string()).collect();
    assert!(safety.iter().any(|n| n.starts_with("lyncost_pre_wipe_")), "wipe keeps a safety copy");
    let s = h.ok("get_app_settings", json!({}));
    assert_eq!(s["has_pin"], false);
    assert_eq!(h.ok("get_accounts", json!({"includeArchived": true})).as_array().unwrap().len(), 0);
    assert!(!h.ok("get_categories", json!({})).as_array().unwrap().is_empty(), "defaults are re-seeded");
}
