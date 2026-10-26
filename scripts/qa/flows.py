"""End-to-end UI QA: every feature through its real form, verified against the backend.

    sh scripts/qa/env.sh reset && python3 scripts/qa/seed.py && python3 scripts/qa/flows.py
"""
import datetime as dt
import os
import time
import traceback

from wd import Session

TODAY = dt.date.today().isoformat()
s = Session.restart()
s.js("window.confirm=()=>true")  # accept the app's native "Are you sure?" dialogs
results = []

FILL_JS = r"""
let [label, value] = arguments;
const m = [...document.querySelectorAll('.fixed.inset-0 > div')].pop() || document.querySelector('main');
const l = [...m.querySelectorAll('label')].find(l => l.innerText.trim().toLowerCase().startsWith(label.toLowerCase()));
if (!l) return 'no label ' + label;
let el = null, p = l.parentElement;
for (let i = 0; i < 4 && !el && p; i++, p = p.parentElement) el = p.querySelector('input, select, textarea');
if (!el) return 'no control ' + label;
if (el.tagName === 'SELECT') {
  const opt = [...el.options].find(o => o.text.toLowerCase().includes(String(value).toLowerCase()));
  if (!opt) return 'no option ' + value;
  value = opt.value;
}
const proto = el.tagName === 'SELECT' ? HTMLSelectElement : el.tagName === 'TEXTAREA' ? HTMLTextAreaElement : HTMLInputElement;
Object.getOwnPropertyDescriptor(proto.prototype, 'value').set.call(el, value);
el.dispatchEvent(new Event('input', {bubbles: true}));
el.dispatchEvent(new Event('change', {bubbles: true}));
return 'ok';
"""

MODAL_CLICK_JS = r"""
const m = [...document.querySelectorAll('.fixed.inset-0 > div')].pop() || document;
const b = [...m.querySelectorAll('button')].filter(b => b.offsetParent !== null &&
  b.innerText.trim().toLowerCase().startsWith(arguments[0].toLowerCase())).pop();
if (!b) return false; b.click(); return true;
"""

# Records what <main> shows on each animation frame after a nav click.
FRAMES_JS = r"""
const [label] = arguments; const done = arguments[arguments.length - 1];
const main = document.querySelector('main'); const log = []; const t0 = performance.now();
[...document.querySelectorAll('aside nav button')].find(b => b.innerText.trim().startsWith(label)).click();
(function sample() {
  const page = main.firstElementChild;
  log.push([(main.innerText || '').trim().length > 0, page ? +getComputedStyle(page).opacity : 1]);
  if (performance.now() - t0 < 700) requestAnimationFrame(sample); else done(log);
})();
"""


def go(page):
    assert s.click_text(page), f"nav {page}"
    time.sleep(1.1)


def click(text):
    assert s.click_text(text), f"click {text}"
    time.sleep(0.8)


def modal(text):
    assert s.js(MODAL_CLICK_JS, text), f"modal button {text}"
    time.sleep(1.2)


def fill(label, value):
    r = s.js(FILL_JS, label, str(value))
    assert r == "ok", r


def find(cmd, key, value, args=None):
    return next((x for x in s.invoke(cmd, args) if x.get(key) == value), None)


def bal(name):
    return find("get_accounts", "name", name, {"includeArchived": True})["current_balance"]


def check(name, ok, detail=""):
    results.append((ok, name, detail))
    print(f"[{'PASS' if ok else 'FAIL'}] {name} {detail}")


def step(fn):
    try:
        fn()
    except Exception as e:
        check(fn.__name__, False, f"EXCEPTION {e!r}"[:220])
        traceback.print_exc(limit=1)
        s.shot("FAIL-" + fn.__name__)
        s.js("document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))")
        if s.js("return !!document.querySelector('.fixed.inset-0 > div')"):
            s.js(MODAL_CLICK_JS, "Cancel")
    return fn


