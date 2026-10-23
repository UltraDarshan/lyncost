//! Recurring transaction rules, posted automatically when they come due.

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::transactions::{insert_txn, NewTxn};
use crate::db::AppState;
use crate::util::{add_months, fmt_date, opt_bool, opt_f64, opt_i64, opt_string, parse_date, round2, today, CmdResult, Ctx};

/// Safety valve: never back-fill more than this many occurrences of one rule at once.
const MAX_CATCH_UP: usize = 400;

#[derive(Serialize)]
pub struct RecurringRule {
    id: i64,
    name: String,
    account_id: i64,
    account_name: String,
    #[serde(rename = "type")]
    kind: String,
    category_id: Option<i64>,
    category_name: Option<String>,
    transfer_to_account_id: Option<i64>,
    transfer_to_account_name: Option<String>,
    amount: f64,
    payment_type: Option<String>,
    frequency: String,
    next_due_date: String,
    is_active: i64,
    note: Option<String>,
}

const SELECT_RULE: &str = "SELECT
        r.id, r.name, r.account_id, a.name, r.type,
        r.category_id, c.name, r.transfer_to_account_id, a_to.name,
        r.amount, r.payment_type, r.frequency, r.next_due_date, COALESCE(r.is_active, 1), r.note
     FROM recurring_rules r
     JOIN accounts a ON a.id = r.account_id
     LEFT JOIN accounts a_to ON a_to.id = r.transfer_to_account_id
     LEFT JOIN categories c ON c.id = r.category_id";

fn map_rule(r: &Row) -> rusqlite::Result<RecurringRule> {
    Ok(RecurringRule {
        id: r.get(0)?,
        name: r.get(1)?,
        account_id: r.get(2)?,
        account_name: r.get(3)?,
        kind: r.get(4)?,
        category_id: r.get(5)?,
        category_name: r.get(6)?,
        transfer_to_account_id: r.get(7)?,
        transfer_to_account_name: r.get(8)?,
        amount: r.get(9)?,
        payment_type: r.get(10)?,
        frequency: r.get(11)?,
        next_due_date: r.get(12)?,
        is_active: r.get(13)?,
        note: r.get(14)?,
    })
}

fn load_rule(conn: &Connection, id: i64) -> CmdResult<RecurringRule> {
    conn.query_row(&format!("{SELECT_RULE} WHERE r.id = ?1"), params![id], map_rule)
        .optional()
        .ctx("Failed to load recurring rule")?
        .ok_or_else(|| "Recurring rule not found".to_string())
}

#[tauri::command]
pub fn get_recurring_rules(state: State<'_, AppState>) -> CmdResult<Vec<RecurringRule>> {
    let conn = state.conn()?;
    let mut stmt = conn
        .prepare(&format!("{SELECT_RULE} ORDER BY r.next_due_date ASC"))
        .ctx("Failed to load recurring rules")?;
    let rows = stmt.query_map([], map_rule).ctx("Failed to load recurring rules")?;
    rows.collect::<Result<_, _>>().ctx("Failed to load recurring rules")
}

#[derive(Deserialize)]
pub struct RulePayload {
    #[serde(default, deserialize_with = "opt_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    account_id: Option<i64>,
    #[serde(rename = "type", default, deserialize_with = "opt_string")]
    kind: Option<String>,
    #[serde(default, deserialize_with = "opt_i64")]
    category_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_i64")]
    transfer_to_account_id: Option<i64>,
    #[serde(default, deserialize_with = "opt_f64")]
    amount: Option<f64>,
    #[serde(default, deserialize_with = "opt_string")]
    payment_type: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    frequency: Option<String>,
    #[serde(default, deserialize_with = "opt_string")]
    next_due_date: Option<String>,
    #[serde(default, deserialize_with = "opt_bool")]
    is_active: Option<bool>,
    #[serde(default, deserialize_with = "opt_string")]
    note: Option<String>,
}

struct ValidRule {
    name: String,
    account_id: i64,
    kind: String,
    category_id: Option<i64>,
    transfer_to: Option<i64>,
    amount: f64,
    frequency: String,
    next_due: NaiveDate,
}

fn validate(p: &RulePayload) -> CmdResult<ValidRule> {
    let name = p.name.clone().ok_or("Rule name cannot be empty")?;
    let account_id = p.account_id.ok_or("Please select an account")?;
    let kind = p.kind.clone().unwrap_or_else(|| "expense".into());
    if !matches!(kind.as_str(), "income" | "expense" | "transfer") {
        return Err(format!("Invalid transaction type: {kind}"));
    }
    let amount = round2(p.amount.unwrap_or(0.0));
    if amount <= 0.0 {
        return Err("Amount must be greater than zero".into());
    }
    let frequency = p.frequency.clone().unwrap_or_else(|| "monthly".into());
    if !matches!(frequency.as_str(), "daily" | "weekly" | "monthly" | "yearly") {
        return Err(format!("Invalid frequency: {frequency}"));
    }
    let next_due = p.next_due_date.as_deref().and_then(parse_date).ok_or("Please choose a valid next due date")?;
    let (category_id, transfer_to) = if kind == "transfer" {
        let to = p.transfer_to_account_id.ok_or("Please select a destination account for the transfer")?;
        if to == account_id {
            return Err("Source and destination accounts must be different".into());
        }
        (None, Some(to))
    } else {
        (p.category_id, None)
    };
    Ok(ValidRule { name, account_id, kind, category_id, transfer_to, amount, frequency, next_due })
}

