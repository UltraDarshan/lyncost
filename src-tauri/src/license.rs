//! Lyncost for Windows: a 14-day trial, then a lifetime license key.
//!
//! Keys (`LYNC3-...`) are Ed25519-signed by Wintershogun's key server. The app only holds the
//! public key, so it can check a key but never make one. The trial start, the last time the app
//! ran, and the license key are also kept outside the database (the Windows registry and a small
//! state file), sealed with an HMAC tied to this computer, so deleting or editing the database,
//! reinstalling, or turning the clock back does not restart or extend the trial.
//!
//! The gate is enforced here, in the backend: when the trial is over and no valid key is
//! activated, commands that add, edit, or delete records are refused (see [`blocks`]). Viewing,
//! backups, and exports keep working, so nobody's data is held hostage.

use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use hmac::{Hmac, Mac};
use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::util::{CmdResult, Ctx};

pub const TRIAL_DAYS: i64 = 14;
const PREFIX: &str = "LYNC3";
/// How far the clock may go backwards (time-zone or NTP corrections) before it counts as tampering.
const CLOCK_SLACK_HOURS: i64 = 36;

/// Public half of the license signing key. QA builds use a test key so CI can issue keys;
/// release builds must never be built with the `qa` feature (the release workflow checks).
#[cfg(not(feature = "qa"))]
const PUBLIC_KEY_HEX: &str = "34f9642006dd0abb2d23ccf308c6af3e82669b49f684decf65774cda74c67d0d";
#[cfg(feature = "qa")]
const PUBLIC_KEY_HEX: &str = "50c0a27d8cdd2fb70e4bea9f57938c3060e4e96351bd62d9ce904e12e4d56ad0";

/// Mixed into the seal key; not a secret on its own (it ships in the binary), it just makes the
/// sealed values meaningless to anyone who copies them to another computer.
const SEAL_SALT: &[u8] = b"lyncost/seal/v1/7f3c9a1e";

/// Whether this build enforces the license. Linux builds are free for everyone.
pub fn gate_enabled() -> bool {
    cfg!(windows) || cfg!(feature = "license-gate")
}

// ── Keys ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct License {
    pub id: String,
    pub name: String,
    pub issued: String,
}

fn public_key() -> VerifyingKey {
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&PUBLIC_KEY_HEX[i * 2..i * 2 + 2], 16).expect("valid public key hex");
    }
    VerifyingKey::from_bytes(&bytes).expect("valid Ed25519 public key")
}

/// Checks a key's signature and decodes it. Accepts any case, spaces, and dashes.
pub fn parse_key(key: &str) -> Option<License> {
    let compact: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    let body = compact.strip_prefix(PREFIX)?;
    let raw = data_encoding::BASE32_NOPAD.decode(body.as_bytes()).ok()?;
    if raw.len() < 11 + 64 {
        return None;
    }
    let (payload, sig) = raw.split_at(raw.len() - 64);
    let mut message = PREFIX.as_bytes().to_vec();
    message.extend_from_slice(payload);
    public_key().verify(&message, &Signature::from_slice(sig).ok()?).ok()?;

    // [version][id: 6][issued: u16][edition][name len][name]
    if payload[0] != 1 || payload[9] != 1 {
        return None;
    }
    let name_len = payload[10] as usize;
    let name = std::str::from_utf8(payload.get(11..11 + name_len)?).ok()?.to_string();
    if payload.len() != 11 + name_len {
        return None;
    }
    let days = u16::from_be_bytes([payload[7], payload[8]]) as i64;
    let issued = (Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap() + Duration::days(days)).format("%Y-%m-%d").to_string();
    let id = payload[1..7].iter().map(|b| format!("{b:02x}")).collect();
    Some(License { id, name, issued })
}

// ── Sealed state outside the database ────────────────────────────────────────

/// Somewhere to keep small sealed values that survive a database wipe or reinstall.
pub trait Store {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str);
}

/// `name=value` lines in a file outside the app's data folder.
pub struct FileStore(pub std::path::PathBuf);

