---
title: Choose what's backed up and kept
kind: guide
order: 6
summary: Pick a plan's folders, leave out what never needs a backup, set when it runs and how long versions are kept.
---
## Open the plan
Click **Edit…** on a plan's card in [Backup Plans](app:overview), or [make a plan](app:plans/new). Changes to a plan are saved as you make them; a new plan is made when you click **Create and back up**.

## Add what to back up
Under **What to keep**, **Add a source** adds folders on this computer or a drive, a folder on an SMB share, a folder in a cloud service, or an S3 bucket. Keepr only reads them. [Backup Plans](app:overview) lists each plan's sources on its card.

## Leave things out
Under **What to leave out**, type a rule in **+ Rule** and press Return: `*.iso` for files by name, `build/` for folders by name, or a folder's path from `~`. The switches below skip what `.gitignore` files leave out, files only in the cloud, and files over a size. [Size Map](app:sizemap) shows what's taking the space, if you're not sure what to leave out.

## Choose when
Under **When**, pick **Every 15 min**, **Hourly**, **Daily**, **Weekly** or **Only when I ask**, and whether to wait on battery or a <!-- mac -->personal hotspot<!-- /mac --><!-- github --> or <!-- /github --><!-- windows -->metered connection<!-- /windows -->. **Back up now** on [Backup Plans](app:overview) always runs straight away.

## Choose how long versions are kept
Under **Versions to keep**, set how long **Every backup**, **One a day**, **One a week** and **One a month** are kept; the sentence beneath says what that keeps. Older snapshots are thinned out in a tidy-up after a backup, which [Activity](app:activity) shows.
