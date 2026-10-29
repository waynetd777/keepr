# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Checks that a burst of launches leaves one Keepr running (Windows).

    python tools/single_instance_burst.py --exe src-tauri/target/release/Keepr.exe [--count 8] [--wait 30]

Starts the app `--count` times as fast as the shell can (a few hundred milliseconds for eight), waits
`--wait` seconds for the copies to start or hand over (src-tauri/src/single_instance_win.rs), then
counts the Keepr.exe processes that are the app (not Keepr Helper, `--helper-…`, nor a command-line
job). Passes when exactly one copy is left. `--exe` is a release build (`npx tauri build`): a debug
build doesn't take the lock, nor does a copy started with `KEEPR_DATA` set. The default is the
installed app. Quit Keepr first, and quit the copy that's left afterwards (Quit Keepr in the
notification-area panel).
"""

import argparse
import os
import subprocess
import sys
import time
from pathlib import Path

PS = ["powershell", "-NoProfile", "-Command"]


def app_processes() -> list[tuple[int, str]]:
    """(pid, command line) of every Keepr.exe that is the app: started with no arguments or with
    --login, not Keepr Helper (--helper-…) or a command-line job (a subcommand)."""
    out = subprocess.run(
        PS + ['Get-CimInstance Win32_Process -Filter "Name=\'Keepr.exe\'" | ForEach-Object { "$($_.ProcessId)`t$($_.CommandLine)" }'],
        capture_output=True,
        text=True,
        check=False,
    ).stdout
    found = []
    for line in out.splitlines():
        pid, _, cmd = line.partition("\t")
        if not pid.strip().isdigit():
            continue
        # The program, quoted or not, then its arguments.
        rest = cmd.strip()
        rest = rest[rest.index('"', 1) + 1 :] if rest.startswith('"') else rest.partition(" ")[2]
        if rest.split() in ([], ["--login"]):
            found.append((int(pid), cmd.strip()))
    return found


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--exe", default=str(Path(os.environ["LOCALAPPDATA"]) / "Keepr" / "Keepr.exe"))
    ap.add_argument("--count", type=int, default=8)
    ap.add_argument("--wait", type=float, default=30)
    a = ap.parse_args()

    if os.environ.get("KEEPR_DATA"):
        print("KEEPR_DATA is set, so the copies don't take the lock; unset it first")
        return 2
    exe = str(Path(a.exe).resolve())
    before = app_processes()
    if before:
        print(f"already running before the burst: {len(before)} copy(ies) {[p for p, _ in before]}; quit it first")
        return 2

    t0 = time.perf_counter()
    for _ in range(a.count):
        subprocess.Popen([exe], close_fds=True)
    print(f"launched {a.count} copies of {exe} in {time.perf_counter() - t0:.2f} s")

    time.sleep(a.wait)
    live = app_processes()
    print(f"after {a.wait:g} s: {len(live)} live copy(ies) {[p for p, _ in live]}")
    ok = len(live) == 1
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
