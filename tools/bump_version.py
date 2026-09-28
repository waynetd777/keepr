"""Bumps the app's version for a release build: the last number of tauri.conf.json's version
(0.1.0 → 0.1.1), written to package.json and src-tauri/Cargo.toml (and both lock files) too, so
they agree. Prints the new version.

    python3 tools/bump_version.py            # 0.1.0 → 0.1.1
    python3 tools/bump_version.py 1.1.0      # set it (a minor or major step is chosen by hand)

`make app` runs it before every release build. The build number (CFBundleVersion) is separate: the
Makefile stamps one per build on the app, its binary.
"""
import json, re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CONF, PACKAGE, NPMLOCK, CARGO, LOCK = (ROOT / p for p in ("src-tauri/tauri.conf.json", "package.json", "package-lock.json",
                                                            "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"))


def replace(path, pattern, new, count=1):
    text = path.read_text()
    out, n = re.subn(pattern, new, text, count=count, flags=re.M)
    if n != count:
        sys.exit(f"bump_version: no version found in {path.relative_to(ROOT)}")
    path.write_text(out)


def main():
    old = json.loads(CONF.read_text())["version"]
    if len(sys.argv) > 1:
        new = sys.argv[1]
        if not re.fullmatch(r"\d+\.\d+\.\d+", new):
            sys.exit(f"bump_version: {new!r} is not major.minor.patch")
    else:
        major, minor, patch = (int(x) for x in old.split("."))
        new = f"{major}.{minor}.{patch + 1}"
    replace(CONF, r'^(  "version": )"[^"]+"', rf'\1"{new}"')
    replace(PACKAGE, r'^(  "version": )"[^"]+"', rf'\1"{new}"')
    if NPMLOCK.exists():  # the lock's own entry, and its root package's
        replace(NPMLOCK, r'^(  "name": "keepr",\n  "version": )"[^"]+"', rf'\1"{new}"')
        replace(NPMLOCK, r'^(      "name": "keepr",\n      "version": )"[^"]+"', rf'\1"{new}"')
    replace(CARGO, r'^(version = )"[^"]+"', rf'\1"{new}"')
    if LOCK.exists():
        replace(LOCK, r'^(name = "keepr"\nversion = )"[^"]+"', rf'\1"{new}"')
    print(new)


if __name__ == "__main__":
    main()