impl Store for FileStore {
    fn get(&self, name: &str) -> Option<String> {
        let text = std::fs::read_to_string(&self.0).ok()?;
        text.lines().find_map(|l| l.strip_prefix(&format!("{name}=")).map(str::to_string))
    }
    fn set(&self, name: &str, value: &str) {
        let mut lines: Vec<String> = std::fs::read_to_string(&self.0)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.starts_with(&format!("{name}=")))
            .map(str::to_string)
            .collect();
        lines.push(format!("{name}={value}"));
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.0, lines.join("\n") + "\n");
    }
}

#[cfg(windows)]
pub struct RegistryStore;

#[cfg(windows)]
impl RegistryStore {
    fn key(write: bool) -> Option<winreg::RegKey> {
        use winreg::enums::*;
        let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
        if write {
            hkcu.create_subkey(r"Software\Wintershogun\Lyncost").ok().map(|(k, _)| k)
        } else {
            hkcu.open_subkey(r"Software\Wintershogun\Lyncost").ok()
        }
    }
}

#[cfg(windows)]
impl Store for RegistryStore {
    fn get(&self, name: &str) -> Option<String> {
        Self::key(false)?.get_value(name).ok()
    }
    fn set(&self, name: &str, value: &str) {
        if let Some(k) = Self::key(true) {
            let _ = k.set_value(name, &value.to_string());
        }
    }
}

/// The stores this computer uses: the registry (Windows) and a state file.
pub fn system_stores() -> Vec<Box<dyn Store>> {
    let mut stores: Vec<Box<dyn Store>> = Vec::new();
    #[cfg(windows)]
    stores.push(Box::new(RegistryStore));
    if let Some(dir) = dirs::config_dir() {
        stores.push(Box::new(FileStore(dir.join("Wintershogun").join("lyncost.state"))));
    }
    stores
}

fn machine_id() -> String {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        let guid: Option<String> = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(r"SOFTWARE\Microsoft\Cryptography", KEY_READ | KEY_WOW64_64KEY)
            .and_then(|k| k.get_value("MachineGuid"))
            .ok();
        if let Some(g) = guid {
            return g;
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(id) = std::fs::read_to_string("/etc/machine-id") {
            return id.trim().to_string();
        }
    }
    "lyncost".to_string()
}

pub struct Sealer {
    key: Vec<u8>,
}

impl Sealer {
    pub fn for_machine(machine: &str) -> Self {
        let mut h = Sha256::new();
        h.update(SEAL_SALT);
        h.update(machine.as_bytes());
        Self { key: h.finalize().to_vec() }
    }

    pub fn system() -> Self {
        Self::for_machine(&machine_id())
    }

    fn tag(&self, name: &str, value: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).expect("any key length works");
        mac.update(name.as_bytes());
        mac.update(b"=");
        mac.update(value.as_bytes());
        mac.finalize().into_bytes()[..16].iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn seal(&self, name: &str, value: &str) -> String {
        format!("{value}|{}", self.tag(name, value))
    }

    /// `Ok(None)` = not stored, `Ok(Some)` = genuine, `Err` = stored but altered.
    pub fn open(&self, name: &str, sealed: Option<String>) -> Result<Option<String>, ()> {
        let Some(sealed) = sealed else { return Ok(None) };
        let (value, tag) = sealed.rsplit_once('|').ok_or(())?;
        if self.tag(name, value) == tag {
            Ok(Some(value.to_string()))
        } else {
            Err(())
        }
    }
}

// ── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct LicenseStatus {
    /// "free" (Linux), "licensed", "trial", or "expired".
    pub state: &'static str,
    pub days_total: i64,
    pub days_remaining: i64,
    pub trial_started_at: String,
    pub licensed_to: Option<String>,
    pub license_id: Option<String>,
    /// The activated key, so Settings can show and copy it.
    pub license_key: Option<String>,
    /// True only on the very first run, so the app can welcome the person to their trial.
    pub first_run: bool,
    /// The clock was turned back or the stored trial data was altered.
    pub tampered: bool,
}

