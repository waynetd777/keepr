---
title: A backup plan
kind: screen
screens: [plans]
order: 3
summary: What a plan backs up and leaves out, where and when, the versions it keeps, and encryption.
---
A plan is a set of folders backed up to one destination, on a schedule, with rules for what to leave out and how long to keep versions. A new plan starts hourly, encrypted, keeping every backup for a day, then one a day for 30 days, one a week for 12 months and one a month forever. Change what you like, then click **Create and back up**. Until then the new plan is kept if you leave its page: Backup Plans says so, and **Continue new plan** in the toolbar (or ⌘N) goes back to it. It's kept only while Keepr is open, so quitting Keepr discards it; **Discard** throws it away sooner. After that, changes are saved as you make them.

## What to keep
**Add a source** adds what the plan backs up:

- **Folder or drive…**: any folders on this computer or a connected drive. Pick several at once.
- **SMB share on the network…**: a folder on a NAS or another computer.
- **In iCloud Drive…**, **In Google Drive…**, **In OneDrive…** and so on: a folder in a cloud service synced to this computer.
- **S3 bucket…**: a bucket at Amazon S3, Backblaze B2, Cloudflare R2 or another S3 service. **Check** tells you how much is in it.

Keepr only reads your sources; it never changes or moves them. Click a source's name to change it. Removing a source that has been backed up asks whether to **Keep its versions** or **Delete its backed-up data…**. A plan backs up at least one source: to stop backing up its last, delete the plan.

## What to leave out
Rules leave files and folders out. Type one in **+ Rule** and press Return; click × on a rule to remove it.

- `name` or `*.iso` leaves out any file or folder with that name, or matching the pattern.
- `build/` leaves out any folder with that name, and everything in it.
- <!-- mac -->`~/Movies` or `/Volumes/Scratch`<!-- /mac --><!-- github --> on a Mac, or on Windows <!-- /github --><!-- windows -->`~/Videos` or `D:/Scratch`<!-- /windows --> leaves out that folder and everything in it.<!-- windows --> Write a folder's path with `/`, from `~` (your user folder) or from a drive (`D:/`); `/` alone is the system drive. On Windows rules match in any case.<!-- /windows -->

The switches below also skip what a project's `.gitignore` leaves out, files only in the cloud (they aren't downloaded), <!-- mac -->what apps mark as not needing a backup (as Time Machine does), <!-- /mac -->and files larger than a size you choose.<!-- windows --> On Windows a file only in the cloud is one OneDrive, iCloud or another service shows with its cloud mark in File Explorer.<!-- /windows -->

A new plan starts with rules for what never needs a backup: <!-- mac -->`node_modules/`, `target/`, `.DS_Store`, `*.tmp`, `~/Library/Caches` and the folders macOS keeps on every drive (`.Trashes/`, `.Spotlight-V100/` and the like)<!-- /mac --><!-- github --> on a Mac; on Windows <!-- /github --><!-- windows -->`node_modules/`, `target/`, `*.tmp`, `Thumbs.db`, `~/AppData/Local/Temp`, `~/AppData/Local/Microsoft/Windows/INetCache`, `$RECYCLE.BIN/` and `System Volume Information/`<!-- /windows -->.

**Leave this plan out of Ask** keeps the plan's folders and runs out of [Ask](help:ask): it sees only the plan's name, its Ask buttons are greyed out, and Ask can't suggest changes to it.

## Versions to keep
Every backup is a snapshot of all the plan's files. Older snapshots are thinned out: **Every backup** for a while, then **One a day**, **One a week** and **One a month**, each for as long as you choose, or not kept. The sentence under them says what they keep, counting back from now. The newest snapshot is always kept.

**Keep deleted files for at least 90 days** keeps the last copy of anything deleted in the last 90 days, whatever the rules say. Thinning out happens in a tidy-up after a backup, at most once a day.

## Where
**Where** is the destination the plan backs up to; the backup is a folder in it named after the plan. Once a plan has backed up, its destination is fixed: to keep a backup somewhere else as well, make another plan. **Add another destination** adds one without leaving the plan, and picks it if the plan hasn't backed up yet. See [Destinations](help:destinations).

## When
**Every 15 min**, **Hourly**, **Daily** or **Weekly** at a time you choose, or **Only when I ask**. Then:

- **Catch up after sleep or when the destination comes back** runs a missed backup as soon as it can.
- **Back up while on battery**, and **Wait when battery is below 20%**.
- **Not on a personal hotspot** waits while the <!-- mac -->Mac is on an expensive connection<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->PC is on a metered connection: a phone's hotspot, or one set as metered in Windows' Settings<!-- /windows --><!-- github --> (Windows)<!-- /github -->.
- **Limit speed to** a number of MB/s, for writing to the destination.

These hold back scheduled backups only: **Back up now** runs straight away.

## Full backups and checks
The first backup copies everything; after that each one looks only at what changed, and every snapshot still restores as a complete copy.

- **Re-read every file, not just changed ones**: weekly, monthly or never. It still stores only what's new.
- **Check stored data can be read back**: a sample each week, all of it each month, or never. A check that finds a problem always tells you.

The ⋯ menu runs either now.

## Before and after each backup
**Before each backup** runs a command first, such as one that exports a database into a folder the plan backs up; **If it fails**, Keepr backs up anyway with a warning, or doesn't back up. **After each backup** runs one when the backup ends, however it went, and tells it how it went in `KEEPR_` environment variables such as `KEEPR_RESULT`. Both run in your <!-- mac -->login shell (zsh)<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->PowerShell, which reads them as `$env:KEEPR_RESULT`<!-- /windows --><!-- github --> (Windows)<!-- /github -->, and their output goes into the backup's log in [Activity](app:activity). While one runs, Activity and the <!-- mac -->menu bar<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->notification-area panel<!-- /windows --> say so, since nothing is read until it ends.

## Encryption
An encrypted plan's files can only be read with its password or its recovery key. Encryption is chosen when you make a plan and can't be changed afterwards. Keepr keeps the password in your <!-- mac -->Keychain<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->Windows' Credential Manager<!-- /windows --><!-- github --> (Windows)<!-- /github -->.

The first backup makes a **recovery key**, which works in place of the password. Keepr keeps asking you to save it (**Save recovery key…**) until you click **I've saved it**. Keep it somewhere safe away from this computer, such as a password manager.

> Without the password or the recovery key, nobody can restore these files, including you.

**Change password…** needs the current password or the recovery key, and rewrites nothing in the backup.

## Turn a plan off, or delete it
**Plan on** turns a plan off: it won't back up until you turn it on again, and its backups stay. In the ⋯ menu, **Delete plan…** deletes the plan and its password and recovery key, and asks whether to **Keep Backup** at the destination or delete it too (**Delete Plan and Backup…**, which frees its space, can't be undone, and shows in Activity as it goes). If no other plan uses the destination, it can remove that too. **Start a new backup…** is only for when a plan's backup has gone for good, a drive that died, say.
