#!/usr/bin/env python3
# Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
# See LICENSE for the full text.
# SPDX-License-Identifier: GPL-3.0-or-later

"""Builds the app's Apple Help Book from the user guides in docs/, so there is one copy of the text.

Writes src-tauri/gen/help/Keepr.help: the pages as HTML (with pandoc), the screenshots they
show, and the search indexes hiutil makes, which the Help menu's search field and Help Viewer use.
The bundle goes into the app's Contents/Resources (bundle.macOS.files in tauri.conf.json) and the
app's Info.plist names it (CFBundleHelpBookFolder, CFBundleHelpBookName).
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from html import escape
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"
BOOK = ROOT / "src-tauri/gen/help/Keepr.help"
TITLE = "Keepr Help"
BOOK_ID = "com.wayned.keepr.help"
# The user guides, features.md first as the book's front page. development.md is for contributors.
PAGES = ["features", "plans", "restore", "destinations", "day-to-day"]

# The app's look (src/styles.css): its neutral Mac palette and green accent, the system font throughout
# (SF Pro Display semibold for headings), cards and tables like the app's. Keep the colours in step with styles.css's :root.
CSS = """
:root {
  color-scheme: light dark;
  --bg: #ffffff; --panel: #ffffff; --panel2: #f5f5f7; --border: #e5e5ea; --text: #1d1d1f; --muted: #6e6e73; --accent: #0b5c45; --ring: #0f7a5c;
  --display: -apple-system, BlinkMacSystemFont, "SF Pro Display", sans-serif; --ui: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Helvetica Neue", sans-serif; --mono: ui-monospace, "SF Mono", Menlo, monospace;
}
@media (prefers-color-scheme: dark) {
  :root { --bg: #1e1e20; --panel: #252527; --panel2: #1b1b1d; --border: #323235; --text: #f2f2f4; --muted: #8e8e93; --accent: #6fd9b1; --ring: #3cc593; }
}
* { box-sizing: border-box; }
body { font: 14px/1.55 var(--ui); color: var(--text); background: var(--bg); -webkit-font-smoothing: antialiased; margin: 0; padding: 20px 28px 56px; }
h1, h2, h3 { font-family: var(--display); font-weight: 650; letter-spacing: -0.02em; }
h1 { font-size: 28px; line-height: 1.1; margin: 6px 0 14px; }
h2 { font-size: 20px; line-height: 1.2; margin: 28px 0 8px; }
h3 { font-size: 16px; line-height: 1.3; margin: 22px 0 6px; }
p, ul, ol { margin: 0 0 10px; } li { margin: 3px 0; } ul, ol { padding-left: 22px; }
a { color: var(--accent); text-decoration: none; } a:hover { text-decoration: underline; }
strong { font-weight: 600; }
.crumbs { font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); margin: 0 0 2px; }
.crumbs a { color: var(--muted); } .crumbs a:hover { color: var(--accent); text-decoration: none; }
img { max-width: 100%; height: auto; display: block; border: 1px solid var(--border); border-radius: 10px; margin: 12px 0 16px; }
table { border-collapse: separate; border-spacing: 0; width: 100%; background: var(--panel); border: 1px solid var(--border); border-radius: 10px; overflow: hidden; margin: 10px 0 16px; }
th { font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); text-align: left; background: var(--panel2); padding: 8px 14px; border-bottom: 1px solid var(--border); }
td { padding: 8px 14px; border-bottom: 1px solid var(--border); vertical-align: top; } tr:last-child td { border-bottom: 0; }
code, kbd { font: 12px var(--mono); background: var(--panel2); border: 1px solid var(--border); padding: 0 4px; border-radius: 4px; }
pre { background: var(--panel2); border: 1px solid var(--border); padding: 10px 14px; border-radius: 10px; overflow-x: auto; } pre code { border: 0; padding: 0; background: none; }
blockquote { margin: 10px 0; padding: 8px 14px; background: var(--panel2); border: 1px solid var(--border); border-left: 3px solid var(--ring); border-radius: 8px; color: var(--muted); }
.topics { list-style: none; padding: 0; margin: 14px 0; display: flex; flex-direction: column; gap: 8px; }
.topics li { margin: 0; }
.topics a { display: block; padding: 12px 16px; background: var(--panel); border: 1px solid var(--border); border-radius: 10px; color: var(--text); }
.topics a:hover { border-color: var(--ring); text-decoration: none; }
.topics b { font-weight: 600; } .topics span { display: block; font-size: 12.5px; color: var(--muted); margin-top: 2px; }
"""


def slug(heading: str) -> str:
    """GitHub's anchor for a heading, which the docs' links use."""
    return re.sub(r"\s", "-", re.sub(r"[^\w\s-]", "", heading.strip().lower()))


def plain(md: str) -> str:
    """Markdown as plain text, for a search result's description."""
    t = re.sub(r"<[^>]+>", "", md)
    t = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", t)
    return re.sub(r"\s+", " ", re.sub(r"[*_`]", "", t)).strip()


