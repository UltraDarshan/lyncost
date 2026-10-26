"""Minimal W3C WebDriver client + helpers for driving the Lyncost QA build."""
import base64
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

BASE = "http://127.0.0.1:4444"
ROOT = Path(__file__).resolve().parents[2]
QA = ROOT / "src-tauri" / "target" / "qa"
SHOTS = QA / "shots"
# The QA build; LYNCOST_QA_APP points elsewhere (for example the Windows CI build).
APP = Path(os.environ.get("LYNCOST_QA_APP") or ROOT / "src-tauri" / "target" / "debug" / ("lyncost.exe" if sys.platform == "win32" else "lyncost"))
SESSION_FILE = QA / "session.txt"


def req(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    r = urllib.request.Request(BASE + path, data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(r, timeout=120) as resp:
            return json.loads(resp.read() or b"{}").get("value")
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"{method} {path}: {e.read().decode()[:400]}") from None


CLICK_TEXT_JS = """
const [txt] = arguments;
const els = [...document.querySelectorAll('button, a, [role=button], li, div, span')]
  .filter(e => e.offsetParent !== null && (e.innerText || '').trim() === txt);
const el = els.find(e => e.tagName === 'BUTTON' || e.tagName === 'A') || els[0];
if (!el) return false;
(el.closest('button, a, [role=button]') || el).click();
return true;
"""


class Session:
    def __init__(self, session_id=None):
        if session_id is None:
            caps = {"capabilities": {"alwaysMatch": {"tauri:options": {"application": str(APP)}}}}
            session_id = req("POST", "/session", caps)["sessionId"]
            SESSION_FILE.parent.mkdir(parents=True, exist_ok=True)
            SESSION_FILE.write_text(session_id)
        self.id = session_id

    @classmethod
    def attach(cls):
        return cls(SESSION_FILE.read_text().strip())

    @classmethod
    def restart(cls, keep_welcome=False):
        try:
            cls.attach().close()
        except Exception:
            pass
        time.sleep(1)
        s = cls()
        time.sleep(2.5)
        s.js("window.__errs=[];window.addEventListener('error',e=>window.__errs.push('E:'+e.message));"
             "window.addEventListener('unhandledrejection',e=>window.__errs.push('R:'+String(e.reason)))")
        # First run of a gated build shows the trial welcome over everything; only license_flows checks it.
        if not keep_welcome and "Congratulations!" in s.text() and "Start using Lyncost" in s.text():
            s.click_text("Start using Lyncost")
            time.sleep(0.5)
        return s

    def p(self, path):
        return f"/session/{self.id}{path}"

    def js(self, script, *args):
        return req("POST", self.p("/execute/sync"), {"script": script, "args": list(args)})

    def invoke(self, cmd, args=None):
        """Call a backend command through the page's own IPC bridge."""
        r = req("POST", self.p("/execute/async"), {"script":
            "const [c,a,done]=arguments; window.__TAURI_INTERNALS__.invoke(c,a||{})"
            ".then(v=>done({ok:v})).catch(e=>done({err:String(e)}))", "args": [cmd, args or {}]})
        if "err" in r:
            raise AssertionError(f"{cmd}: {r['err']}")
        return r["ok"]

    def click_text(self, text):
        return self.js(CLICK_TEXT_JS, text)

    def text(self):
        return self.js("return document.body.innerText")

    def errors(self):
        return self.js("return window.__errs||[]")

    def unlock(self, pin):
        if "Enter your 6-digit PIN" in self.text():
            for d in pin:
                self.click_text(d)
            time.sleep(3)

    def shot(self, name):
        # Screenshots may be public (CI artifacts), so license keys are blurred first.
        self.js("for (const el of document.querySelectorAll('textarea, code, span, p, div')) "
                "if (!el.children.length && /LYNC3-/i.test(el.value || el.textContent)) el.style.filter = 'blur(6px)'")
        SHOTS.mkdir(parents=True, exist_ok=True)
        path = SHOTS / f"{name}.png"
        if os.environ.get("LYNCOST_DESKTOP_SHOTS"):
            # The whole screen (window frame, title bar, taskbar), as a person sees it.
            from PIL import ImageGrab
            time.sleep(0.4)
            ImageGrab.grab(all_screens=True).save(path)
        else:
            path.write_bytes(base64.b64decode(req("GET", self.p("/screenshot"))))
        return path

    def close(self):
        req("DELETE", self.p(""))
