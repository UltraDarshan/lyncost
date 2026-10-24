#!/bin/sh
# Lyncost installer: https://github.com/UltraDarshan/lyncost
#
#   curl -fsSL https://raw.githubusercontent.com/UltraDarshan/lyncost/main/install.sh | sh
#
# Installs the latest release using the best package for this system:
#   Fedora / RHEL / openSUSE -> .rpm (dnf / zypper)
#   Debian / Ubuntu / Mint   -> .deb (apt)
#   anything else            -> AppImage in your home folder (no sudo)
#
# Options (pass after `sh -s --`):
#   --appimage        always use the AppImage, even where a package would work
#   --version X.Y.Z   install a specific release instead of the latest
#
# Every download is checked against the release's SHA256SUMS before use.

set -eu

REPO="UltraDarshan/lyncost"
APP="Lyncost"

FORCE_APPIMAGE=0
LC_VERSION=""

say() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m✗\033[0m %s\n' "$*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --appimage) FORCE_APPIMAGE=1 ;;
        --version) [ $# -ge 2 ] || die "--version needs a value"; LC_VERSION="${2#v}"; shift ;;
        --version=*) LC_VERSION="${1#--version=}"; LC_VERSION="${LC_VERSION#v}" ;;
        -h|--help) sed -n '2,17p' "$0" 2>/dev/null || true; exit 0 ;;
        *) die "Unknown option: $1" ;;
    esac
    shift
done

[ "$(uname -s)" = "Linux" ] || die "$APP for $(uname -s) is not available from this script yet."
case "$(uname -m)" in
    x86_64|amd64) ;;
    *) die "$APP currently ships for x86_64 only (this machine is $(uname -m))." ;;
esac

need() { command -v "$1" >/dev/null 2>&1; }
need curl || die "curl is required."
need sha256sum || die "sha256sum (coreutils) is required."

if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
elif need sudo; then
    SUDO="sudo"
else
    SUDO=""
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

fetch() { curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --connect-timeout 15 "$@"; }

# --- Which release ------------------------------------------------------------

if [ -z "$LC_VERSION" ]; then
    say "Finding the latest $APP release"
    LC_VERSION="$(fetch "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)"
    [ -n "$LC_VERSION" ] || die "Could not determine the latest release. Check your internet connection."
fi
BASE="https://github.com/$REPO/releases/download/v$LC_VERSION"
say "Installing $APP $LC_VERSION"

fetch -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS" || die "Release v$LC_VERSION not found."

download() { # download <file>: fetch and verify against SHA256SUMS
    say "Downloading $1"
    fetch -o "$TMP/$1" "$BASE/$1" || die "Download failed: $1"
    (cd "$TMP" && grep -E "  \*?$1\$" SHA256SUMS | sha256sum -c --status -) \
        || die "Checksum mismatch for $1. The file may be corrupted; please try again."
    ok "Checksum verified"
}

# --- Pick the install method --------------------------------------------------

# Read only ID / ID_LIKE, in a subshell: os-release also defines VERSION and
# other common names that must not leak into this script.
family=""
if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    family="$(. /etc/os-release && echo "${ID:-} ${ID_LIKE:-}")"
fi

method="appimage"
if [ "$FORCE_APPIMAGE" -eq 0 ] && { [ "$(id -u)" -eq 0 ] || [ -n "$SUDO" ]; }; then
    case "$family" in
        *fedora*|*rhel*|*centos*|*rocky*|*alma*) need dnf && method="dnf" ;;
        *suse*) need zypper && method="zypper" ;;
        *debian*|*ubuntu*) need apt-get && method="apt" ;;
    esac
fi

as_root() {
    if [ -n "$SUDO" ]; then
        say "Administrator permission is needed to install the package"
        $SUDO "$@"
    else
        "$@"
    fi
}

# --- Install ------------------------------------------------------------------

install_appimage() {
    file="${APP}_${LC_VERSION}_amd64.AppImage"
    download "$file"

    data="${XDG_DATA_HOME:-$HOME/.local/share}"
    dir="$data/lyncost"
    bin="$HOME/.local/bin"
    mkdir -p "$dir" "$bin" "$data/applications" "$data/icons/hicolor/512x512/apps"

    install -m 0755 "$TMP/$file" "$dir/$APP.AppImage"

    # Launcher: AppImages need FUSE; fall back to extract-and-run without it.
    cat > "$bin/lyncost" <<EOF
#!/bin/sh
if ! command -v fusermount >/dev/null 2>&1 && ! command -v fusermount3 >/dev/null 2>&1; then
    export APPIMAGE_EXTRACT_AND_RUN=1
fi
exec "$dir/$APP.AppImage" "\$@"
EOF
    chmod 0755 "$bin/lyncost"

    if fetch -o "$TMP/icon.png" "https://raw.githubusercontent.com/$REPO/v$LC_VERSION/src-tauri/icons/icon.png"; then
        install -m 0644 "$TMP/icon.png" "$data/icons/hicolor/512x512/apps/lyncost.png"
    fi

    cat > "$data/applications/lyncost.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP
GenericName=Personal Finance Tracker
Comment=Offline, privacy-first personal finance & investment tracker
Exec=$bin/lyncost
Icon=lyncost
Terminal=false
StartupNotify=true
StartupWMClass=lyncost
Categories=Office;Finance;
Keywords=finance;budget;money;expense;income;investment;net worth;bills;offline;
EOF
    # Marker so the uninstaller knows this was a user-level AppImage install.
    printf '%s\n' "$LC_VERSION" > "$dir/installed-version"

    need update-desktop-database && update-desktop-database "$data/applications" >/dev/null 2>&1 || true
    need gtk-update-icon-cache && gtk-update-icon-cache -q -t "$data/icons/hicolor" >/dev/null 2>&1 || true

    ok "$APP $LC_VERSION installed to $dir"
    case ":$PATH:" in
        *":$bin:"*) ;;
        *) warn "$bin is not on your PATH; start $APP from your app menu, or add it to PATH." ;;
    esac
}

case "$method" in
    dnf)
        file="${APP}-${LC_VERSION}-1.x86_64.rpm"
        download "$file"
        as_root dnf install -y "$TMP/$file"
        ;;
    zypper)
        file="${APP}-${LC_VERSION}-1.x86_64.rpm"
        download "$file"
        as_root zypper --non-interactive install --allow-unsigned-rpm "$TMP/$file"
        ;;
    apt)
        file="${APP}_${LC_VERSION}_amd64.deb"
        download "$file"
        chmod 0644 "$TMP/$file"
        # apt runs downloads as the _apt user, which cannot read our private temp dir.
        chmod 0755 "$TMP"
        as_root apt-get install -y "$TMP/$file"
        ;;
    appimage)
        install_appimage
        ;;
esac

ok "Done. Open $APP from your applications menu."
echo "   To uninstall later: curl -fsSL https://raw.githubusercontent.com/$REPO/main/uninstall.sh | sh"
