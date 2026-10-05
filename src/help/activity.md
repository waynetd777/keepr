---
title: Activity
kind: screen
screens: [activity]
order: 6
summary: The backup that's running, step by step, and the history of every backup, check and restore.
---
Activity (`⌘3`) shows what Keepr is doing now and what it has done.

## The backup that's running
Step by step: looking for changes, comparing, packing (splitting, skipping what's already kept, compressing and encrypting), sending, and confirming. It shows the progress, the speed and the time left, and how much was already stored against how much is new.

**Pause** holds it; **Stop** ends it once the file it's on is done. If the Mac sleeps or the destination drops off, the next backup picks up the work. Nothing half-sent counts as a snapshot.

## History
Every backup, full re-read, check, tidy-up and restore, with when it ran, how many files and how much data, how long it took and its result. **All**, **Problems** and **Restores** filter it.

## A run's log
Click a row for its log, which says what was backed up, skipped or couldn't be read, and the output of the plan's before and after commands. Files that couldn't be read are often a sign Keepr needs Full Disk Access; see [Start here](help:guide-start).

## One job at a time
Backups, restores, checks and tidy-ups queue up and run one after another.
