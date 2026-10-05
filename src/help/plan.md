---
title: A backup plan
kind: screen
screens: [plans]
order: 3
summary: What a plan backs up and leaves out, where and when, the versions it keeps, and encryption.
---
A plan is a set of folders backed up to one destination, on a schedule, with rules for what to leave out and how long to keep versions. A new plan starts hourly, encrypted, keeping every backup for a day, then one a day for 30 days, one a week for 12 months and one a month forever. Change what you like, then click **Create and back up**. After that, changes are saved as you make them.

## What to keep
**Add a source** adds what the plan backs up:

- **Folder or drive…**: any folders on this Mac or a connected drive. Pick several at once.
- **SMB share on the network…**: a folder on a NAS or another computer.
- **In iCloud Drive…**, **In Google Drive…**, **In OneDrive…** and so on: a folder in a cloud service synced to this Mac.
- **S3 bucket…**: a bucket at Amazon S3, Backblaze B2, Cloudflare R2 or another S3 service. **Check** tells you how much is in it.

Keepr only reads your sources; it never changes or moves them. Click a source's name to change it. Removing a source that has been backed up asks whether to **Keep its versions** or **Delete its backed-up data…**.

## What to leave out
Rules leave files and folders out. Type one in **+ Rule** and press Return; click × on a rule to remove it.

- `name` or `*.iso` leaves out any file or folder with that name, or matching the pattern.
- `build/` leaves out any folder with that name, and everything in it.
- `~/Movies` or `/Volumes/Scratch` leaves out that folder and everything in it.

The switches below also skip what a project's `.gitignore` leaves out, files only in the cloud (they aren't downloaded), what apps mark as not needing a backup (as Time Machine does), and files larger than a size you choose.

## Versions to keep
Every backup is a snapshot of all the plan's files. Older snapshots are thinned out: **Every backup** for a while, then **One a day**, **One a week** and **One a month**, each for as long as you choose. The newest snapshot is always kept.

**Keep deleted files for at least 90 days** keeps the last copy of anything deleted in the last 90 days, whatever the rules say. Thinning out happens in a tidy-up after a backup, at most once a day.

## Where
**Where** is the destination the plan backs up to; the backup is a folder in it named after the plan. Once a plan has backed up, its destination is fixed: to keep a backup somewhere else as well, make another plan. **Add another destination** adds one without leaving the plan. See [Destinations](help:destinations).

## When
**Every 15 min**, **Hourly**, **Daily** or **Weekly** at a time you choose, or **Only when I ask**. Then:

- **Catch up after sleep or when the destination comes back** runs a missed backup as soon as it can.
- **Back up while on battery**, and **Wait when battery is below 20%**.
- **Not on a personal hotspot** waits while the Mac is on an expensive connection.
- **Limit speed to** a number of MB/s, for writing to the destination.

These hold back scheduled backups only: **Back up now** runs straight away.

## Full backups and checks
The first backup copies everything; after that each one looks only at what changed, and every snapshot still restores as a complete copy.

- **Re-read every file, not just changed ones**: weekly, monthly or never. It still stores only what's new.
- **Check stored data can be read back**: a sample each week, all of it each month, or never. A check that finds a problem always tells you.

The ⋯ menu runs either now.

## Before and after each backup
**Before each backup** runs a command first, such as one that exports a database into a folder the plan backs up; **If it fails**, Keepr backs up anyway with a warning, or doesn't back up. **After each backup** runs one when the backup ends, however it went, and tells it how it went in `KEEPR_` environment variables such as `KEEPR_RESULT`. Both run in your login shell, and their output goes into the backup's log in [Activity](app:activity).

## Encryption
An encrypted plan's files can only be read with its password or its recovery key. Encryption is chosen when you make a plan and can't be changed afterwards. Keepr keeps the password in your Keychain.

The first backup makes a **recovery key**, which works in place of the password. Keepr keeps asking you to save it (**Save recovery key…**) until you click **I've saved it**. Keep it somewhere safe away from the Mac, such as a password manager.

> Without the password or the recovery key, nobody can restore these files, including you.

**Change password…** needs the current password or the recovery key, and rewrites nothing in the backup.

## Turn a plan off, or delete it
**Plan on** turns a plan off: it won't back up until you turn it on again, and its backups stay. In the ⋯ menu, **Delete plan…** deletes the plan and its password and recovery key; the backup itself stays at the destination. **Start a new backup…** is only for when a plan's backup has gone for good, a drive that died, say.
