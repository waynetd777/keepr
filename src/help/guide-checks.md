---
title: Make sure a backup restores
kind: guide
order: 10
summary: Check that the stored data reads back, re-read every file now and then, and try a restore into another folder.
---
## Checks run on their own
Each plan checks its stored data can be read back: a sample each week and all of it each month, unless you change **Check stored data can be read back** in the plan (from [Backup Plans](app:overview), **Edit…**). A check that finds a problem always tells you.

## Check now
The ⋯ menu in a plan (from [Backup Plans](app:overview), **Edit…**) runs a check or a full re-read straight away. [Activity](app:activity) shows each check with what it read back.

## Re-read every file now and then
Each backup reads only what changed. **Re-read every file, not just changed ones**, weekly by default, reads everything again, so a file whose change slipped past still gets its new version. It stores nothing it already has. Change it in the plan from [Backup Plans](app:overview).

## Try a restore
Now and then, restore something into another folder to see it come back. In [Restore](app:restore), tick a folder, choose **Another folder…** under **Restore to**, and click **Restore**; then open the files there.
