#!/usr/bin/env python3
"""Screenshots of Keepr's screens, from demo data: design/screens/<scene>-<theme>.png.

Makes a demo in .demo/ (gitignored): sample folders, a "NAS" folder as the destination, and a
drive that isn't connected. It backs the folders up a few times with `Keepr --back-up`, editing
files between runs, so there are snapshots and versions to show. Then it launches the dev build
once per scene and theme with the scene in KEEPR_SCENE (the app saves and runs nothing), and
captures the window. Nothing outside .demo/ is read or written, and no snapshots of the Mac are
taken (KEEPR_NO_STILL).

    python3 tools/screenshots.py                 # every scene, light and dark
    python3 tools/screenshots.py restore         # one scene
    python3 tools/screenshots.py --theme dark    # one theme
    python3 tools/screenshots.py --fresh         # remake the demo first

Needs the Vite dev server (started here if it isn't running), Screen Recording permission for the
terminal, Pillow and swiftc.
"""

import argparse
import io
import json
import os
import pathlib
import random
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.request

from PIL import Image, ImageCms

ROOT = pathlib.Path(__file__).resolve().parent.parent
HERE = ROOT / "tools" / "screenshots"
DEMO = ROOT / ".demo"
OUT = ROOT / "design" / "screens"
BIN = ROOT / "src-tauri" / "target" / "debug" / "Keepr"
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


def build(tmp):
    subprocess.run(["cargo", "build", "--no-default-features"], cwd=ROOT / "src-tauri", check=True)
    exe = pathlib.Path(tmp) / "window_id"
    subprocess.run(["swiftc", "-O", str(HERE / "window_id.swift"), "-o", str(exe)], check=True)
    return exe


def env():
    return {**os.environ, "KEEPR_DATA": str(DEMO / "data"), "KEEPR_NO_STILL": "1"}


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
    nas = DEMO / "NAS" / "Backups" / "Keepr"
    nas.mkdir(parents=True)
    ex = ["node_modules/", "target/", ".DS_Store", "*.tmp", "~/Library/Caches", ".Trash/"]

    def plan(pid, name, sources, dest, every, at="02:00"):
        return {
            "id": pid, "name": name, "enabled": True, "sources": [{"kind": "folder", "path": str(s)} for s in sources],
            "destination": dest, "folder": f"{name.replace(' & ', '-').replace(' ', '-')} {pid[:6]}",
            "schedule": {"every": every, "at": at, "weekday": 0},
            "retention": {"allHours": 24, "dailyDays": 30, "weeklyWeeks": 52, "monthlyMonths": 0, "keepDeletedDays": 90},
            "excludes": ex, "gitignore": True, "skipCloudOnly": True, "maxFileSize": 0, "encrypted": False,
            "fullEvery": "weekly", "checkEvery": "weekly",
            "conditions": {"catchUp": True, "minBattery": 20, "noHotspot": True, "limitMbps": 0},
        }

    config = {
        "destinations": [
            {"id": "nas000000001", "name": "keep-nas", "place": {"kind": "folder", "path": str(nas)}, "disconnectAfter": True},
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


def window_of(window_id, pid, timeout=30, any_layer=False):
    end = time.time() + timeout
    while time.time() < end:
        out = subprocess.run([str(window_id), str(pid)] + (["any"] if any_layer else []), capture_output=True, text=True).stdout.strip()
        if out:
            return out
        time.sleep(0.3)
    return None


def shoot(scene, theme, window_id):
    sc = {k: v for k, v in scene.items() if k not in ("name", "crop")}
    sc["theme"] = theme
    app = subprocess.Popen([str(BIN)], cwd=ROOT / "src-tauri", env={**env(), "KEEPR_SCENE": json.dumps(sc)}, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        floating = bool(scene.get("tray"))
        win = window_of(window_id, app.pid, any_layer=floating)
        if not win:
            print(f"  {scene['name']} {theme}: no window")
            return False
        time.sleep(SETTLE)
        win = window_of(window_id, app.pid, timeout=5, any_layer=floating) or win
        with tempfile.NamedTemporaryFile(suffix=".png") as raw:
            subprocess.run(["screencapture", "-x", "-o", f"-l{win}", raw.name], check=True)
            im = Image.open(raw.name)
            im.load()
            im = to_srgb(im)
        if not floating:
            im = im.resize((WIDTH, round(im.height * WIDTH / im.width)), Image.LANCZOS)
        if "crop" in scene:
            x, y, w, h = scene["crop"]
            im = im.crop((x, y, x + w, y + h))
        out = OUT / f"{scene['name']}-{theme}.png"
        im.save(out, optimize=True)
        print(f"  {out.relative_to(ROOT)}  {im.width}x{im.height}")
        return True
    finally:
        app.send_signal(signal.SIGKILL)
        app.wait()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="*")
    ap.add_argument("--theme", choices=["light", "dark"], action="append")
    ap.add_argument("--fresh", action="store_true", help="remake the demo data")
    a = ap.parse_args()
    scenes = json.loads((HERE / "scenes.json").read_text())
    if a.names:
        scenes = [s for s in scenes if s["name"] in a.names]
    themes = a.theme or ["light", "dark"]
    OUT.mkdir(parents=True, exist_ok=True)
    server = dev_server()
    try:
        with tempfile.TemporaryDirectory() as tmp:
            window_id = build(tmp)
            if a.fresh or not (DEMO / "data" / "state.json").exists():
                make_demo()
            ok = all([shoot(s, t, window_id) for s in scenes for t in themes])
    finally:
        if server:
            server.terminate()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
