#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Retake the README and docs screenshots, from demo data: docs/images/<scene>-<theme>.png.

Makes a demo in .demo/ (gitignored): sample folders in a demo home, a backup folder as the
destination, and a drive that isn't connected. It backs the folders up a few times with `Keepr --back-up`, editing
files between runs, so there are snapshots and versions to show. Then it launches the dev build
once per scene and theme with the scene in KEEPR_SCENE (the app saves and runs nothing). The
window is invisible and takes no focus, so nothing flashes on screen: the app saves its webview's
snapshot to KEEPR_SNAPSHOT, and the window's buttons and rounded corners are drawn back on it
here. Nothing outside .demo/ is read or written, and no snapshots of the Mac are taken
(KEEPR_NO_STILL).

    python3 tools/screenshots.py                 # every scene, light and dark
    python3 tools/screenshots.py restore         # one scene
    python3 tools/screenshots.py --theme dark    # one theme
    python3 tools/screenshots.py --fresh         # remake the demo first
    python3 tools/screenshots.py -j 1            # one at a time (default: 4 side by side)
    python3 tools/screenshots.py --release       # the built app (make app)

Needs the Vite dev server (started here if it isn't running) and Pillow.
"""

import argparse
import concurrent.futures
import io
import json
import os
import pathlib
import queue
import random
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

from PIL import Image, ImageCms, ImageDraw, ImageStat

ROOT = pathlib.Path(__file__).resolve().parent.parent
HERE = ROOT / "tools" / "screenshots"
DEMO = ROOT / ".demo"
OUT = ROOT / "docs" / "images"
BIN = ROOT / "src-tauri" / "target" / "debug" / "Keepr"
RELEASE_BIN = ROOT / "src-tauri" / "target" / "release" / "bundle" / "macos" / "Keepr.app" / "Contents" / "MacOS" / "Keepr"
DEV_URL = "http://localhost:1420"
WIDTH = 1400
SETTLE = 5.0

def to_srgb(im):
    icc = im.info.get("icc_profile")
    if icc:
        alpha = im.getchannel("A") if im.mode == "RGBA" else None
        src = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        im = ImageCms.profileToProfile(im.convert("RGB"), src, ImageCms.createProfile("sRGB"), renderingIntent=ImageCms.Intent.PERCEPTUAL)
        if alpha:
            im.putalpha(alpha)
    im.info.pop("icc_profile", None)
    return im


def dev_server():
    try:
        urllib.request.urlopen(DEV_URL, timeout=1)
        return None
    except OSError:
        pass
    p = subprocess.Popen(["npx", "vite", "--port", "1420", "--strictPort"], cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(60):
        time.sleep(0.5)
        try:
            urllib.request.urlopen(DEV_URL, timeout=1)
            return p
        except OSError:
            pass
    sys.exit("the Vite dev server didn't start")


def build():
    subprocess.run(["cargo", "build", "--no-default-features"], cwd=ROOT / "src-tauri", check=True)


def env(data=DEMO / "data"):
    # HOME is the demo's, so paths show as ~/Documents and not as where this repo is on this Mac.
    return {**os.environ, "HOME": str(DEMO / "Home"), "KEEPR_DATA": str(data), "KEEPR_NO_STILL": "1"}


def write(p, data):
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data if isinstance(data, bytes) else data.encode())


def noise(n, seed):
    r = random.Random(seed)
    return bytes(r.getrandbits(8) for _ in range(n))


def make_demo():
    """Sample folders, a config with three plans, and a few backups."""
    shutil.rmtree(DEMO, ignore_errors=True)
    home = DEMO / "Home"
    docs, proj, pics, notes = home / "Documents", home / "Projects", home / "Pictures", home / "Notes"
    write(docs / "Finance" / "Budget 2026.numbers", noise(412_000, 1))
    write(docs / "Finance" / "Tax return 2025.pdf", noise(1_840_000, 2))
    write(docs / "Finance" / "Bond statement August.pdf", noise(640_000, 3))
    for i, name in enumerate(["To the body corporate", "Insurance claim", "Reference for Sam"]):
        write(docs / "Letters" / f"{name}.pages", noise(90_000 + i * 20_000, 10 + i))
    write(docs / "Wedding speech.pages", noise(2_330_000, 4))
    write(proj / "keepr" / "design-notes.md", "# Keepr design notes\n\nThe strata show how much changed each day.\nRestore starts from a moment, not a file.\n")
    write(proj / "keepr" / "icon.png", noise(640_000, 5))
    for i in range(12):
        write(proj / "two-edged-sword" / "src" / f"screen{i}.tsx", f"export const screen{i} = () => null;\n" * 40)
    write(proj / "two-edged-sword" / "README.md", "# Two-edged Sword\n")
    write(proj / "two-edged-sword" / "node_modules" / "left-pad" / "index.js", "module.exports = 1;")
    for i in range(24):
        write(pics / "2026" / f"IMG_{4100 + i}.jpg", noise(300_000 + i * 5_000, 100 + i))
    write(notes / "Journal.md", "Notes\n")
    nas = home / "Backups" / "Keepr"
    nas.mkdir(parents=True)
    ex = ["node_modules/", "target/", ".DS_Store", "*.tmp", "~/Library/Caches", ".Trash/"]

    def plan(pid, name, sources, dest, every, at="02:00"):
        return {
            "id": pid, "name": name, "enabled": True, "sources": [{"kind": "folder", "path": str(s)} for s in sources],
            "destination": dest, "folder": f"{name.replace(' & ', '-').replace(' ', '-')} {pid[:6]}",
            "schedule": {"every": every, "at": at, "weekday": 0},
            "retention": {"allHours": 24, "dailyDays": 30, "weeklyWeeks": 52, "monthlyMonths": 0, "keepDeletedDays": 90},
            "excludes": ex, "gitignore": True, "skipCloudOnly": True, "skipMarked": True, "maxFileSize": 0, "encrypted": False,
            "fullEvery": "weekly", "checkEvery": "weekly",
            "conditions": {"catchUp": True, "onBattery": True, "minBattery": 20, "noHotspot": True, "limitMbps": 0},
        }

    config = {
        "destinations": [
            {"id": "nas000000001", "name": "Backups", "place": {"kind": "folder", "path": str(nas)}, "disconnectAfter": True},
            {"id": "ssd000000001", "name": "Archive SSD", "place": {"kind": "folder", "path": "/Volumes/Keepr Demo Archive SSD/Keepr"}, "disconnectAfter": True},
        ],
        "plans": [
            plan("docs00000001", "Documents & Projects", [docs, proj], "nas000000001", "hourly"),
            plan("pics00000001", "Photos", [pics], "nas000000001", "daily"),
            plan("note00000001", "Notes vault", [notes], "ssd000000001", "minutes15"),
        ],
        "settings": {"notifyFailures": False, "notifySuccess": False, "staleDays": 3},
    }
    write(DEMO / "data" / "config.json", json.dumps(config, indent=2))

    def back_up(*ids):
        r = subprocess.run([str(BIN), "--back-up", *ids], env=env(), capture_output=True, text=True)
        print("   ", r.stdout.strip().replace("\n", "\n    "))

    print("demo backups:")
    back_up("docs00000001", "pics00000001", "note00000001")
    edits = [
        lambda: write(docs / "Finance" / "Budget 2026.numbers", noise(413_500, 6)),
        lambda: write(proj / "keepr" / "design-notes.md", (proj / "keepr" / "design-notes.md").read_text() + "Versions go down the side.\n"),
        lambda: (docs / "Finance" / "Bond statement August.pdf").unlink(),
        lambda: write(docs / "Finance" / "Budget 2026.numbers", noise(414_200, 7)),
    ]
    for e in edits:
        time.sleep(1.1)
        e()
        back_up("docs00000001")
    time.sleep(1.1)
    write(pics / "2026" / "IMG_4200.jpg", noise(410_000, 300))
    back_up("pics00000001")


def capture(scene, theme, data):
    """Launches the app on the scene, unseen, and returns its webview's snapshot as an sRGB image, or None."""
    sc = {k: v for k, v in scene.items() if k not in ("name", "crop", "width")}
    sc["theme"] = theme
    with tempfile.TemporaryDirectory() as tmp:
        shot = pathlib.Path(tmp) / "shot.tiff"
        app = subprocess.Popen(
            [str(BIN)],
            cwd=ROOT / "src-tauri",
            env={
                **env(data),
                "KEEPR_SCENE": json.dumps(sc).replace("{home}", str(DEMO / "Home")),
                "KEEPR_SNAPSHOT": str(shot),
                "KEEPR_SNAPSHOT_AFTER": str(SETTLE),
            },
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            # The app writes the file whole (atomically) once WebKit has drawn it.
            end = time.time() + SETTLE + 30
            while not shot.exists() and app.poll() is None and time.time() < end:
                time.sleep(0.2)
            if not shot.exists():
                return None
            im = Image.open(shot)
            im.load()
            return to_srgb(im.convert("RGBA") if im.mode not in ("RGB", "RGBA") else im)
        finally:
            app.send_signal(signal.SIGKILL)
            app.wait()


def chrome(im, theme):
    """Draws back what a webview snapshot leaves out: the window's (unfocused) buttons and its
    rounded corners, as a capture of the window showed them."""
    k = 4  # drawn large and scaled down, for smooth edges
    w, h = im.size
    over = Image.new("RGBA", (w * k, h * k))
    d = ImageDraw.Draw(over)
    fill, rim = ((230, 231, 233), (220, 221, 223)) if theme == "light" else ((126, 127, 129), (140, 141, 143))
    for cx in (23.5, 46, 68.5):
        cy, r = 19, 7
        d.ellipse([(cx - r) * k, (cy - r) * k, (cx + r) * k, (cy + r) * k], fill=fill + (255,), outline=rim + (255,), width=k)
    im = Image.alpha_composite(im.convert("RGBA"), over.resize((w, h), Image.LANCZOS))
    mask = Image.new("L", (w * k, h * k))
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, w * k - 1, h * k - 1], radius=16 * k, fill=255)
    im.putalpha(mask.resize((w, h), Image.LANCZOS))
    return im


