// Runs before the app (the original bundle in /assets, whose source is lost).
// 1. The backend decides the license; this copies its answer into the flags the bundle reads,
//    so a hand-edited flag is reset on every start (the backend refuses writes anyway).
// 2. On the very first run of the Windows edition, welcomes the person to their 14-day trial.
const tauri = window.__TAURI_INTERNALS__

function welcome(status) {
  const ends = new Date(Date.parse(status.trial_started_at) + status.days_total * 86400000)
  const date = ends.toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" })
  const light = localStorage.getItem("lyncost_desktop_theme") === "light"
  const wrap = document.createElement("div")
  wrap.setAttribute("role", "dialog")
  wrap.setAttribute("aria-modal", "true")
  wrap.setAttribute("aria-labelledby", "lc-welcome-title")
  wrap.style.cssText =
    "position:fixed;inset:0;z-index:2147483647;display:flex;align-items:center;justify-content:center;padding:24px;" +
    `background:${light ? "rgba(15,23,42,.35)" : "rgba(2,6,23,.7)"};backdrop-filter:blur(8px);font-family:'Plus Jakarta Sans',system-ui,sans-serif`
  wrap.innerHTML = `
    <div style="width:100%;max-width:440px;border-radius:24px;padding:32px;text-align:center;
      background:${light ? "#ffffff" : "#0f172a"};color:${light ? "#0f172a" : "#f1f5f9"};
      border:1px solid ${light ? "rgba(15,23,42,.1)" : "rgba(255,255,255,.1)"};box-shadow:0 30px 80px -20px rgba(91,91,214,.45)">
      <div style="width:64px;height:64px;margin:0 auto;border-radius:18px;display:grid;place-items:center;
        background:linear-gradient(135deg,#6e6ade,#5b5bd6);box-shadow:0 10px 30px -8px rgba(91,91,214,.8)">
        <svg width="30" height="30" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>
      </div>
      <h2 id="lc-welcome-title" style="margin:20px 0 0;font-size:22px;font-weight:800;letter-spacing:-.02em;text-wrap:balance">Congratulations! Your 14‑day free trial has started</h2>
      <p style="margin:12px 0 0;font-size:14.5px;line-height:1.6;color:${light ? "#475569" : "#94a3b8"}">
        Every feature is unlocked until <strong style="color:inherit;color:${light ? "#0f172a" : "#f1f5f9"}">${date}</strong>.
        After that, enter a license key to keep adding records. Your data always stays on this PC.</p>
      <button type="button" style="margin-top:24px;width:100%;height:46px;border:0;border-radius:14px;cursor:pointer;
        font:700 15px 'Plus Jakarta Sans',system-ui,sans-serif;color:#fff;background:linear-gradient(180deg,#6d6de6,#5050c8);
        box-shadow:0 8px 24px -8px rgba(91,91,214,.9)">Start using Lyncost</button>
    </div>`
  const button = wrap.querySelector("button")
  button.addEventListener("click", () => wrap.remove())
  wrap.addEventListener("keydown", (e) => e.key === "Escape" && wrap.remove())
  document.body.appendChild(wrap)
  button.focus()
}

if (tauri) {
  // Settings shows this version (the bundle used to have it typed in).
  window.__LYNCOST_VERSION__ = await tauri.invoke("plugin:app|version").catch(() => undefined)
  try {
    const status = await tauri.invoke("get_license_status")
    if (status.state === "licensed") {
      localStorage.setItem("lyncost_license_activated", "true")
      localStorage.setItem("lyncost_license_key", status.license_key ?? "")
      localStorage.setItem("lyncost_licensed_to", status.licensed_to ?? "")
    } else if (status.state === "trial" || status.state === "expired") {
      for (const k of ["lyncost_license_activated", "lyncost_license_key", "lyncost_licensed_to"]) localStorage.removeItem(k)
      if (status.state === "trial" && status.first_run) welcome(status)
    }
  } catch (e) {
    console.error("License check failed:", e)
  }
}

await import("/assets/index-BxBChbje.js")
