#!/usr/bin/env python3
"""Verify every invoke() in the built frontend matches a registered Rust command.

Checks that (1) each command the frontend calls is registered in lib.rs, (2) the
argument keys it passes match the Rust parameter names, and (3) every
required (non-Option) Rust parameter is actually sent. Run from the
repository root:  python3 scripts/check-ipc-contract.py
"""
import glob
import re
import sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else "."
IGNORED_PARAMS = {"state", "app"}

def frontend_calls():
    calls = {}
    for path in glob.glob(f"{ROOT}/dist/assets/*.js"):
        src = open(path, encoding="utf-8").read()
        # The bundle aliases invoke as a short identifier: ae("cmd") / ae("cmd",{k:v})
        for m in re.finditer(r'\b[A-Za-z_$]{1,3}\("([a-z]+(?:_[a-z]+)+)"(?:,\{([^{}]*)\})?\)', src):
            cmd, body = m.group(1), m.group(2)
            keys = set(re.findall(r'(?:^|,)\s*([A-Za-z_$][\w$]*)\s*:', body)) if body else set()
            calls.setdefault(cmd, set()).update(keys)
    return calls

def rust_commands():
    registered = set(re.findall(r'^\s+\w+::(\w+),$', open(f"{ROOT}/src-tauri/src/lib.rs").read(), re.M))
    params = {}
    for path in glob.glob(f"{ROOT}/src-tauri/src/commands/*.rs"):
        src = open(path, encoding="utf-8").read()
        for m in re.finditer(r'#\[tauri::command\][^\n]*\n(?:#\[[^\n]*\n)*pub (?:async )?fn (\w+)(?:<[^>]*>)?\(([^)]*)\)', src):
            args = [a.split(":", 1) for a in m.group(2).split(",") if ":" in a]
            params[m.group(1)] = {
                name.strip(): not ty.strip().startswith("Option<")
                for name, ty in args
                if name.strip() not in IGNORED_PARAMS
            }
    return registered, params

def main():
    calls = frontend_calls()
    registered, params = rust_commands()
    known = set(params)
    problems = []
    for cmd, keys in sorted(calls.items()):
        if cmd not in known:
            continue  # snake_case string that is not a command (e.g. a CSS class or enum value)
        if cmd not in registered:
            problems.append(f"{cmd}: defined but not registered in lib.rs")
        expected = set(params[cmd])
        unknown = keys - expected
        if unknown:
            problems.append(f"{cmd}: frontend sends {sorted(unknown)}, Rust expects {sorted(expected)}")
        required = {name for name, req in params[cmd].items() if req}
        if required - keys:
            problems.append(f"{cmd}: Rust requires {sorted(required - keys)} but the frontend never sends it")
    invoked = {c for c in calls if c in known}
    unused = sorted(known - invoked)
    print(f"{len(invoked)} commands invoked by the frontend, {len(registered)} registered.")
    if unused:
        print("Registered but never invoked:", ", ".join(unused))
    for p in problems:
        print("MISMATCH:", p)
    sys.exit(1 if problems else 0)

if __name__ == "__main__":
    main()
