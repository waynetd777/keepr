#!/usr/bin/env python3
"""Draw Keepr's artwork: design/icon.png (the app icon source) and the menu-bar templates in
src-tauri/icons/ (tray@2x.png, tray-busy@2x.png, tray-alert@2x.png).

The mark is three stacked lines, the last one short: layers of kept versions. The app icon puts
them in white on a green rounded tile; the menu-bar images are TEMPLATES (black plus alpha),
which macOS tints for light and dark menu bars. Drawn with PIL, supersampled for smooth edges.

    python3 tools/make_icons.py            # then: npx tauri icon design/icon.png
    make icons                             # does both
"""

import pathlib

from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
SS = 4
GREEN_TOP = (26, 138, 104)
GREEN_BOTTOM = (13, 98, 74)


def app_icon():
    S = 1024 * SS
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # Apple's icon grid: an 824px tile centred in 1024, corner radius about 185.
    m = 100 * SS
    tile = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    grad = Image.new("RGBA", (1, S))
    for y in range(S):
        t = y / S
        grad.putpixel((0, y), tuple(int(a + (b - a) * t) for a, b in zip(GREEN_TOP, GREEN_BOTTOM)) + (255,))
    grad = grad.resize((S, S))
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle((m, m, S - m, S - m), radius=185 * SS, fill=255)
    tile.paste(grad, (0, 0), mask)
    img = Image.alpha_composite(img, tile)
    d = ImageDraw.Draw(img)
    w = 70 * SS
    x0, x1, xs = 300 * SS, 724 * SS, 560 * SS
    for y, right in ((380, x1), (512, x1), (644, xs)):
        y *= SS
        d.rounded_rectangle((x0, y - w // 2, right, y + w // 2), radius=w // 2, fill=(255, 255, 255, 255))
    out = ROOT / "design" / "icon.png"
    out.parent.mkdir(exist_ok=True)
    img.resize((1024, 1024), Image.LANCZOS).save(out)


def tray(name, extra=None):
    # 22pt menu-bar image at @2x: 44px square.
    S = 44 * SS
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    w = 4 * SS
    x0, x1, xs = 9 * SS, 35 * SS, 25 * SS
    for y, right in ((14, x1), (22, x1), (30, xs)):
        y *= SS
        d.rounded_rectangle((x0, y - w // 2, right, y + w // 2), radius=w // 2, fill=(0, 0, 0, 255))
    if extra == "busy":
        # A small filled dot at the end of the short line: something is running.
        cx, cy, r = 33 * SS, 30 * SS, 4 * SS
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(0, 0, 0, 255))
    elif extra == "alert":
        # An exclamation mark in place of the short line's end.
        d.rounded_rectangle((31 * SS, 25 * SS, 35 * SS, 31 * SS), radius=2 * SS, fill=(0, 0, 0, 255))
        d.ellipse((31 * SS, 33 * SS, 35 * SS, 37 * SS), fill=(0, 0, 0, 255))
    img.resize((44, 44), Image.LANCZOS).save(ROOT / "src-tauri" / "icons" / name)


if __name__ == "__main__":
    (ROOT / "src-tauri" / "icons").mkdir(parents=True, exist_ok=True)
    app_icon()
    tray("tray@2x.png")
    tray("tray-busy@2x.png", "busy")
    tray("tray-alert@2x.png", "alert")
    print("drew design/icon.png and src-tauri/icons/tray*.png")