impl LicenseStatus {
    pub fn unlocked(&self) -> bool {
        matches!(self.state, "free" | "licensed" | "trial")
    }
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

/// Works out the license state and records this run. `now` is passed in so tests can move the clock.
pub fn evaluate(conn: &Connection, stores: &[Box<dyn Store>], sealer: &Sealer, now: DateTime<Utc>) -> CmdResult<LicenseStatus> {
    let row: (Option<String>, Option<String>, Option<String>) = conn
        .query_row("SELECT trial_started_at, license_key, last_seen_at FROM app_settings WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .ctx("Failed to read license state")?;
    let (db_start, db_key, db_seen) = row;
    let mut tampered = false;
    let mut read = |name: &str| -> Vec<String> {
        stores
            .iter()
            .filter_map(|s| match sealer.open(name, s.get(name)) {
                Ok(v) => v,
                Err(()) => {
                    tampered = true;
                    None
                }
            })
            .collect()
    };

    // License: the database copy, or the sealed copy that survives a wipe.
    let stored_keys = read("lk");
    let license = db_key.iter().chain(stored_keys.iter()).find_map(|k| parse_key(k).map(|l| (k.clone(), l)));

    // Trial start: the earliest genuine record wins, so nothing can push it later.
    let starts: Vec<DateTime<Utc>> = read("ts").iter().chain(db_start.iter()).filter_map(|s| parse_time(s)).collect();
    let first_run = starts.is_empty();
    let start = starts.into_iter().min().unwrap_or(now);

    // Clock rollback: the latest time the app has seen.
    let last_seen = read("ls").iter().chain(db_seen.iter()).filter_map(|s| parse_time(s)).max();
    if let Some(seen) = last_seen {
        if now + Duration::hours(CLOCK_SLACK_HOURS) < seen {
            tampered = true;
        }
    }
    let seen_now = last_seen.map_or(now, |s| s.max(now));

    // Record: database first, then the sealed copies.
    conn.execute(
        "UPDATE app_settings SET trial_started_at = ?1, last_seen_at = ?2, license_key = COALESCE(?3, license_key) WHERE id = 1",
        params![start.to_rfc3339(), seen_now.to_rfc3339(), license.as_ref().map(|(k, _)| k.clone())],
    )
    .ctx("Failed to save license state")?;
    for s in stores {
        if sealer.open("ts", s.get("ts")).ok().flatten().and_then(|v| parse_time(&v)) != Some(start) {
            s.set("ts", &sealer.seal("ts", &start.to_rfc3339()));
        }
        s.set("ls", &sealer.seal("ls", &seen_now.to_rfc3339()));
        if let Some((k, _)) = &license {
            s.set("lk", &sealer.seal("lk", k));
        }
    }

    let used = (now - start).num_days().max(0);
    let remaining = if tampered { 0 } else { (TRIAL_DAYS - used).clamp(0, TRIAL_DAYS) };
    let state = if !gate_enabled() {
        "free"
    } else if license.is_some() {
        "licensed"
    } else if remaining > 0 {
        "trial"
    } else {
        "expired"
    };
    Ok(LicenseStatus {
        state,
        days_total: TRIAL_DAYS,
        days_remaining: remaining,
        trial_started_at: start.to_rfc3339(),
        licensed_to: license.as_ref().map(|(_, l)| l.name.clone()),
        license_id: license.as_ref().map(|(_, l)| l.id.clone()),
        license_key: license.map(|(k, _)| k),
        first_run,
        tampered,
    })
}

/// Saves a key after checking it. Returns the license it unlocks.
pub fn activate(conn: &Connection, stores: &[Box<dyn Store>], sealer: &Sealer, key: &str) -> Option<License> {
    let license = parse_key(key)?;
    let normalized: String = key.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_uppercase();
    let _ = conn.execute(
        "UPDATE app_settings SET license_key = ?1, license_activated_at = ?2 WHERE id = 1",
        params![normalized, Utc::now().to_rfc3339()],
    );
    for s in stores {
        s.set("lk", &sealer.seal("lk", &normalized));
    }
    Some(license)
}

/// Commands refused once the trial has ended without a license: anything that adds, edits,
/// or deletes records. Startup routines, viewing, backups, and exports stay allowed.
pub fn blocks(command: &str) -> bool {
    const WRITES: &[&str] = &[
        "create_", "update_", "delete_", "archive_", "import_", "contribute_", "spend_", "draw_", "record_debt_",
        "mark_", "unmark_", "toggle_", "clear_", "bulk_", "set_exchange_rate", "apply_regional_", "set_base_currency",
    ];
    WRITES.iter().any(|w| command.starts_with(w)) && !matches!(command, "update_notification_settings" | "create_backup")
}

pub const LOCKED_MESSAGE: &str =
    "Your free trial has ended. Enter your license key to keep adding and editing records. Your data is safe and you can still view and export it.";

/// Current time; QA builds can move it forward to test trial expiry.
pub fn now() -> DateTime<Utc> {
    #[cfg(feature = "qa")]
    {
        let offset = QA_CLOCK_OFFSET_HOURS.load(std::sync::atomic::Ordering::Relaxed);
        return Utc::now() + Duration::hours(offset);
    }
    #[allow(unreachable_code)]
    Utc::now()
}

#[cfg(feature = "qa")]
pub static QA_CLOCK_OFFSET_HOURS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// The license state on this computer right now.
pub fn current_status(conn: &Connection) -> CmdResult<LicenseStatus> {
    evaluate(conn, &system_stores(), &Sealer::system(), now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Mem(Rc<RefCell<HashMap<String, String>>>);
    impl Store for Mem {
        fn get(&self, name: &str) -> Option<String> {
            self.0.borrow().get(name).cloned()
        }
        fn set(&self, name: &str, value: &str) {
            self.0.borrow_mut().insert(name.into(), value.into());
        }
    }

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE app_settings (id INTEGER PRIMARY KEY, trial_started_at TEXT, license_key TEXT,
             license_activated_at TEXT, last_seen_at TEXT); INSERT INTO app_settings (id) VALUES (1);",
        )
        .unwrap();
        conn
    }

    fn t(day: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 1, 10, 0, 0).unwrap() + Duration::days(day)
    }