#[tauri::command]
pub fn create_recurring_rule(payload: RulePayload, state: State<'_, AppState>) -> CmdResult<RecurringRule> {
    let v = validate(&payload)?;
    let conn = state.conn()?;
    conn.execute(
        "INSERT INTO recurring_rules (
            name, account_id, type, category_id, transfer_to_account_id,
            amount, payment_type, frequency, next_due_date, is_active, note, anchor_day
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11)",
        params![
            v.name, v.account_id, v.kind, v.category_id, v.transfer_to, v.amount, payload.payment_type,
            v.frequency, fmt_date(v.next_due), payload.note, v.next_due.day()
        ],
    )
    .ctx("Failed to create recurring rule")?;
    load_rule(&conn, conn.last_insert_rowid())
}

#[tauri::command]
pub fn update_recurring_rule(id: i64, payload: RulePayload, state: State<'_, AppState>) -> CmdResult<RecurringRule> {
    let v = validate(&payload)?;
    let conn = state.conn()?;
    conn.execute(
        "UPDATE recurring_rules SET
            name = ?1, account_id = ?2, type = ?3, category_id = ?4, transfer_to_account_id = ?5,
            amount = ?6, payment_type = ?7, frequency = ?8, next_due_date = ?9, is_active = ?10, note = ?11,
            anchor_day = ?12
         WHERE id = ?13",
        params![
            v.name, v.account_id, v.kind, v.category_id, v.transfer_to, v.amount, payload.payment_type,
            v.frequency, fmt_date(v.next_due), payload.is_active.unwrap_or(true), payload.note,
            v.next_due.day(), id
        ],
    )
    .ctx("Failed to update recurring rule")?;
    load_rule(&conn, id)
}

#[tauri::command]
pub fn delete_recurring_rule(id: i64, state: State<'_, AppState>) -> CmdResult<()> {
    let mut conn = state.conn()?;
    let tx = conn.transaction().ctx("Failed to delete recurring rule")?;
    // Past transactions stay; they just no longer point at the rule.
    tx.execute("UPDATE transactions SET recurring_rule_id = NULL WHERE recurring_rule_id = ?1", params![id])
        .ctx("Failed to delete recurring rule")?;
    tx.execute("DELETE FROM recurring_rules WHERE id = ?1", params![id]).ctx("Failed to delete recurring rule")?;
    tx.commit().ctx("Failed to delete recurring rule")
}

pub fn advance(date: NaiveDate, frequency: &str, anchor_day: Option<u32>) -> NaiveDate {
    match frequency {
        "daily" => date + Duration::days(1),
        "weekly" => date + Duration::days(7),
        "yearly" => add_months(date, 12, anchor_day),
        _ => add_months(date, 1, anchor_day),
    }
}

struct DueRule {
    id: i64,
    name: String,
    account_id: i64,
    kind: String,
    category_id: Option<i64>,
    transfer_to: Option<i64>,
    amount: f64,
    payment_type: Option<String>,
    frequency: String,
    next_due: String,
    note: Option<String>,
    anchor_day: Option<u32>,
}

/// Post every occurrence that has come due. Returns how many transactions were created.
pub fn process_due(conn: &mut Connection, on: NaiveDate) -> CmdResult<i64> {
    let due: Vec<DueRule> = conn
        .prepare(
            "SELECT id, name, account_id, type, category_id, transfer_to_account_id, amount, payment_type,
                    frequency, next_due_date, note, anchor_day
             FROM recurring_rules WHERE is_active = 1 AND next_due_date <= ?1",
        )
        .and_then(|mut s| {
            s.query_map(params![fmt_date(on)], |r| {
                Ok(DueRule {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    account_id: r.get(2)?,
                    kind: r.get(3)?,
                    category_id: r.get(4)?,
                    transfer_to: r.get(5)?,
                    amount: r.get(6)?,
                    payment_type: r.get(7)?,
                    frequency: r.get(8)?,
                    next_due: r.get(9)?,
                    note: r.get(10)?,
                    anchor_day: r.get(11)?,
                })
            })?
            .collect()
        })
        .ctx("Failed to load due recurring rules")?;

    let mut created = 0;
    let mut tx = conn.transaction().ctx("Failed to process recurring rules")?;
    for rule in due {
        let Some(mut date) = parse_date(&rule.next_due) else { continue };
        let note = rule.note.clone().unwrap_or_else(|| rule.name.clone());
        // A rule that can no longer post (e.g. its category was removed) is skipped, not fatal.
        let sp = tx.savepoint().ctx("Failed to process recurring rules")?;
        let mut posted = 0;
        let ok = (|| -> CmdResult<()> {
            while date <= on && posted < MAX_CATCH_UP {
                let date_str = fmt_date(date);
                insert_txn(
                    &sp,
                    &NewTxn {
                        account_id: rule.account_id,
                        kind: &rule.kind,
                        category_id: rule.category_id,
                        transfer_to_account_id: rule.transfer_to,
                        amount: rule.amount,
                        payment_type: rule.payment_type.as_deref(),
                        txn_date: &date_str,
                        note: Some(&note),
                        is_confirmed: true,
                        recurring_rule_id: Some(rule.id),
                    },
                )?;
                posted += 1;
                date = advance(date, &rule.frequency, rule.anchor_day);
            }
            sp.execute("UPDATE recurring_rules SET next_due_date = ?1 WHERE id = ?2", params![fmt_date(date), rule.id])
                .ctx("Failed to advance recurring rule")?;
            Ok(())
        })();
        match ok {
            Ok(()) => {
                sp.commit().ctx("Failed to process recurring rules")?;
                created += posted as i64;
            }
            Err(e) => eprintln!("[lyncost] skipped recurring rule {} ({}): {e}", rule.id, rule.name),
        }
    }
    tx.commit().ctx("Failed to process recurring rules")?;
    Ok(created)
}

#[tauri::command]
pub fn process_recurring_rules(state: State<'_, AppState>) -> CmdResult<i64> {
    process_due(&mut *state.conn()?, today())
}
