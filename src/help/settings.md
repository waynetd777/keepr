---
title: Settings
kind: screen
screens: [settings]
order: 8
summary: Open at Login, Getting started, notifications, Ask, the website, appearance and updates.
---
Settings (`⌘,`) holds what applies to Keepr as a whole; what applies to one plan is in the plan.

## Open at Login
Starts Keepr when you <!-- mac -->log in, in the menu bar with no window. It's the same setting as System Settings › General › Login Items, and as the switch in the menu bar panel.<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->sign in, in the notification area with no window. It's the same as the switch in the notification-area panel, and shows in Windows' Settings › Apps › Startup. If it's switched off there, turning it on in Keepr opens that page, where only you can switch it back on.<!-- /windows -->

<!-- windows -->
## Still copies and the change journal
Installs **Keepr Helper**, a small Windows service that does for Keepr what only an administrator may: a still copy of the drive for each backup (a shadow copy), so files that change during it are copied as they were at one moment, and the drive's change journal, so a backup looks only in folders that changed. Windows asks for an administrator once, when you turn it on. It works for folders on one of the PC's own NTFS drives; it answers only your account (and administrators), and Keepr still reads every file as you. Turning it off removes it; so does uninstalling Keepr. If it's from an older Keepr, **Update** installs this one's.
<!-- /windows -->

## Getting started
**Offer Getting started when Keepr opens** shows a welcome when the window opens, with **Open Getting Started** for the first guide. **Don't ask me again** in the welcome turns this off too. The guides are always in the help: press `?` and click **Getting started**.

## Notifications
- **Tell me when a backup fails** (on unless you turn it off): a backup, check or restore that didn't finish.
- **Tell me when a backup finishes** (off unless you turn it on).
- **Warn when a plan hasn't backed up for** 1 to 14 days: once a day until it backs up. Plans that back up only when you ask aren't warned about.

A check that finds a problem with the stored data always tells you.

## Ask
**Ask** turns on [Ask](help:ask): an AI tool already signed in on this computer answers questions about your backups, with Keepr's figures and folder names, never your files. Off, nothing is sent, and the Ask buttons are greyed out.

## Website
**Website** opens Keepr's website, [keepr.davies.co.za](https://keepr.davies.co.za/), in your browser: what Keepr does and the latest version to download. <!-- mac -->It's also under Help › **Keepr Website**, and in **About Keepr** in the Keepr menu.<!-- /mac --><!-- windows -->Windows has no Keepr menu: **Check for Updates…** is beside it, and the foot of Settings is Keepr's About.<!-- /windows -->

## Appearance
**Match the <!-- mac -->Mac<!-- /mac --><!-- github --> (or PC)<!-- /github --><!-- windows -->PC<!-- /windows -->**, **Light** or **Dark**.

## Version and licence
The foot of Settings shows Keepr's version and build. Keepr is free software under the GNU General Public License, version 3 or later.

## Updates
Keepr checks for a new version once a day, and asks in its window before installing it, with what's new: **Install and Restart**, **Later** (asked again tomorrow) or **Skip This Version**. **Check for Updates…** <!-- mac -->in the Keepr menu<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->beside **Website** here, or in the notification-area panel,<!-- /windows --><!-- github --> (Windows)<!-- /github --> checks now.<!-- windows --> On Windows a new version comes some minutes after the Mac's; until then the check says there's no update for this system yet.<!-- /windows --> An update waits while a backup, restore or check is running, or while Ask is answering.
