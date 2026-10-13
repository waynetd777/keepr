---
title: Restore
kind: screen
screens: [restore]
order: 4
summary: Go back to any backup, see a file's versions, compare them, and put back what you choose.
---
Restore shows your files as they were at any backup, and puts back whichever you choose: one file, a folder, or everything.

## Go back to a moment
The tabs along the top are your plans. Pick one, and it opens at its latest snapshot.

- **Earlier** and **Later**, or `←` and `→`, step through the snapshots; **Latest** goes back to the newest.
- The strip shows as many days as fit, up to ten, one bar per snapshot, taller for more new data. Click a bar to go to that snapshot.
- **Find in** searches this snapshot by name.
- **Show deleted files** also lists files that were in an earlier snapshot but not this one, however long ago they went, struck through. A deleted folder opens to show what it last held. At the latest snapshot they show as **Not on Mac**, ready to restore or to remove from the backup.

Each file shows whether it's **New**, **Changed** or **Deleted** in this snapshot. At the latest snapshot the list is also checked against your Mac now: **Not on Mac** is in the backup but gone from the Mac, **Not backed up** is on the Mac but not in the backup yet.

## A file's versions
Click a file to see every version kept of it, newest first. The one in the snapshot you're looking at is marked **In this snapshot**.

- **Quick Look** (or `Space`) shows the version without restoring it. A big file is copied out of the backup first; **Cancel** stops that.
- **Compare with current** compares it with the file on the Mac now: the lines that differ for a text file, or whether they're the same.
- **Restore this version** puts it back.

**Show in Finder** opens the folder it's in on your Mac. Right-click a file or folder for all of these in one place, and for several ticked items at once.

## Put files back
Tick what you want back: files, folders, or a mix. A ticked folder comes back with everything in it. Ticks are for one snapshot: going to another clears them, and so does a removal or tidy-up that rewrites the snapshots. Then choose, in the bar at the bottom:

- **Restore to**: **Original location**, or **Another folder…**.
- **If a file is already there**: **Keep both (add "restored")**, **Replace it** or **Skip it**.

Click **Restore**. A file only takes its real name once it's complete, so a stopped restore never leaves half a file. A deleted file comes from the last snapshot that had it. The restore shows in [Activity](app:activity), and in the top bar with **Pause** and **Stop**. Files backed up from an S3 bucket can only be restored to a folder on the Mac.

## See what takes the space
**Size map**, beside **Files** above the list, shows the snapshot as a map: each folder a block as big as what it holds. Click a folder to zoom in, the path bar to come back out. `⌘`-click a folder, or click a dashed block, to find it in **Files**. For every plan in one map, see [Size map](help:sizemap).

## Remove something from a backup
**Remove from backup…**, under a file's or folder's versions, takes it out of every snapshot, to free the space or because it should never have been backed up. Keepr asks twice; it can't be undone. To stop it being backed up again, add a rule to the plan's What to leave out.

Right-click a ticked item to remove all the ticked items at once; Keepr lists what will go before it asks. A folder with only some of what's in it ticked isn't removed whole: tick the folder itself to remove all of it. A folder, share or bucket the plan backs up can't be removed from here: take it out of the plan's folders instead, which asks whether to delete its backed-up data.
