#!/bin/sh
# Start a hidden display (Xvfb :99) and tauri-driver for UI QA.
# Test windows never appear on your screen, and the QA build uses its own app id
# (com.lyncost.qa) and data folder, so it never touches your real Lyncost.
#
#   sh scripts/qa/env.sh build   # build the QA app (debug, id com.lyncost.qa)
#   sh scripts/qa/env.sh start   # start Xvfb + tauri-driver
#   sh scripts/qa/env.sh stop
set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
QA="$ROOT/src-tauri/target/qa"
mkdir -p "$QA/data" "$QA/shots"
case "${1:-start}" in
  build)
    cd "$ROOT/src-tauri"
    TAURI_CONFIG='{"identifier":"com.lyncost.qa","productName":"LyncostQA"}' cargo build ;;
  start)
    pgrep -x Xvfb >/dev/null || (setsid nohup Xvfb :99 -screen 0 1920x1200x24 -nolisten tcp >"$QA/xvfb.log" 2>&1 &)
    sleep 1
    pgrep -x tauri-driver >/dev/null || (cd "$QA" && env -u WAYLAND_DISPLAY DISPLAY=:99 GDK_BACKEND=x11 \
        XDG_DATA_HOME="$QA/data" setsid nohup tauri-driver >"$QA/tauri-driver.log" 2>&1 &)
    sleep 2; echo "QA environment ready (data: $QA/data)" ;;
  stop)
    pkill -x tauri-driver || true; pkill -x WebKitWebDriver || true; pkill -x Xvfb || true ;;
  reset)
    rm -rf "$QA/data" && mkdir -p "$QA/data" && echo "QA data wiped" ;;
esac
