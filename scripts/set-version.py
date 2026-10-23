#!/usr/bin/env python3
"""Set the Lyncost version everywhere it is recorded.

    python3 scripts/set-version.py 0.2.1

Updates package.json (read by tauri.conf.json), src-tauri/Cargo.toml, the
version labels compiled into the prebuilt frontend (dist/), and adds a release
entry to the AppStream metainfo. The frontend source was lost, so its version
labels are patched in place using the exact patterns the bundle contains.
"""
import datetime
import glob
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEMVER = re.compile(r"^\d+\.\d+\.\d+$")


def replace_exact(path: Path, pairs):
    text = path.read_text(encoding="utf-8")
    count = 0
    for old, new in pairs:
        count += text.count(old)
        text = text.replace(old, new)
    path.write_text(text, encoding="utf-8")
    return count


def main():
    if len(sys.argv) != 2 or not SEMVER.match(sys.argv[1]):
        sys.exit("usage: set-version.py X.Y.Z")
    new = sys.argv[1]

    pkg_path = ROOT / "package.json"
    pkg = json.loads(pkg_path.read_text(encoding="utf-8"))
    old = pkg["version"]
    pkg["version"] = new
    pkg_path.write_text(json.dumps(pkg, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    cargo = ROOT / "src-tauri" / "Cargo.toml"
    text = cargo.read_text(encoding="utf-8")
    cargo.write_text(re.sub(r'(?m)^version = "[^"]+"', f'version = "{new}"', text, count=1), encoding="utf-8")

    # Frontend labels: ["v","X"], ["Lyncost v","X", and the Settings fallback ||"X".
    patched = 0
    for js in glob.glob(str(ROOT / "dist" / "assets" / "*.js")):
        for prev in {old, "0.1.9"}:
            patched += replace_exact(
                Path(js),
                [
                    (f'["v","{prev}"]', f'["v","{new}"]'),
                    (f'["Lyncost v","{prev}"', f'["Lyncost v","{new}"'),
                    (f'current_version||"{prev}"', f'current_version||"{new}"'),
                ],
            )

    meta = ROOT / "packaging" / "linux" / "com.lyncost.desktop.metainfo.xml"
    text = meta.read_text(encoding="utf-8")
    if f'version="{new}"' not in text:
        today = datetime.date.today().isoformat()
        entry = f'    <release version="{new}" date="{today}"/>\n'
        text = text.replace("  <releases>\n", "  <releases>\n" + entry, 1)
        meta.write_text(text, encoding="utf-8")

    print(f"{old} -> {new}: package.json, Cargo.toml, metainfo, {patched} frontend label(s)")
    print("Next: update version.json and CHANGELOG.md, then commit and tag v" + new)


if __name__ == "__main__":
    main()
