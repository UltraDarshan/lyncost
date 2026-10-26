"""Find layout defects on every page, both themes, at several window sizes.

Reports:
  wrap     short labels (<= 4 words) broken across lines, e.g. a "Grade B Stable Growth" pill
  overflow text sticking out of its card/box
  clipped  text cut off inside its own box (without an intentional ellipsis)

    python3 scripts/qa/layout_check.py [pin]
"""
import json
import sys
import time

from capture import PAGES
from wd import Session, req, QA

CHECK_JS = r"""
const out = [];
const seen = new Set();
const main = document.querySelector('main');
const all = [...main.querySelectorAll('*')].filter(e => e instanceof HTMLElement && e.offsetParent !== null);
for (const e of all) {
  // Leaf-ish text holders: no element children that themselves hold text.
  const txt = (e.innerText || '').trim().replace(/\s+/g, ' ');
  if (!txt || [...e.children].some(c => (c.innerText || '').trim())) continue;
  const cs = getComputedStyle(e);
  const fs = parseFloat(cs.fontSize);
  const lh = parseFloat(cs.lineHeight) || fs * 1.35;
  const r = e.getBoundingClientRect();
  if (r.width === 0 || r.height === 0) continue;
  const words = txt.split(' ').length;
  const key = (k) => { const id = k + ':' + txt; if (!seen.has(id)) { seen.add(id); out.push([k, txt.slice(0, 40)]); } };
  // Short label wrapped onto 2+ lines (use a Range to count real line boxes).
  if (words >= 2 && words <= 4 && txt.length <= 32) {
    // Measure text nodes only (icons beside the text are not "lines").
    const tops = [];
    const walker = document.createTreeWalker(e, NodeFilter.SHOW_TEXT);
    for (let n = walker.nextNode(); n; n = walker.nextNode()) {
      if (!n.textContent.trim()) continue;
      const range = document.createRange(); range.selectNodeContents(n);
      for (const q of range.getClientRects()) if (q.width > 0) tops.push(q.top);
    }
    tops.sort((a, b) => a - b);
    const lines = tops.filter((t, i) => i === 0 || t - tops[i - 1] > fs * 0.6).length;
    if (lines > 1) key('wrap');
  }
  // Clipped inside own box, unless it is an intentional ellipsis.
  if (e.scrollWidth > e.clientWidth + 1 && cs.overflow !== 'visible' && cs.textOverflow !== 'ellipsis') key('clipped');
  // Sticking out of the nearest card.
  const card = e.closest('[class*="rounded-2xl"], [class*="rounded-xl"]');
  let scroller = e.parentElement;
  while (scroller && scroller !== card && !/(auto|scroll)/.test(getComputedStyle(scroller).overflowX)) scroller = scroller.parentElement;
  const inScroller = (scroller && scroller !== card) || (card && /(auto|scroll)/.test(getComputedStyle(card).overflowX));
  if (card && card !== e && !inScroller) {
    const cr = card.getBoundingClientRect();
    if (r.right > cr.right + 1 || r.left < cr.left - 1) key('overflow');
  }
}
// Text must stay inside its own grid column (the loan card's amounts once ran into each other).
for (const g of document.querySelectorAll('main .grid')) {
  for (const cell of g.children) {
    const c = cell.getBoundingClientRect();
    if (!c.width) continue;
    for (const el of cell.querySelectorAll('span, p')) {
      const r = el.getBoundingClientRect();
      if (r.width && el.children.length === 0 && r.right > c.right + 2)
        out.push(['spill', (el.innerText || '').trim().slice(0, 40)]);
    }
  }
}
return out;
"""

SIZES = [(1366, 768), (1100, 720), (1024, 720), (900, 650)]


def main():
    pin = sys.argv[1] if len(sys.argv) > 1 else ""
    s = Session.restart()
    s.unlock(pin)
    found = {}
    for w, h in SIZES:
        req("POST", s.p("/window/rect"), {"width": w, "height": h})
        time.sleep(0.6)
        for _ in range(2):
            theme = s.invoke("get_app_settings")["theme"]
            for page in PAGES:
                s.click_text(page)
                time.sleep(1.0)
                # Check the whole scroll height, not just the first screen.
                height = s.js("return document.querySelector('main').scrollHeight")
                for y in range(0, height, 600):
                    s.js("document.querySelector('main').scrollTo(0, arguments[0])", y)
                    time.sleep(0.15)
                    for kind, txt in s.js(CHECK_JS):
                        found.setdefault((kind, page, txt), set()).add(f"{w}x{h}/{theme}")
                s.js("document.querySelector('main').scrollTo(0,0)")
            s.click_text("Day Mode" if theme == "light" else "Night Mode")
            time.sleep(0.8)
    for (kind, page, txt), where in sorted(found.items()):
        print(f"{kind:8s} {page:16s} {txt!r:44s} {', '.join(sorted(where))}")
    print(f"\n{len(found)} issue(s); JS errors: {s.errors()}")
    json.dump([[k, p, t, sorted(w)] for (k, p, t), w in found.items()], open(QA / "layout.json", "w"))


if __name__ == "__main__":
    main()
