---
title: How backups work
kind: screen
screens: []
order: 10
summary: What Keepr does in the background, and how a backup is stored at its destination.
---
Keepr backs up on its own, whether or not its window is open.

## Still copies of the startup disk
For folders on the Mac's own disk, Keepr takes a still copy of the disk (an APFS snapshot, as Time Machine does) and backs up from that, so files that change during the backup are copied as they were at one moment. The copy is deleted afterwards. Drives, shares, cloud folders and buckets are read as they are.

## Only what changed
For folders on the Mac's own disk, Keepr asks macOS which folders have changed since the last backup, and only looks in those. A full re-read looks at everything.

## Stored once, however many snapshots
Each plan's backup is a folder at its destination. Files are split into pieces by their content, and each piece is stored once, however many files and snapshots have it, so a file that moves or is copied costs nothing more. Pieces are compressed, unless they already are.

## Encrypted with a key of its own
An encrypted plan's pieces and snapshots are encrypted under a random key, which is itself locked with your password and, separately, with the recovery key. That's why changing the password rewrites nothing else.

## Never a broken snapshot
A snapshot is written last, after everything it needs, so a backup that's stopped or cut off never leaves a broken snapshot.

## Tidying up
After a backup, at most once a day, Keepr thins out old snapshots by the plan's Versions to keep, and frees the space nothing uses any more.
