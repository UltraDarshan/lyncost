//! Bill reminders and native desktop notifications.

use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_notification::NotificationExt;

use crate::db::AppState;
use crate::util::{parse_date, today, today_str, CmdResult, Ctx};

#[derive(Serialize)]
pub struct BillReminder {
    bill_id: i64,
    bill_name: String,
    amount: f64,
    due_date: String,
    account_name: Option<String>,
    /// "overdue", "due_today" or "upcoming"
    status: &'static str,
    days_until_due: i64,
}

fn notify<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) -> bool {
    app.notification().builder().title(title).body(body).show().is_ok()
}

fn reminder_line(r: &BillReminder) -> String {
    let when = match r.days_until_due {
        d if d < 0 => format!("is OVERDUE by {} day(s)!", -d),
        0 => "is due TODAY!".to_string(),
        1 => "is due TOMORROW (1 day left)!".to_string(),
        d => format!("is due in {d} days!"),
    };
    format!("{} ({:.2}) {when}", r.bill_name, r.amount)
}

/// Unpaid bills that are overdue or due within the advance window.
/// Also shows one desktop notification per day when enabled.
#[tauri::command]
pub fn check_and_send_due_reminders<R: Runtime>(app: AppHandle<R>, state: State<'_, AppState>) -> CmdResult<Vec<BillReminder>> {
    let conn = state.conn()?;
    let (notify_os, advance_days, last_check): (bool, i64, Option<String>) = conn
        .query_row(
            "SELECT COALESCE(notify_os, 1), COALESCE(notify_advance_days, 1), last_notification_check_at
             FROM app_settings WHERE id = 1",
            [],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get(1)?, r.get(2)?)),
        )
        .ctx("Failed to read notification settings")?;

    let today = today();
    let mut stmt = conn
        .prepare(
            "SELECT b.id, b.name, b.amount, b.due_date, a.name
             FROM bills b LEFT JOIN accounts a ON b.account_id = a.id
             WHERE b.is_paid = 0 ORDER BY b.due_date ASC",
        )
        .ctx("Failed to load bills")?;
    let reminders: Vec<BillReminder> = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, String>(3)?, r.get(4)?))
        })
        .ctx("Failed to load bills")?
        .filter_map(Result::ok)
        .filter_map(|(bill_id, bill_name, amount, due_date, account_name)| {
            let days = (parse_date(&due_date)? - today).num_days();
            (days <= advance_days).then_some(BillReminder {
                bill_id,
                bill_name,
                amount,
                due_date,
                account_name,
                status: match days {
                    d if d < 0 => "overdue",
                    0 => "due_today",
                    _ => "upcoming",
                },
                days_until_due: days,
            })
        })
        .collect();
    drop(stmt);

    let today_s = today_str();
    let already_notified = last_check.as_deref().is_some_and(|d| d.starts_with(&today_s));
    if notify_os && !reminders.is_empty() && !already_notified {
        let body = reminders.iter().take(5).map(reminder_line).collect::<Vec<_>>().join("\n");
        let title = if reminders.len() == 1 {
            "Lyncost: 1 bill needs attention".to_string()
        } else {
            format!("Lyncost: {} bills need attention", reminders.len())
        };
        notify(&app, &title, &body);
        conn.execute("UPDATE app_settings SET last_notification_check_at = ?1 WHERE id = 1", params![today_s])
            .ctx("Failed to save notification state")?;
    }
    Ok(reminders)
}

#[tauri::command]
pub fn send_os_desktop_notification<R: Runtime>(app: AppHandle<R>, title: String, body: String) -> bool {
    notify(&app, &title, &body)
}
