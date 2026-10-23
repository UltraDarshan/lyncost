//! Small helpers shared by every command module: error plumbing, dates,
//! money rounding and lenient deserializers for frontend payloads.

use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Commands return plain strings as errors; the frontend shows them verbatim.
pub type CmdResult<T> = Result<T, String>;

/// Attach a human-readable prefix to any error, e.g. `.ctx("Failed to create bill")`.
pub trait Ctx<T> {
    fn ctx(self, msg: &str) -> CmdResult<T>;
}

impl<T, E: std::fmt::Display> Ctx<T> for Result<T, E> {
    fn ctx(self, msg: &str) -> CmdResult<T> {
        self.map_err(|e| format!("{msg}: {e}"))
    }
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn today_str() -> String {
    today().format("%Y-%m-%d").to_string()
}

pub fn now_str() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Parse the date part of `YYYY-MM-DD` or `YYYY-MM-DD HH:MM:SS` / ISO strings.
pub fn parse_date(s: &str) -> Option<NaiveDate> {
    let head = s.get(..10)?;
    NaiveDate::parse_from_str(head, "%Y-%m-%d").ok()
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

/// Move `date` forward by `months`, clamping to `anchor_day` (or the last day of a short month).
pub fn add_months(date: NaiveDate, months: i32, anchor_day: Option<u32>) -> NaiveDate {
    let total = date.year() * 12 + date.month0() as i32 + months;
    let (year, month) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let day = anchor_day.unwrap_or(date.day()).min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(date)
}

pub fn non_empty(s: Option<String>) -> Option<String> {
    s.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

// ---------------------------------------------------------------------------
// Lenient deserializers. Form state in the frontend sometimes sends "" for an
// unselected id, a numeric string for a number, or true/false for a flag that
// the database stores as 0/1. These accept all of those shapes.
// ---------------------------------------------------------------------------

fn value_to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

pub fn opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.as_ref().and_then(value_to_f64).map(|f| f as i64))
}

pub fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.as_ref().and_then(value_to_f64).filter(|f| f.is_finite()))
}

pub fn opt_bool<'de, D: Deserializer<'de>>(d: D) -> Result<Option<bool>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(match v {
        Some(Value::Bool(b)) => Some(b),
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0),
        Some(Value::String(s)) => match s.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

/// Treats "" and whitespace-only strings as missing.
pub fn opt_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(match v {
        Some(Value::String(s)) => non_empty(Some(s)),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

pub fn vec_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    let v = Option::<Vec<Value>>::deserialize(d)?;
    Ok(v.unwrap_or_default()
        .iter()
        .filter_map(value_to_f64)
        .map(|f| f as i64)
        .collect())
}

pub fn vec_string<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let v = Option::<Vec<Value>>::deserialize(d)?;
    Ok(v.unwrap_or_default()
        .into_iter()
        .filter_map(|x| match x {
            Value::String(s) => non_empty(Some(s)),
            _ => None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_months_clamps_and_restores_anchor() {
        let jan31 = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        let feb = add_months(jan31, 1, Some(31));
        assert_eq!(feb, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(add_months(feb, 1, Some(31)), NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
        assert_eq!(add_months(jan31, 12, Some(31)), NaiveDate::from_ymd_opt(2027, 1, 31).unwrap());
        assert_eq!(add_months(jan31, -1, None), NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn parse_date_accepts_datetime_strings() {
        assert_eq!(parse_date("2026-10-23 14:00:00"), NaiveDate::from_ymd_opt(2026, 10, 23));
        assert_eq!(parse_date("2026-10-23"), NaiveDate::from_ymd_opt(2026, 10, 23));
        assert_eq!(parse_date("bad"), None);
    }
}
