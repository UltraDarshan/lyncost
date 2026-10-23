//! App-lock PIN: bcrypt hashing plus persistent brute-force lockout.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;

pub const PIN_LEN: usize = 6;
const MAX_ATTEMPTS: i64 = 5;
const BASE_LOCKOUT_SECS: i64 = 30;
const MAX_LOCKOUT_SECS: i64 = 600;
#[cfg(not(test))]
const BCRYPT_COST: u32 = bcrypt::DEFAULT_COST;
#[cfg(test)]
const BCRYPT_COST: u32 = 4;

#[derive(Serialize)]
pub struct PinCheck {
    pub valid: bool,
    pub lockout_seconds: i64,
    pub remaining_attempts: i64,
}

pub fn is_valid_pin(pin: &str) -> bool {
    pin.len() == PIN_LEN && pin.bytes().all(|b| b.is_ascii_digit())
}

pub fn hash_pin(pin: &str) -> Result<String, String> {
    bcrypt::hash(pin, BCRYPT_COST).map_err(|e| format!("Failed to hash PIN: {e}"))
}

/// 30s after the 5th wrong attempt, doubling for each further miss, capped at 10 minutes.
fn lockout_for(failed: i64) -> i64 {
    let extra = (failed - MAX_ATTEMPTS).clamp(0, 10) as u32;
    (BASE_LOCKOUT_SECS << extra).min(MAX_LOCKOUT_SECS)
}

pub fn check_pin(conn: &Connection, pin: &str) -> Result<PinCheck, String> {
    let (hash, failed, locked_until): (Option<String>, i64, i64) = conn
        .query_row(
            "SELECT pin_hash, COALESCE(failed_pin_attempts, 0), COALESCE(pin_locked_until, 0) FROM app_settings WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("Failed to read PIN settings: {e}"))?;
    let hash = hash.ok_or("No PIN has been configured yet")?;

    let now = Utc::now().timestamp();
    if locked_until > now {
        return Ok(PinCheck { valid: false, lockout_seconds: locked_until - now, remaining_attempts: 0 });
    }

    if bcrypt::verify(pin, &hash).unwrap_or(false) {
        conn.execute("UPDATE app_settings SET failed_pin_attempts = 0, pin_locked_until = 0 WHERE id = 1", [])
            .map_err(|e| e.to_string())?;
        return Ok(PinCheck { valid: true, lockout_seconds: 0, remaining_attempts: MAX_ATTEMPTS });
    }

    let failed = failed + 1;
    let lockout = if failed >= MAX_ATTEMPTS { lockout_for(failed) } else { 0 };
    conn.execute(
        "UPDATE app_settings SET failed_pin_attempts = ?1, pin_locked_until = ?2 WHERE id = 1",
        params![failed, if lockout > 0 { now + lockout } else { 0 }],
    )
    .map_err(|e| e.to_string())?;
    Ok(PinCheck {
        valid: false,
        lockout_seconds: lockout,
        remaining_attempts: (MAX_ATTEMPTS - failed).max(0),
    })
}

pub fn store_pin(conn: &Connection, pin: &str) -> Result<(), String> {
    let hash = hash_pin(pin)?;
    conn.execute(
        "INSERT INTO app_settings (id, pin_hash) VALUES (1, ?1)
         ON CONFLICT(id) DO UPDATE SET pin_hash = excluded.pin_hash, failed_pin_attempts = 0, pin_locked_until = 0",
        params![hash],
    )
    .map_err(|e| format!("Failed to save PIN: {e}"))?;
    Ok(())
}

/// Databases from the interim rebuild stored the PIN in plain text.
/// Hash it in place so the user keeps the same PIN.
pub fn hash_plaintext_pin(conn: &Connection) -> Result<(), String> {
    let stored: Option<String> = conn
        .query_row("SELECT pin_hash FROM app_settings WHERE id = 1", [], |r| r.get(0))
        .unwrap_or(None);
    match stored {
        Some(pin) if !pin.starts_with("$2") => {
            if is_valid_pin(&pin) {
                store_pin(conn, &pin)
            } else {
                // Not a usable PIN either way; clear it so the user can set a new one.
                conn.execute("UPDATE app_settings SET pin_hash = NULL WHERE id = 1", [])
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_memory;

    #[test]
    fn lockout_after_five_failures() {
        let conn = open_memory();
        store_pin(&conn, "246810").unwrap();
        for expected_left in (0..MAX_ATTEMPTS).rev() {
            let r = check_pin(&conn, "000000").unwrap();
            assert!(!r.valid);
            assert_eq!(r.remaining_attempts, expected_left);
        }
        let locked = check_pin(&conn, "246810").unwrap();
        assert!(!locked.valid, "correct PIN must be refused while locked out");
        assert!(locked.lockout_seconds > 0);

        conn.execute("UPDATE app_settings SET pin_locked_until = 0", []).unwrap();
        assert!(check_pin(&conn, "246810").unwrap().valid);
        let failed: i64 = conn.query_row("SELECT failed_pin_attempts FROM app_settings", [], |r| r.get(0)).unwrap();
        assert_eq!(failed, 0);
    }

    #[test]
    fn lockout_escalates_and_caps() {
        assert_eq!(lockout_for(5), 30);
        assert_eq!(lockout_for(6), 60);
        assert_eq!(lockout_for(50), MAX_LOCKOUT_SECS);
    }
}