def describe(md: str) -> str:
    """A topic's first sentence (or so), shown under it in Help search results."""
    for para in re.split(r"\n\s*\n", md):
        p = para.strip()
        if not p or p.startswith(("<sub>", "<a ", "<picture", "|", "#")) or re.fullmatch(r"(\[[^\]]+\]\(#[^)]+\)\s*·?\s*)+", p, re.S):
            continue
        t = plain(p.lstrip("-* "))
        m = re.match(r"(.{40,}?[.:;])\s", t + " ")
        t = re.sub(r"[:;]$", ".", m[1] if m else t)
        return t if len(t) <= 180 else t[:177].rsplit(" ", 1)[0] + "…"
    return ""


class Topic:
    def __init__(self, page: str, title: str, md: str, file: str):
        self.page, self.title, self.md, self.file = page, title, md, file


def split(page: str, md: str) -> tuple[Topic, list[Topic]]:
    """A guide as its chapter (the title and what comes before the first ## section) and one topic per section."""
    title = re.search(r"^# (.+)$", md, re.M)[1]
    parts = re.split(r"^## (.+)$", md, flags=re.M)
    intro = re.sub(r"^# .+$", "", parts[0], count=1, flags=re.M)
    intro = re.sub(r"^<sub>.*?</sub>\s*$", "", intro, flags=re.M)
    # The docs' own list of the sections: the chapter page lists its topics itself.
    intro = re.sub(r"^(\[[^\]]+\]\(#[^)]+\)\s*·?\s*)+$", "", intro, flags=re.M)
    chapter = Topic(page, title, intro, "index.html" if page == "features" else f"{page}.html")
    topics = [Topic(page, parts[i], parts[i + 1], f"{page}-{slug(parts[i])}.html") for i in range(1, len(parts), 2)]
    return chapter, topics


def to_html(md: str, where: dict[tuple[str, str], str], page: str) -> str:
    # The screenshots link to the docs' gallery page, which isn't in the book: keep just the picture.
    md = re.sub(r'<a href="images/index\.md[^"]*">(<picture>.*?</picture>|<img [^>]*>)</a>', r"\1", md, flags=re.S)
    # A link to a doc that isn't in the book (development.md, for contributors) would be dead text in Help.
    outside = re.findall(r"\[[^\]]+\]\((?!https?:|#|(?:" + "|".join(PAGES) + r")\.md)[^)]*\.md[^)]*\)", md)
    if outside:
        sys.exit(f"{page}.md links to docs that aren't in Help: {', '.join(outside)}")
    def link(m: re.Match) -> str:
        target, anchor = m[1] or page, (m[2] or "")[1:]
        return f"]({where.get((target, anchor)) or where.get((target, '')) or m[0][2:-1]})"
    md = re.sub(r"\]\((?:(" + "|".join(PAGES) + r")\.md)?(#[^)]*)?\)", lambda m: link(m) if m[1] or m[2] else m[0], md)
    return subprocess.run(["pandoc", "-f", "gfm", "-t", "html5"], input=md, capture_output=True, text=True, check=True).stdout