@step
def accounts():
    go("Accounts"); click("Add Account"); modal("Cash Wallet")
    fill("Account Name", "QA Petty Cash"); fill("Opening Balance", "5000"); modal("Save Account")
    a = find("get_accounts", "name", "QA Petty Cash")
    check("create account", a and a["current_balance"] == 5000)


@step
def transactions():
    go("Transactions"); click("Add Transaction")
    fill("Account", "QA Petty Cash"); fill("Category", "Food"); fill("Amount", "350.5"); fill("Note", "QA lunch")
    modal("Record Transaction")
    t = find("get_transactions", "note", "QA lunch", {"filter": {}})
    check("create expense", t and t["amount"] == 350.5 and bal("QA Petty Cash") == 4649.5)
    s.js("const r=[...document.querySelectorAll('tbody tr')].find(r=>r.innerText.includes('QA lunch')); [...r.querySelectorAll('button')][1].click();")
    time.sleep(1); fill("Amount", "400"); modal("Update") if s.js(MODAL_CLICK_JS.replace("b.click(); return true;", "return true;"), "Update") else modal("Save")
    check("edit expense", bal("QA Petty Cash") == 4600, str(bal("QA Petty Cash")))
    s.js("window.confirm=()=>true; const r=[...document.querySelectorAll('tbody tr')].find(r=>r.innerText.includes('QA lunch')); [...r.querySelectorAll('button')].pop().click();")
    time.sleep(1.5)
    check("delete expense", bal("QA Petty Cash") == 5000 and not find("get_transactions", "note", "QA lunch", {"filter": {}}))
    click("Add Transaction"); modal("Transfer")
    fill("Transfer From Account", "QA Petty Cash"); fill("Transfer To Account", "HDFC Savings"); fill("Amount", "1000"); fill("Note", "QA transfer")
    before = bal("HDFC Savings"); modal("Record Transaction")
    check("transfer", bal("QA Petty Cash") == 4000 and bal("HDFC Savings") == before + 1000)
    s.js("""const i=document.querySelector('main input[placeholder^="Search"]');
            Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'QA transfer');
            i.dispatchEvent(new Event('input',{bubbles:true}));""")
    time.sleep(1.5)
    rows = s.js("return [...document.querySelectorAll('tbody tr')].map(r=>r.innerText).filter(t=>t.trim())")
    check("search", len(rows) == 1 and "QA transfer" in rows[0], f"{len(rows)} rows")


@step
def categories_budgets_goals():
    go("Categories"); click("Add Category"); fill("Category Name", "QA Pets"); modal("Create Category")
    check("create category", find("get_categories", "name", "QA Pets") is not None)
    go("Budgets"); click("Create Budget"); fill("Budget Name", "QA Everything"); fill("Budget Limit Amount", "500000")
    modal("Select All"); modal("Create Budget")
    b = find("get_budgets", "name", "QA Everything")
    # Its first period starts today (the form default), so nothing is spent in it yet.
    check("create budget", b and len(b["category_ids"]) >= 5 and b["spent"] == 0 and b["rollover"] is False)
    go("Financial Goals"); click("New Goal"); fill("Goal Name", "QA Laptop"); fill("Target Amount", "100000")
    fill("Initial Saved Amount", "10000"); modal("Create Goal")
    g = find("get_goals", "name", "QA Laptop")
    check("create goal", g and g["current_amount"] == 10000)
    s.js("""const t=[...document.querySelectorAll('main *')].find(e=>e.childElementCount===0&&e.textContent.trim()==='QA Laptop');
            let c=t; while(c && ![...c.querySelectorAll('button')].some(b=>/^Deposit$/.test(b.innerText))) c=c.parentElement;
            [...c.querySelectorAll('button')].find(b=>/^Deposit$/.test(b.innerText)).click();""")
    time.sleep(1); fill("Amount", "15000"); modal("Add Deposit")
    check("goal deposit", find("get_goals", "name", "QA Laptop")["current_amount"] == 25000)


