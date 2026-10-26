"""Screenshot pages in both themes: python3 scripts/qa/capture.py <prefix> [all|page,page,...] [pin]"""
import sys
import time

from wd import Session

PAGES = ["Dashboard", "Transactions", "Accounts", "Budgets", "Financial Goals", "Bills", "Investments",
         "Debts & Loans", "Reports", "Settings", "Recurring Rules", "Warranties", "Shopping List",
         "Categories", "CSV Import", "Calculators"]


def main():
    prefix = sys.argv[1] if len(sys.argv) > 1 else "page"
    pages = sys.argv[2].split(",") if len(sys.argv) > 2 and sys.argv[2] != "all" else PAGES
    s = Session.restart()
    s.unlock(sys.argv[3] if len(sys.argv) > 3 else "")
    for _ in range(2):
        theme = s.invoke("get_app_settings")["theme"]
        for i, page in enumerate(pages):
            s.click_text(page)
            time.sleep(1.4)
            s.shot(f"{prefix}-{theme}-{i:02d}-{page.split()[0].lower()}")
        s.click_text("Day Mode" if theme == "light" else "Night Mode")
        time.sleep(1)
    print("JS errors:", s.errors())


if __name__ == "__main__":
    main()