    // Test keys signed with scripts/license/test-private.pem (only accepted by `qa` builds).
    #[cfg(feature = "qa")]
    const QA_KEY: &str = "LYNC3-AEAQEA-YEAWVA-CKIBBF-IUCICU-MVZXIZ-LSUKTW-RDRSRI-V5D3TW-VV5DQZ-7HN2CN-7WVJAJ-A5YLFX-7O4ZIC-LOCQLB-FIA5IP-RDOQ2H-OXEN33-TQ2R3I-OABE32-NFA7XR-SEWFRJ-WYARGN-SCI";

    #[test]
    fn junk_and_tampered_keys_are_rejected() {
        assert!(parse_key("").is_none());
        assert!(parse_key("LYNC2-ABCDE-FGHIJ").is_none());
        assert!(parse_key("LYNC3-AAAAAA-AAAAAA").is_none());
        // A real-looking key signed by some other key never passes.
        let forged = "LYNC3-AEAQEA-YEAWVA-CKIBBF-IUCICU-MVZXIZ-LSUKTW-RDRSRI-V5D3TW-VV5DQZ-7HN2CN-7WVJAJ-A5YLFX-7O4ZIC-LOCQLB-FIA5IP-RDOQ2H-OXEN33-TQ2R3I-OABE32-NFA7XR-SEWFRJ-WYARGN-SCA";
        assert!(parse_key(forged).is_none());
    }

    #[cfg(feature = "qa")]
    #[test]
    fn test_key_decodes_and_any_formatting_works() {
        let l = parse_key(QA_KEY).expect("valid QA key");
        assert_eq!(l.name, "QA Tester");
        assert_eq!(l.id, "0102030405aa");
        assert_eq!(l.issued, "2026-10-25");
        assert_eq!(parse_key(&QA_KEY.to_lowercase().replace('-', " ")), Some(l.clone()));
        // Changing one character of the name breaks the signature.
        let mut chars: Vec<char> = QA_KEY.chars().collect();
        chars[40] = if chars[40] == 'A' { 'B' } else { 'A' };
        assert!(parse_key(&chars.into_iter().collect::<String>()).is_none());
    }

    #[test]
    fn trial_runs_14_days_then_expires() {
        let conn = db();
        let stores: Vec<Box<dyn Store>> = vec![Box::new(Mem::default())];
        let sealer = Sealer::for_machine("pc-1");
        let first = evaluate(&conn, &stores, &sealer, t(0)).unwrap();
        assert!(first.first_run);
        assert_eq!(first.days_remaining, 14);
        assert!(!evaluate(&conn, &stores, &sealer, t(1)).unwrap().first_run);
        assert_eq!(evaluate(&conn, &stores, &sealer, t(13)).unwrap().days_remaining, 1);
        let over = evaluate(&conn, &stores, &sealer, t(14)).unwrap();
        assert_eq!(over.days_remaining, 0);
        if gate_enabled() {
            assert_eq!(over.state, "expired");
            assert!(!over.unlocked());
        }
    }

