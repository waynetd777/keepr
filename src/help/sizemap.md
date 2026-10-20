---
title: Size map
kind: screen
screens: [sizemap]
order: 2
summary: Every plan's latest backup, and the space all its versions take, as two maps side by side, so what takes the space shows at a glance.
---
Size Map (`⌘2`, or **Size map** in the Space used card on [Backup Plans](app:overview)) shows two maps side by side: on the left every plan's latest backup, on the right the space every version takes in the destination.

## Reading the map
Each plan is a block, as big as the files it holds. Inside each plan its folders are blocks too, nested and coloured by how deep they are. With only one plan the map starts at its folders.

The left map's sizes are of the files themselves, before de-duplication and compression, so its total won't match the space used.

## Space used
The right map shares out the space the backups take, after de-duplication and compression, among the folders it came from, so its total is the Space used card's figure. Each piece of data counts once, in the folder of the file that first stored it, so a copy of a file stores nothing and adds nothing here. Its counts are of file versions, not files: every changed version of a file that stored new data counts.

A folder you've deleted still shows here while its old versions are kept. Two blocks aren't folders: **Folder listings** is what every snapshot records about its folders and files, and **Not used by any snapshot** is data from versions that have been removed, which the next tidy-up frees.

Both maps zoom together. While a map is being read it says so, with **Stop**; after a backup only that plan's map is read again, and the others stay on screen. A folder that's only on one of them (deleted since, or holding only copies) says so on the other.

## What grew this week
On the left, each folder is compared with the backup from a week before. On the right, a folder's growth is what this week's backups stored there. Where the growth landed is outlined, with a tag saying how much it grew or shrank: the folder it's concentrated in, rather than every folder above it. The path bar's total says how much the whole map grew, and pointing at a folder shows its own change. Changes under 10 MB aren't tagged. Until a plan has a week of backups, the map compares with its first backup and says since when.

## Zoom in and out
Click a plan or a folder to zoom into it, and the path bar above either map to come back out. ⌘-click one to open it in [Restore](app:restore).

## Ask about a folder
**Ask**, in the path bar above either map, asks an AI what takes the space in the folder on screen, with its folders' sizes and growth. See [Ask](help:ask).

## One plan's map
Restore has the same map for one snapshot of one plan: **Size map**, beside **Files** above its list. See [Restore](help:restore).
