---
title: Make backups smaller
kind: guide
order: 7
summary: Find what takes the space in the backups, leave it out, take it out of past backups, and keep fewer versions.
---
## Find what takes the space
[Size Map](app:sizemap) shows every plan's latest backup on the left and the space all its versions take on the right. Click a big block to zoom into it. Outlines show what grew this week.

## Look in one backup
⌘-click a folder in [Size Map](app:sizemap) to open it in Restore, where every file has its versions. [Restore](app:restore)'s own **Size map**, beside **Files**, maps one snapshot.

## Leave it out from now on
Add a rule to the plan's **What to leave out**: open the plan from [Backup Plans](app:overview) with **Edit…**. A folder of caches, downloads or build output is the usual find. [Ask](app:sizemap) can suggest what to leave out: click **Ask** above either map.

## Take it out of past backups
Leaving something out keeps its old versions until they're thinned out. To free the space now, pick it in [Restore](app:restore) and click **Remove from backup…**. Keepr asks twice, as it can't be undone.

## Keep fewer versions
Shorter **Versions to keep** in the plan (from [Backup Plans](app:overview), **Edit…**) keep fewer old snapshots. The next tidy-up frees what nothing needs any more, and [Destinations](app:destinations) shows the space come back.
