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
An encrypted plan's first backup makes a recovery key, which opens the backup if you forget the password. [Backup Plans](app:overview) reminds you with **Show recovery key** until you click **I've saved it**; the plan's **Save recovery key…** shows it too. Keep it somewhere safe away from this computer, such as a password manager.

<!-- mac -->
## Give Keepr Full Disk Access
Keepr needs Full Disk Access to back up every folder. Turn it on in System Settings › Privacy & Security › Full Disk Access. When macOS asks about the Keychain, click **Always Allow**. A backup's log in [Activity](app:activity) says what couldn't be read.
<!-- /mac -->

<!-- windows -->
## Install Keepr Helper
On a Mac, each backup reads a still copy of the disk and looks only in folders that changed. On Windows that takes **Keepr Helper**, a small Windows service: turn on **Still copies and the change journal** in [Settings](app:settings), and give Windows an administrator's go-ahead once. Without it, backups still work, reading files as they are.
<!-- /windows -->

## Open at Login
Turn on **Open at Login** in [Settings](app:settings), so Keepr starts in the <!-- mac -->menu bar when you log in<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->notification area when you sign in<!-- /windows --><!-- github --> (Windows)<!-- /github --> and keeps backing up. Closing the window doesn't stop it; only Quit Keepr does.
