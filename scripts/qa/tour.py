"""Screenshot tour of an installed Windows build: first launch, trial, activation with a real key.

    LYNCOST_TEST_LICENSE=LYNC3-... python scripts/qa/tour.py
Then seed.py and capture.py add every page in both themes.
"""
import os
import sys
import time

from wd import Session

key = os.environ["LYNCOST_TEST_LICENSE"]
s = Session.restart(keep_welcome=True)
time.sleep(2)
s.shot("01-first-launch-welcome")
s.click_text("Start using Lyncost")
time.sleep(1)
s.shot("02-dashboard-trial")
print("before:", {k: v for k, v in s.invoke("get_license_status").items() if k != "license_key"})

s.click_text("Enter key")
time.sleep(1.5)
s.shot("03-activation-window")
s.js(
    "const t=document.querySelector('textarea'); const set=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set;"
    "set.call(t, arguments[0]); t.dispatchEvent(new Event('input',{bubbles:true}));",
    key,
)
s.shot("04-key-pasted")
s.click_text("Activate Perpetual License")
time.sleep(1)
s.shot("05-activation-successful")
time.sleep(3)
s.shot("06-dashboard-licensed")

status = s.invoke("get_license_status")
print("after:", {k: v for k, v in status.items() if k != "license_key"})
errors = s.errors()
print("JS errors:", errors)
sys.exit(0 if status["state"] == "licensed" and not errors else 1)