def write(path: Path, title: str, description: str, crumbs: str, body: str, anchor: str, front: bool = False) -> None:
    head = f'<meta name="AppleTitle" content="{TITLE}">\n<meta name="AppleIcon" content="../shared/icon.png">\n' if front else ""
    path.write_text(f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
{head}<meta name="description" content="{escape(description)}">
<title>{escape(title)}</title>
<link rel="stylesheet" href="../shared/help.css">
</head>
<body>
<a name="{anchor}"></a>
{crumbs}{body}</body>
</html>
""")


def main() -> None:
    if not shutil.which("pandoc"):
        sys.exit("helpbook: pandoc is needed to build the help (brew install pandoc)")
    version = json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"]
    # helpd caches a book as "identifier*CFBundleShortVersionString" and serves the cached pages until
    # that version changes (and a book whose version no longer matches its entry won't open at all),
    # so the book carries the app's version, which every release build bumps. CFBundleVersion is the
    # app's build number when the Makefile gives one (KEEPR_BUILD), else the time.
    build = os.environ.get("KEEPR_BUILD") or time.strftime("%Y%m%d.%H%M%S")
    if BOOK.exists():
        shutil.rmtree(BOOK)
    res = BOOK / "Contents/Resources"
    lproj = res / "en.lproj"
    (lproj / "images").mkdir(parents=True)
    (res / "shared").mkdir()
    (res / "shared/help.css").write_text(CSS.strip() + "\n")
    shutil.copy(ROOT / "src-tauri/icons/32x32.png", res / "shared/icon.png")

    chapters = [split(page, (DOCS / f"{page}.md").read_text()) for page in PAGES]
    # Where each doc and heading ends up: (page, heading anchor) -> file#anchor. A ### heading is in its ## topic's page.
    where: dict[tuple[str, str], str] = {}
    for chapter, topics in chapters:
        where[(chapter.page, "")] = chapter.file
        for t in topics:
            where[(t.page, slug(t.title))] = t.file
            for h in re.findall(r"^#{3,} (.+)$", t.md, re.M):
                where.setdefault((t.page, slug(h)), f"{t.file}#{slug(h)}")

    images: set[str] = set()
    home = '<a href="index.html">Help</a>'
    for chapter, topics in chapters:
        front = chapter.page == "features"
        body = to_html(f"# {TITLE if front else chapter.title}\n\n{chapter.md}", where, chapter.page)
        if topics:
            body += "<ul class=\"topics\">\n" + "".join(f'<li><a href="{t.file}"><b>{escape(t.title)}</b><span>{escape(describe(t.md))}</span></a></li>\n' for t in topics) + "</ul>\n"
        crumbs = "" if front else f'<p class="crumbs">{home} › {escape(chapter.title)}</p>\n'
        write(lproj / chapter.file, TITLE if front else chapter.title, describe(chapter.md) or chapter.title, crumbs, body, chapter.file[:-5], front)
        images.update(re.findall(r'(?:src|srcset)="images/([^"]+)"', body))
        for t in topics:
            tb = to_html(f"# {t.title}\n\n{t.md}", where, t.page)
            up = "" if front else f' › <a href="{chapter.file}">{escape(chapter.title)}</a>'
            write(lproj / t.file, t.title, describe(t.md) or t.title, f'<p class="crumbs">{home}{up} › {escape(t.title)}</p>\n', tb, t.file[:-5])
            images.update(re.findall(r'(?:src|srcset)="images/([^"]+)"', tb))
    for name in sorted(images):
        shutil.copy(DOCS / "images" / name, lproj / "images" / name)

    (BOOK / "Contents/Info.plist").write_text(f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key><string>en</string>
	<key>CFBundleIdentifier</key><string>{BOOK_ID}</string>
	<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
	<key>CFBundleName</key><string>Keepr</string>
	<key>CFBundlePackageType</key><string>BNDL</string>
	<key>CFBundleShortVersionString</key><string>{version}</string>
	<key>CFBundleSignature</key><string>hbwr</string>
	<key>CFBundleVersion</key><string>{build}</string>
	<key>HPDBookAccessPath</key><string>index.html</string>
	<key>HPDBookIconPath</key><string>shared/icon.png</string>
	<key>HPDBookIndexPath</key><string>search.helpindex</string>
	<key>HPDBookCSIndexPath</key><string>search.cshelpindex</string>
	<key>HPDBookTitle</key><string>{TITLE}</string>
	<key>HPDBookType</key><string>3</string>
</dict>
</plist>
""")
    # The search indexes: Core Spotlight's for current macOS, the older LSM one as well.
    subprocess.run(["hiutil", "-I", "corespotlight", "-Caf", str(lproj / "search.cshelpindex"), str(lproj)], check=True, capture_output=True)
    subprocess.run(["hiutil", "-I", "lsm", "-Caf", str(lproj / "search.helpindex"), str(lproj)], check=True, capture_output=True)
    print(f"helpbook: {len(list(lproj.glob('*.html')))} pages, {len(images)} images -> {BOOK.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
