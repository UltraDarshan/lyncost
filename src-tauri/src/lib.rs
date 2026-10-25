//! Lyncost: offline, privacy-first personal finance tracker.

mod commands;
mod db;
mod fx;
mod ledger;
mod license;
mod security;
mod util;

#[cfg(test)]
mod contract_tests;

use tauri::{Manager, Runtime};

use commands::*;
use db::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    linux_webkit_workarounds();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init());
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(license_gate(handlers()))
        .run(tauri::generate_context!())
        .expect("error while running Lyncost");
}

/// Refuses record-changing commands once the Windows trial has ended without a license.
/// Enforced here, in the backend, so editing the page or its storage cannot get around it.
fn license_gate<R: Runtime>(
    inner: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if license::gate_enabled() && license::blocks(invoke.message.command()) {
            let app = invoke.message.webview().app_handle().clone();
            let state = app.state::<AppState>();
            let unlocked = state.conn().and_then(|c| license::current_status(&c)).map(|s| s.unlocked()).unwrap_or(false);
            if !unlocked {
                invoke.resolver.reject(license::LOCKED_MESSAGE);
                return true;
            }
        }
        inner(invoke)
    }
}

/// Every command the frontend can invoke. Generic so tests can use the mock runtime.
fn handlers<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        settings::get_startup_status,
        settings::get_app_settings,
        settings::set_app_theme,
        settings::set_base_currency,
        settings::set_initial_pin,
        settings::verify_pin,
        settings::change_pin,
        settings::update_notification_settings,
        settings::wipe_all_data,
        system::get_trial_status,
        system::get_license_status,
        system::verify_license_key,
        system::qa_set_clock_offset,
        system::check_app_update,
        system::install_app_update,
        system::restart_application,
        accounts::get_accounts,
        accounts::create_account,
        accounts::update_account,
        accounts::delete_account,
        accounts::archive_account,
        accounts::get_account_ledger,
        transactions::get_transactions,
        transactions::create_transaction,
        transactions::update_transaction,
        transactions::delete_transaction,
        transactions::get_tags,
        transactions::import_csv_transactions,
        categories::get_categories,
        categories::create_category,
        categories::update_category,
        categories::delete_category,
        budgets::get_budgets,
        budgets::create_budget,
        budgets::update_budget,
        budgets::delete_budget,
        budgets::get_month_summary,
        goals::get_goals,
        goals::create_goal,
        goals::update_goal,
        goals::delete_goal,
        goals::contribute_to_goal,
        debts::get_debts,
        debts::create_debt,
        debts::update_debt,
        debts::delete_debt,
        debts::spend_from_debt,
        debts::draw_debt_funds,
        debts::record_debt_payment,
        debts::get_debt_transactions,
        investments::get_holdings,
        investments::create_holding,
        investments::update_holding,
        investments::delete_holding,
        investments::archive_holding,
        investments::update_holding_price,
        investments::bulk_update_holding_prices,
        investments::get_holding_price_history,
        investments::get_portfolio_summary,
        investments::get_investment_performance_report,
        bills::get_bills,
        bills::create_bill,
        bills::update_bill,
        bills::delete_bill,
        bills::mark_bill_paid,
        bills::unmark_bill_paid,
        warranties::get_warranties,
        warranties::create_warranty,
        warranties::update_warranty,
        warranties::delete_warranty,
        shopping::get_shopping_items,
        shopping::create_shopping_item,
        shopping::update_shopping_item,
        shopping::delete_shopping_item,
        shopping::toggle_shopping_item,
        shopping::clear_completed_shopping_items,
        recurring::get_recurring_rules,
        recurring::create_recurring_rule,
        recurring::update_recurring_rule,
        recurring::delete_recurring_rule,
        recurring::process_recurring_rules,
        currency::get_exchange_rates,
        currency::set_exchange_rate,
        currency::delete_exchange_rate,
        payment_methods::get_payment_methods,
        payment_methods::create_payment_method,
        payment_methods::update_payment_method,
        payment_methods::delete_payment_method,
        payment_methods::apply_regional_payment_presets,
        reports::get_net_worth_summary,
        reports::get_net_worth_history,
        reports::record_net_worth_snapshot,
        reports::check_and_snapshot_net_worth,
        reports::get_income_expense_trend,
        reports::get_category_spending_report,
        reminders::check_and_send_due_reminders,
        reminders::send_os_desktop_notification,
        backup::check_and_run_daily_backup,
        backup::create_backup,
        backup::restore_backup,
        backup::get_backups_list,
        backup::save_export_file,
        ]
}

/// WebKitGTK's DMA-BUF renderer shows a blank window with the proprietary
/// NVIDIA driver. Respect any value the user already set.
#[cfg(target_os = "linux")]
fn linux_webkit_workarounds() {
    let nvidia = std::path::Path::new("/proc/driver/nvidia").exists();
    if nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: called before any threads are spawned.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}