def blank(im):
    # An undrawn webview is one flat colour; the splash, the sparsest scene, is well above this.
    return ImageStat.Stat(im.convert("L")).stddev[0] < 1


def shoot(scene, theme, data):
    # A shot that came out blank (or never came) is taken again, from a fresh launch.
    for _ in range(3):
        im = capture(scene, theme, data)
        if im is not None and not blank(im):
            break
    else:
        print(f"  {scene['name']} {theme}: " + ("no window" if im is None else "blank"))
        return False
    # The menu-bar window keeps its natural 2x size; the main window is scaled to WIDTH.
    if not scene.get("tray"):
        im = im.resize((WIDTH, round(im.height * WIDTH / im.width)), Image.LANCZOS)
    if not scene.get("tray"):
        im = chrome(im, theme)
    if "crop" in scene:
        x, y, w, h = scene["crop"]
        im = im.crop((x, y, x + w, y + h))
    out = OUT / f"{scene['name']}-{theme}.png"
    im.save(out, optimize=True)
    print(f"  {out.relative_to(ROOT)}  {im.width}x{im.height}")
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*")
    ap.add_argument("--theme", choices=["light", "dark"], action="append")
    ap.add_argument("--fresh", action="store_true", help="remake the demo data")
    ap.add_argument("-j", type=int, default=4, metavar="N", help="apps to run side by side (default 4)")
    ap.add_argument("--release", action="store_true", help="run the built app (make app) instead of the dev build")
    a = ap.parse_args()
    global BIN
    if a.release:
        if not RELEASE_BIN.exists():
            sys.exit("no built app: run make app first")
        BIN = RELEASE_BIN
    scenes = json.loads((HERE / "scenes.json").read_text())
    if a.names:
        scenes = [s for s in scenes if s["name"] in a.names]
    themes = a.theme or ["light", "dark"]
    OUT.mkdir(parents=True, exist_ok=True)
    server = None if a.release else dev_server()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            if not a.release:
                build()
            if a.fresh or not (DEMO / "data" / "state.json").exists():
                make_demo()
            # Each app running at once gets its own copy of the app data.
            jobs = [(s, t) for s in scenes for t in themes]
            n = max(1, min(a.j, len(jobs)))
            free = queue.Queue()
            for i in range(n):
                data = pathlib.Path(tmp) / f"data-{i}"
                shutil.copytree(DEMO / "data", data)
                free.put(data)

            def one(job):
                data = free.get()
                try:
                    return shoot(*job, data)
                finally:
                    free.put(data)

            with concurrent.futures.ThreadPoolExecutor(n) as pool:
                ok = all(list(pool.map(one, jobs)))
    finally:
        if server:
            server.terminate()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
