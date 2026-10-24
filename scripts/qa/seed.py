"""Fill a fresh QA profile with realistic demo data through the app's own IPC.

    sh scripts/qa/env.sh reset && python3 scripts/qa/seed.py
"""
import datetime as dt
import time

from wd import Session


def d(days):
    return (dt.date.today() + dt.timedelta(days=days)).isoformat()


def main():
    s = Session.restart()
    ok = s.invoke
    cats = {c["name"]: c["id"] for c in ok("get_categories")}

    def acc(name, kind, cur, opening):
        return ok("create_account", {"payload": {"name": name, "type": kind, "currency": cur, "opening_balance": opening}})["id"]

    hdfc = acc("HDFC Savings", "bank", "INR", 245000)
    cash = acc("Cash Wallet", "cash", "INR", 18500)
    card = acc("Amex Platinum", "credit_card", "INR", 0)
    zerodha = acc("Zerodha", "investment", "INR", 50000)
    ibkr = acc("Interactive Brokers", "investment", "USD", 1200)

    def txn(account, kind, amount, cat, note, days, tags=(), confirmed=True, to=None, pay="upi"):
        ok("create_transaction", {"payload": {
            "account_id": account, "type": kind, "category_id": cats.get(cat), "transfer_to_account_id": to,
            "amount": amount, "payment_type": pay, "txn_date": d(days), "note": note,
            "is_confirmed": confirmed, "tags": list(tags)}})

    for m in range(5, -1, -1):
        txn(hdfc, "income", 185000, "Salary", "Monthly salary", -30 * m - 1, ["essential"], pay="bank_transfer")
        txn(hdfc, "expense", 42000, "Rent", "Apartment rent", -30 * m - 2, ["essential"], pay="bank_transfer")
        txn(card, "expense", 6800 + m * 450, "Food", "Restaurants & delivery", -30 * m - 6, ["discretionary"], pay="credit_card")
        txn(hdfc, "expense", 3200 + m * 120, "Bills & Utilities", "Electricity + fibre", -30 * m - 9, pay="bank_transfer")
        txn(cash, "expense", 1800 + m * 90, "Transport", "Metro & cabs", -30 * m - 12, pay="cash")
    txn(hdfc, "expense", 24990, "Shopping", "Noise-cancelling headphones", -3, ["personal"], pay="credit_card")
    txn(hdfc, "income", 32000, "Business Income", "Freelance design project", -4, pay="bank_transfer")
    txn(hdfc, "transfer", 20000, None, "Top up brokerage", -5, to=zerodha, pay="bank_transfer")
    txn(card, "expense", 2450, "Entertainment", "Concert tickets", -1, ["discretionary"], confirmed=False, pay="credit_card")

    start = d(0)[:8] + "01"
    for name, amount, cat_names, rollover in [("Dining out", 9000, ["Food"], True), ("Getting around", 2500, ["Transport"], False),
                                              ("Fun money", 30000, ["Entertainment", "Shopping"], False)]:
        ok("create_budget", {"payload": {"name": name, "amount": amount, "period": "monthly", "start_date": start,
                                         "rollover": rollover, "category_ids": [cats[c] for c in cat_names]}})
    ok("create_goal", {"payload": {"name": "Emergency fund", "target_amount": 600000, "current_amount": 410000, "target_date": d(240), "color": "#10b981"}})
    ok("create_goal", {"payload": {"name": "Japan trip", "target_amount": 250000, "current_amount": 95000, "target_date": d(150)}})
    ok("create_bill", {"payload": {"name": "Credit card statement", "amount": 18250, "due_date": d(2), "account_id": hdfc,
                                   "category_id": cats["Bills & Utilities"], "recurrence": "monthly"}})
    ok("create_bill", {"payload": {"name": "Netflix", "amount": 649, "due_date": d(9), "account_id": card,
                                   "category_id": cats["Entertainment"], "recurrence": "monthly"}})
    debt = ok("create_debt", {"payload": {"name": "Car loan", "principal": 800000, "initial_draw": 800000, "debt_type": "mortgage_auto",
                                          "interest_rate": 8.9, "due_date": d(12), "min_payment": 16500, "notes": "SBI, 60 months"}})
    ok("record_debt_payment", {"payload": {"debt_id": debt["id"], "amount": 165000, "txn_date": d(-20)}})
    for sym, name, typ, acc_id, qty, avg, cur, last in [
        ("NIFTYBEES", "Nippon Nifty 50 ETF", "mutual_fund", zerodha, 1200, 238.5, "INR", 271.4),
        ("RELIANCE", "Reliance Industries", "stock", zerodha, 40, 2410, "INR", 2876),
        ("AAPL", "Apple Inc.", "stock", ibkr, 12, 168.2, "USD", 231.6),
        ("BTC", "Bitcoin", "crypto", ibkr, 0.05, 41000, "USD", 67800),
    ]:
        hid = ok("create_holding", {"payload": {"account_id": acc_id, "asset_type": typ, "symbol": sym, "name": name,
                                                "quantity": qty, "avg_buy_price": avg, "currency": cur, "last_price": avg}})
        ok("update_holding_price", {"payload": {"holding_id": hid, "price": last}})
    ok("create_warranty", {"payload": {"item_name": "MacBook Air", "purchase_date": d(-340), "expires_on": d(25)}})
    ok("record_net_worth_snapshot", {"snapshotDate": None})
    s.js("localStorage.setItem('lyncost_wizard_completed','true'); location.reload()")
    time.sleep(3)
    print("seeded")


if __name__ == "__main__":
    main()
