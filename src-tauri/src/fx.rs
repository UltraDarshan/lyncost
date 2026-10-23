//! Currency conversion using the user-maintained `exchange_rates` table.
//!
//! A rate row means `1 from_currency = rate to_currency`. Lookups use the
//! direct pair, its inverse, or a path through other currencies (e.g.
//! USD → INR → EUR), so users only need to enter each currency once.

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::Connection;

pub fn base_currency(conn: &Connection) -> String {
    conn.query_row("SELECT base_currency FROM app_settings WHERE id = 1", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "INR".to_string())
}

/// Conversion rate, or `None` when no path between the currencies is known.
pub fn rate(conn: &Connection, from: &str, to: &str) -> Option<f64> {
    let (from, to) = (from.trim().to_uppercase(), to.trim().to_uppercase());
    if from == to {
        return Some(1.0);
    }
    let mut graph: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT from_currency, to_currency, rate FROM exchange_rates").ok()?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?)))
        .ok()?;
    for (a, b, r) in rows.flatten() {
        if r > 0.0 && r.is_finite() {
            let (a, b) = (a.to_uppercase(), b.to_uppercase());
            graph.entry(a.clone()).or_default().push((b.clone(), r));
            graph.entry(b).or_default().push((a, 1.0 / r));
        }
    }

    let mut seen = HashSet::from([from.clone()]);
    let mut queue = VecDeque::from([(from, 1.0)]);
    while let Some((cur, acc)) = queue.pop_front() {
        for (next, r) in graph.get(&cur).into_iter().flatten() {
            if *next == to {
                return Some(acc * r);
            }
            if seen.insert(next.clone()) {
                queue.push_back((next.clone(), acc * r));
            }
        }
    }
    None
}

/// Rate into the base currency, falling back to 1.0 when no rate is configured.
pub fn rate_to_base(conn: &Connection, from: &str) -> f64 {
    rate(conn, from, &base_currency(conn)).unwrap_or(1.0)
}

pub fn convert(conn: &Connection, amount: f64, from: &str, to: &str) -> f64 {
    amount * rate(conn, from, to).unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    #[test]
    fn direct_inverse_and_cross_rates() {
        let conn = open_memory(); // seeds USD→INR 88, EUR→INR 102
        assert_eq!(rate(&conn, "INR", "INR"), Some(1.0));
        assert_eq!(rate(&conn, "USD", "INR"), Some(88.0));
        assert!((rate(&conn, "INR", "USD").unwrap() - 1.0 / 88.0).abs() < 1e-12);
        assert!((rate(&conn, "USD", "EUR").unwrap() - 88.0 / 102.0).abs() < 1e-12);
        assert_eq!(rate(&conn, "USD", "XYZ"), None);
    }
}
