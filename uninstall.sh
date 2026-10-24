#!/bin/sh
# Lyncost uninstaller: https://github.com/UltraDarshan/lyncost
#
#   curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/uninstall.sh | sh
#
# Removes Lyncost however it was installed (rpm, deb or AppImage).
# Your financial data and backups are KEPT unless you ask to delete them:
#
#   curl -fsSL .../uninstall.sh | sh -s -- --purge
#
# --purge asks for confirmation; add --yes to skip the question.

set -eu

APP="Lyncost"
PURGE=0
ASSUME_YES=0

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --purge) PURGE=1 ;;
        -y|--yes) ASSUME_YES=1 ;;
        -h|--help) sed -n '2,13p' "$0" 2>/dev/null || true; exit 0 ;;
        *) die "Unknown option: $1" ;;
    esac
    shift
done

need() { command -v "$1" >/dev/null 2>&1; }

if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
elif need sudo; then
    SUDO="sudo"
else
    SUDO=""
fi

as_root() {
    if [ "$(id -u)" -ne 0 ] && [ -z "$SUDO" ]; then
        die "Administrator rights are needed to remove the system package (sudo not found)."
    fi
    [ -n "$SUDO" ] && say "Administrator permission is needed to remove the package"
    $SUDO "$@"
}

if pgrep -x lyncost >/dev/null 2>&1; then
    warn "$APP is running; it will keep running until you close it."
fi

removed=0

# System package (rpm or deb)
if need rpm && rpm -q lyncost >/dev/null 2>&1; then
    say "Removing the $APP package (rpm)"
    if need dnf; then as_root dnf remove -y lyncost
    elif need zypper; then as_root zypper --non-interactive remove lyncost
    else as_root rpm -e lyncost
    fi
    removed=1
elif need dpkg-query && [ "$(dpkg-query -W -f='${Status}' lyncost 2>/dev/null || true)" = "install ok installed" ]; then
    say "Removing the $APP package (deb)"
    as_root apt-get remove -y lyncost
    removed=1
fi

# User-level AppImage install
data="${XDG_DATA_HOME:-$HOME/.local/share}"
if [ -e "$data/lyncost/$APP.AppImage" ] || [ -e "$HOME/.local/bin/lyncost" ] || [ -e "$data/applications/lyncost.desktop" ]; then
    say "Removing the $APP AppImage from your home folder"
    rm -f "$data/lyncost/$APP.AppImage" "$data/lyncost/installed-version"
    rmdir "$data/lyncost" 2>/dev/null || true
    # Only remove our own launcher, never an unrelated `lyncost` file.
    if [ -f "$HOME/.local/bin/lyncost" ] && grep -q "$APP.AppImage" "$HOME/.local/bin/lyncost" 2>/dev/null; then
        rm -f "$HOME/.local/bin/lyncost"
    fi
    rm -f "$data/applications/lyncost.desktop" "$data/icons/hicolor/512x512/apps/lyncost.png"
    need update-desktop-database && update-desktop-database "$data/applications" >/dev/null 2>&1 || true
    removed=1
fi

if [ "$removed" -eq 1 ]; then
    ok "$APP has been uninstalled."
else
    warn "$APP does not appear to be installed."
fi

# Data lives in the app's data folder; WebKit keeps its cache next to it.
datadir="$data/com.lyncost.desktop"
cachedir="${XDG_CACHE_HOME:-$HOME/.cache}/com.lyncost.desktop"

if [ "$PURGE" -eq 1 ]; then
    if [ -d "$datadir" ] || [ -d "$cachedir" ]; then
        if [ "$ASSUME_YES" -ne 1 ]; then
            # Read from the terminal: stdin is the script itself under `curl | sh`.
            if [ -r /dev/tty ]; then
                printf '\033[1;31m!\033[0m This permanently deletes ALL your Lyncost data and backups in:\n    %s\n  Type DELETE to confirm: ' "$datadir"
                read -r answer < /dev/tty || answer=""
            else
                die "No terminal to confirm --purge; re-run with --purge --yes to delete your data."
            fi
            [ "$answer" = "DELETE" ] || { warn "Kept your data."; exit 0; }
        fi
        rm -rf "$datadir" "$cachedir"
        ok "Deleted your $APP data and backups."
    fi
elif [ -d "$datadir" ]; then
    echo "   Your data and backups are kept in $datadir"
    echo "   (reinstall to pick up where you left off, or delete with: --purge)"
fi
