---
title: Start here
kind: guide
order: 1
summary: Set Keepr up: somewhere to keep backups, a plan, the recovery key and the permissions it needs.
---
## Add a destination
A destination is where the backups go: a drive, a NAS, a cloud folder or an S3 bucket. [Add a destination](app:destinations/add), pick the kind of place, fill it in and click **Test**, then **Add destination**.

## Make a plan
A plan is what to back up, and when. [Make a plan](app:plans/new), add the folders to back up with **Add a source**, and choose the destination under **Where**. The defaults (hourly, encrypted, thinned out over time) suit most people. Click **Create and back up**; the first backup starts straight away.

## Save the recovery key
An encrypted plan's first backup makes a recovery key, which opens the backup if you forget the password. [Backup Plans](app:overview) reminds you with **Show recovery key** until you click **I've saved it**; the plan's **Save recovery key…** shows it too. Keep it somewhere safe away from the Mac, such as a password manager.

## Give Keepr Full Disk Access
Keepr needs Full Disk Access to back up every folder. Turn it on in System Settings › Privacy & Security › Full Disk Access. When macOS asks about the Keychain, click **Always Allow**. A backup's log in [Activity](app:activity) says what couldn't be read.

## Open at Login
Turn on **Open at Login** in [Settings](app:settings), so Keepr starts in the menu bar when you log in and keeps backing up. Closing the window doesn't stop it; only Quit Keepr does.