    #[test]
    fn wiping_the_database_does_not_restart_the_trial() {
        let store = Mem::default();
        let sealer = Sealer::for_machine("pc-1");
        evaluate(&db(), &[Box::new(store.clone()) as Box<dyn Store>], &sealer, t(0)).unwrap();
        // Fresh database ten days later, same computer.
        let again = evaluate(&db(), &[Box::new(store) as Box<dyn Store>], &sealer, t(10)).unwrap();
        assert!(!again.first_run);
        assert_eq!(again.days_remaining, 4);
    }

    #[test]
    fn editing_the_database_start_date_does_not_extend_the_trial() {
        let conn = db();
        let stores: Vec<Box<dyn Store>> = vec![Box::new(Mem::default())];
        let sealer = Sealer::for_machine("pc-1");
        evaluate(&conn, &stores, &sealer, t(0)).unwrap();
        conn.execute("UPDATE app_settings SET trial_started_at = ?1", [t(12).to_rfc3339()]).unwrap();
        assert_eq!(evaluate(&conn, &stores, &sealer, t(13)).unwrap().days_remaining, 1);
    }

    #[test]
    fn turning_the_clock_back_ends_the_trial() {
        let conn = db();
        let stores: Vec<Box<dyn Store>> = vec![Box::new(Mem::default())];
        let sealer = Sealer::for_machine("pc-1");
        evaluate(&conn, &stores, &sealer, t(0)).unwrap();
        evaluate(&conn, &stores, &sealer, t(10)).unwrap();
        let back = evaluate(&conn, &stores, &sealer, t(3)).unwrap();
        assert!(back.tampered);
        assert_eq!(back.days_remaining, 0);
        // Small corrections (time zone, NTP) are fine.
        let conn2 = db();
        evaluate(&conn2, &stores[..0], &sealer, t(5)).unwrap();
        assert!(!evaluate(&conn2, &stores[..0], &sealer, t(5) - Duration::hours(10)).unwrap().tampered);
    }

    #[test]
    fn altered_sealed_values_count_as_tampering() {
        let store = Mem::default();
        let sealer = Sealer::for_machine("pc-1");
        evaluate(&db(), &[Box::new(store.clone()) as Box<dyn Store>], &sealer, t(0)).unwrap();
        store.set("ts", &format!("{}|0000", t(9).to_rfc3339()));
        let s = evaluate(&db(), &[Box::new(store.clone()) as Box<dyn Store>], &sealer, t(10)).unwrap();
        assert!(s.tampered);
        // Values copied from another computer do not verify either.
        let other = Sealer::for_machine("pc-2");
        assert!(other.open("ls", store.get("ls")).is_err());
    }

    #[test]
    fn only_writes_are_blocked() {
        for c in ["create_transaction", "update_account", "delete_bill", "import_csv_transactions", "mark_bill_paid", "set_base_currency"] {
            assert!(blocks(c), "{c} should be blocked");
        }
        for c in [
            "get_transactions", "get_license_status", "activate_license", "verify_license_key", "create_backup", "save_export_file",
            "check_and_run_daily_backup", "process_recurring_rules", "verify_pin", "set_app_theme", "update_notification_settings",
            "check_app_update", "install_app_update",
        ] {
            assert!(!blocks(c), "{c} should be allowed");
        }
    }

    #[cfg(feature = "qa")]
    #[test]
    fn a_license_unlocks_and_survives_a_wipe() {
        let store = Mem::default();
        let sealer = Sealer::for_machine("pc-1");
        let conn = db();
        let stores: Vec<Box<dyn Store>> = vec![Box::new(store.clone())];
        evaluate(&conn, &stores, &sealer, t(0)).unwrap();
        assert_eq!(activate(&conn, &stores, &sealer, QA_KEY).unwrap().name, "QA Tester");
        let later = evaluate(&db(), &stores, &sealer, t(400)).unwrap();
        assert_eq!(later.licensed_to.as_deref(), Some("QA Tester"));
        if gate_enabled() {
            assert_eq!(later.state, "licensed");
        }
    }
}