@step
def bills():
    go("Bills"); click("Add Bill"); fill("Bill Name", "QA Gym"); fill("Amount", "1500"); fill("Pay From Account", "QA Petty Cash")
    modal("Monthly"); modal("Add Bill")
    a = bal("QA Petty Cash")
    s.js("""window.confirm=()=>true; const t=[...document.querySelectorAll('main *')].find(e=>e.childElementCount===0&&e.textContent.trim()==='QA Gym');
            let c=t; while(c && ![...c.querySelectorAll('button')].some(b=>/Mark as Paid/.test(b.innerText))) c=c.parentElement;
            [...c.querySelectorAll('button')].find(b=>/Mark as Paid/.test(b.innerText)).click();""")
    time.sleep(1.5)
    check("pay bill", bal("QA Petty Cash") == a - 1500 and len([b for b in s.invoke("get_bills") if b["name"] == "QA Gym"]) == 2)
    s.js("[...document.querySelectorAll('main button')].find(b=>/^Paid History/.test(b.innerText.trim())).click()"); time.sleep(1)
    s.js("""window.confirm=()=>true; const t=[...document.querySelectorAll('main *')].find(e=>e.childElementCount===0&&e.textContent.trim()==='QA Gym');
            let c=t; while(c && c.querySelectorAll('button').length===0) c=c.parentElement;
            [...c.querySelectorAll('button')].find(b=>/undo/i.test(b.innerText+' '+b.title)).click();""")
    time.sleep(1.5)
    check("undo bill payment", bal("QA Petty Cash") == a and len([b for b in s.invoke("get_bills") if b["name"] == "QA Gym"]) == 1)


@step
def investments_debts_recurring():
    go("Investments"); click("Add Holding"); fill("Investment Account", "Zerodha"); fill("Symbol", "TCS"); fill("Quantity", "10")
    fill("Average Buy Price", "3500"); fill("Current / Initial Price", "3900"); modal("Add Holding")
    h = find("get_holdings", "symbol", "TCS")
    check("create holding", h and h["unrealized_pnl"] == 4000)
    go("Debts & Loans"); click("Add Loan / Facility"); fill("Facility / Loan Name", "QA Friend Loan"); fill("Loan Type", "Friend")
    fill("Sanctioned Limit", "20000"); modal("Full (100%)"); modal("Create Facility")
    d = find("get_debts", "name", "QA Friend Loan")
    check("create loan", d and d["current_balance"] == 20000 and d["min_payment"] is None)
    go("Recurring Rules"); click("Add Recurring Rule"); fill("Rule Name", "QA Streaming"); fill("Account", "QA Petty Cash")
    fill("Amount", "199"); fill("Next Due Date", (dt.date.today() - dt.timedelta(days=1)).isoformat()); modal("Create Rule")
    a = bal("QA Petty Cash"); click("Process Due Now"); time.sleep(1)
    check("recurring rule posts", bal("QA Petty Cash") == a - 199)


@step
def household():
    go("Warranties"); click("Add Warranty"); fill("Item Name", "QA Kettle"); modal("Track Warranty")
    check("create warranty", find("get_warranties", "item_name", "QA Kettle") is not None)
    go("Shopping List")
    s.js("""const i=document.querySelector('main input[placeholder^="Add item"]');
            Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(i,'QA Bread');
            i.dispatchEvent(new Event('input',{bubbles:true}));""")
    click("Add Item")
    check("shopping item", find("get_shopping_items", "name", "QA Bread") is not None)


