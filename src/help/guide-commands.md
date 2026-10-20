---
title: Run something before or after a backup
kind: guide
order: 9
summary: Have a plan run a command first, such as an export into a folder it backs up, and another when it ends, told how it went.
---
## Before each backup
Open a plan from [Backup Plans](app:overview) with **Edit…** and type a command under **Before each backup**, such as one that exports a database or downloads a device's own backup into a folder the plan backs up. It runs in <!-- mac -->your login shell (zsh)<!-- /mac --><!-- github --> on a Mac, or <!-- /github --><!-- windows -->PowerShell<!-- /windows -->.

## If it fails
Choose under **If it fails**: back up anyway and mark the backup with a warning, or don't back up. While the command runs, [Activity](app:activity) says so; nothing is read until it ends.

## After each backup
**After each backup**, in the same plan from [Backup Plans](app:overview), runs a command when the backup ends, however it went, such as one that tells a home server. It's told how it went in environment variables: <!-- mac -->`$KEEPR_RESULT`, `$KEEPR_PLAN`, `$KEEPR_MESSAGE` and `$KEEPR_LAST_SUCCESS`<!-- /mac --><!-- github --> (Mac) or <!-- /github --><!-- windows -->`$env:KEEPR_RESULT`, `$env:KEEPR_PLAN`, `$env:KEEPR_MESSAGE` and `$env:KEEPR_LAST_SUCCESS`<!-- /windows -->.

## Read what they said
Both commands' output goes into the backup's log: click the backup in [Activity](app:activity) to read it, with whether each command finished or failed.
