"""End-to-end QA for the Windows trial and license, through the real app.

Needs a QA build (`--features qa`, plus `license-gate` on Linux) on a fresh profile:
    sh scripts/qa/env.sh reset && python3 scripts/qa/license_flows.py

The activation screens only render on Windows (the bundle checks the user agent), so on Linux
this checks the backend side and skips the on-screen checks.
"""
import subprocess
import sys
import time
from pathlib import Path

from wd import ROOT, Session

ISSUE = ROOT / "scripts" / "license" / "issue.mjs"
TEST_KEY = ROOT / "scripts" / "license" / "test-private.pem"
results = []


def check(name, ok, detail=""):
    results.append((name, bool(ok)))
    print(f"[{'PASS' if ok else 'FAIL'}] {name} {detail}".rstrip(), flush=True)


def refused(fn):
    try:
        fn()
    except AssertionError as e:
        return "trial has ended" in str(e)
    return False


def issue(name):
    return subprocess.run(["node", str(ISSUE), "--name", name, "--key", str(TEST_KEY)], check=True, capture_output=True, text=True).stdout.strip()


def reload(s):
    s.js("location.reload()")
    time.sleep(4)


def account(s, name):
    return s.invoke("create_account", {"payload": {"name": name, "type": "bank", "currency": "INR", "opening_balance": 1000}})


s = Session.restart(keep_welcome=True)
time.sleep(1.5)
win = "Windows" in s.js("return navigator.userAgent")
print(f"platform: {'Windows' if win else 'other (backend checks only)'}")

# 1. First run: welcome popup and a full 14-day trial.
check("welcome popup on first run", "Congratulations! Your 14\u2011day free trial has started" in s.text())
s.shot("license-1-welcome")
s.click_text("Start using Lyncost")
time.sleep(0.5)
check("welcome closes", "Congratulations!" not in s.text())
st = s.invoke("get_license_status")
check("trial started with 14 days", st["state"] == "trial" and st["days_remaining"] == 14, str(st))
check("can add records during trial", account(s, "Trial Bank")["id"] > 0)
reload(s)
check("welcome shows only once", "Congratulations!" not in s.text())

# 2. Day 15: the trial is over.
s.invoke("qa_set_clock_offset", {"hours": 15 * 24})
reload(s)
st = s.invoke("get_license_status")
check("trial expired after 14 days", st["state"] == "expired" and st["days_remaining"] == 0, str(st))
if win:
    check("lock screen shown", "Your free trial has ended" in s.text())
    s.shot("license-2-expired")
check("adding records refused", refused(lambda: account(s, "Should Fail")))
check("existing data still readable", any(a["name"] == "Trial Bank" for a in s.invoke("get_accounts")))

# 3. Faking the unlock flag in the page does nothing.
s.js("localStorage.setItem('lyncost_license_activated','true'); localStorage.setItem('lyncost_license_key','LYNC3-FAKE')")
reload(s)
check("fake unlock flag is reset on start", s.js("return localStorage.getItem('lyncost_license_activated')") is None)
if win:
    check("lock screen still shown after fake flag", "Your free trial has ended" in s.text())
check("writes still refused after fake flag", refused(lambda: account(s, "Should Fail")))

# 4. Wrong and forged keys.
good = issue("QA Tester")
forged = good[:-1] + ("A" if good[-1] != "A" else "B")
check("random text rejected", s.invoke("verify_license_key", {"key": "hello"}) is False)
check("old-format key rejected", s.invoke("verify_license_key", {"key": "LYNC2-ABCDE-FGHIJ-KLMNO"}) is False)
check("forged key rejected", s.invoke("verify_license_key", {"key": forged}) is False)
check("still locked after bad keys", s.invoke("get_license_status")["state"] == "expired")

# 5. A real key, typed into the activation screen on Windows.
if win:
    s.js(
        "const t=document.querySelector('textarea'); const set=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set;"
        "set.call(t, arguments[0]); t.dispatchEvent(new Event('input',{bubbles:true}));",
        good,
    )
    s.click_text("Activate Perpetual License")
    time.sleep(3)
    check("activation screen accepts the key", "Activation Successful!" in s.text() or "Your free trial has ended" not in s.text())
    s.shot("license-3-activated")
else:
    check("real key accepted", s.invoke("verify_license_key", {"key": good}) is True)
st = s.invoke("get_license_status")
check("licensed to the buyer", st["state"] == "licensed" and st["licensed_to"] == "QA Tester", str(st))
check("can add records again", account(s, "Licensed Bank")["id"] > 0)

# 6. Still licensed after closing and reopening, even far in the future.
s = Session.restart()
time.sleep(1.5)
s.invoke("qa_set_clock_offset", {"hours": 400 * 24})
reload(s)
st = s.invoke("get_license_status")
check("license survives a restart", st["state"] == "licensed", str(st))
check("unlock flag restored from the backend", s.js("return localStorage.getItem('lyncost_license_activated')") == "true")
if win:
    check("no lock screen when licensed", "Your free trial has ended" not in s.text())
    s.click_text("Settings")
    time.sleep(1.5)
    settings = s.text()
    check("Settings shows who it is licensed to", "Licensed to QA Tester" in settings)
    check("no Linux wording in the Windows app", "Linux" not in settings and "Fedora" not in settings)
    version = s.js("return window.__LYNCOST_VERSION__")
    check("Settings shows the real version", "Lyncost v" + version in settings)
    check("no old version numbers anywhere", "v1.0.0" not in settings or version == "1.0.0")
    check("Windows edition is not called free", "Free & Private" not in settings)
    s.shot("license-4-settings")

errors = s.errors()
failed = [n for n, ok in results if not ok]
print(f"\n{len(results) - len(failed)} passed, {len(failed)} failed; JS errors: {errors}")
Path(ROOT / "src-tauri" / "target" / "qa").mkdir(parents=True, exist_ok=True)
sys.exit(1 if failed else 0)