@step
def csv_import():
    go("CSV Import")
    s.js("""const t=document.querySelector('main textarea');
            Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(t,arguments[0]);
            t.dispatchEvent(new Event('input',{bubbles:true}));""",
         f"Date,Amount,Type,Category,Note\n{TODAY},250,expense,QA Imported,Coffee\n{TODAY},5000,income,Salary,Bonus")
    time.sleep(1.5)
    s.js("""const acc=[...document.querySelectorAll('main select')].find(x=>[...x.options].some(o=>o.text.includes('QA Petty Cash')));
            const o=[...acc.options].find(o=>o.text.includes('QA Petty Cash'));
            Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(acc,o.value);
            acc.dispatchEvent(new Event('change',{bubbles:true}));""")
    a = bal("QA Petty Cash")
    s.js("[...document.querySelectorAll('main button')].filter(b=>/^Import \\d/.test(b.innerText.trim())).pop().click()")
    time.sleep(2)
    check("csv import", bal("QA Petty Cash") == a + 4750)


@step
def views_and_tools():
    go("Reports")
    for tab in ["Income vs Expense Trend", "Net Worth Trend", "Investment Performance", "Spending by Category"]:
        click(tab)
    go("Calculators")
    for tab in ["SIP & Compounding", "Emergency Runway", "Loan & EMI"]:
        click(tab)
    check("reports + calculators render", "Monthly EMI" in s.text() and not s.errors(), str(s.errors()))
    go("Dashboard")
    s.js("window.dispatchEvent(new KeyboardEvent('keydown',{key:'k',ctrlKey:true,bubbles:true}))"); time.sleep(0.6)
    opened = "Command Launcher" in s.text()
    s.js("window.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))"); time.sleep(0.4)
    check("command palette", opened)
    click("Privacy"); masked = "Masked" in s.text(); click("Masked")
    check("privacy mode", masked)
    t0 = s.invoke("get_app_settings")["theme"]
    click("Day Mode" if t0 == "light" else "Night Mode"); time.sleep(0.6)
    check("theme switch", s.invoke("get_app_settings")["theme"] != t0)
    click("Day Mode" if t0 == "dark" else "Night Mode")


@step
def backup_export_pin():
    go("Settings"); n = len(s.invoke("get_backups_list")); click("Backup Now"); time.sleep(1.5)
    check("manual backup", len(s.invoke("get_backups_list")) == n + 1)
    path = s.invoke("save_export_file", {"fileName": "lyncost_qa_export.csv", "contents": "a,b\n1,2"})
    check("csv export", os.path.exists(path)); os.remove(path)
    if not s.invoke("get_app_settings")["has_pin"]:
        s.invoke("set_initial_pin", {"pin": "135790"})
    s.js("location.reload()"); time.sleep(3)
    locked = "Enter your 6-digit PIN" in s.text()
    for d in "999999":
        s.click_text(d)
    time.sleep(2)
    wrong = "attempt" in s.text().lower()
    s.unlock("135790")
    s.js("window.confirm=()=>true")
    check("PIN lock / wrong PIN / unlock", locked and wrong and "Enter your 6-digit PIN" not in s.text())


@step
def smooth_navigation():
    pages = ["Transactions", "Accounts", "Categories", "Budgets", "Financial Goals", "Bills", "Shopping List", "Warranties",
             "CSV Import", "Recurring Rules", "Investments", "Debts & Loans", "Calculators", "Reports", "Settings", "Dashboard"]
    bad = []
    for p in pages:
        log = s.js_async(FRAMES_JS, p) if hasattr(s, "js_async") else None
        if log is None:
            from wd import req
            log = req("POST", s.p("/execute/async"), {"script": FRAMES_JS, "args": [p]})
        blank = sum(1 for has_text, _ in log if not has_text)
        faded = sum(1 for has_text, op in log if has_text and op < 0.99)
        if blank or faded:
            bad.append(f"{p}: {blank} blank / {faded} faded frames")
    check("no flicker on page switches", not bad, "; ".join(bad))


fails = [r for r in results if not r[0]]
print(f"\n{len(results) - len(fails)} passed, {len(fails)} failed; JS errors: {s.errors()}")
