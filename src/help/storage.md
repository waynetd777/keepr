---
title: How backups work
kind: screen
screens: []
order: 10
summary: What Keepr does in the background, and how a backup is stored at its destination.
---
Keepr backs up on its own, whether or not its window is open.

## Still copies of the startup disk
<!-- mac -->For folders on the Mac's own disk, Keepr takes a still copy of the disk (an APFS snapshot, as Time Machine does) and backs up from that, so files that change during the backup are copied as they were at one moment. The copy is deleted afterwards. Drives, shares, cloud folders and buckets are read as they are.<!-- /mac --><!-- github --> On Windows: <!-- /github --><!-- windows -->On Windows, a still copy (a shadow copy, Volume Shadow Copy) needs an administrator, so Keepr leaves it to **Keepr Helper**, a small Windows service you can install from [Settings](app:settings) (**Still copies and the change journal**; Windows asks for an administrator once). With it, for folders on one of the PC's own NTFS drives, each backup reads a still copy of the drive, deleted afterwards. Without it, files are read as they are, and a file that changes while it's read is backed up again next time.<!-- /windows -->

## Only what changed
<!-- mac -->For folders on the Mac's own disk, Keepr asks macOS which folders have changed since the last backup, and only looks in those. A full re-read looks at everything.<!-- /mac --><!-- github --> On Windows: <!-- /github --><!-- windows -->On Windows, each backup looks at every file and compares its size and times with the last snapshot, and reads only those that changed (Windows' own record of changes needs an administrator). Looking takes longer for a folder of many files than on a Mac; a full re-read reads everything.<!-- /windows -->

## Stored once, however many snapshots
Each plan's backup is a folder at its destination. Files are split into pieces by their content, and each piece is stored once, however many files and snapshots have it, so a file that moves or is copied costs nothing more. Pieces are compressed, unless they already are.

## Encrypted with a key of its own
An encrypted plan's pieces and snapshots are encrypted under a random key, which is itself locked with your password and, separately, with the recovery key. That's why changing the password rewrites nothing else.

## Never a broken snapshot
A snapshot is written last, after everything it needs, so a backup that's stopped or cut off never leaves a broken snapshot.

## Tidying up
After a backup, at most once a day, Keepr thins out old snapshots by the plan's Versions to keep, and frees the space nothing uses any more.
<!-- windows -->

## On Windows
Keepr works the same on a PC, with what Windows has in place of the Mac's:

- **Paths** are shown as Windows writes them (`C:\Users\alex\Documents`). In a plan's rules write them with `/`, from `~` or a drive: `~/AppData/Local/Temp`, `D:/Scratch`.
- **Passwords** are in Windows' Credential Manager (Control Panel › Credential Manager › Windows Credentials), one for each, named Keepr/…. Windows never asks before Keepr reads its own.
- **Settings and history** are in `%APPDATA%\Keepr`. Uninstalling with Delete the application data ticked removes them; the backups themselves stay at their destinations.
- **Still copies and the change journal** need **Keepr Helper**, a small Windows service, installed from [Settings](app:settings) with an administrator's go-ahead once. With it, each backup reads a still copy of the drive and looks only in folders the drive's change journal says changed. Without it, files are read as they are, and every backup compares every file with the last snapshot.
- **The notification area** stands in for the menu bar, and there's no Keepr menu: Help is `⌘?` or the panel's **Keepr Help**, and **Check for Updates…** is beside **Website** in [Settings](app:settings) and in the panel.
- **Open** opens a file's version from the backup in its own app, where a Mac has Quick Look; **Show in File Explorer** shows where it is.
- **Commands** before and after a backup run in PowerShell.
- **Apps' marks** for backups to skip are a Mac thing: that switch isn't there. Files only in the cloud are still skipped if you choose.
- **Amazon S3** is set up through AWS CloudShell.
<!-- /windows -->
