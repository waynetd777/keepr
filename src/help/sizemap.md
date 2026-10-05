---
title: Size map
kind: screen
screens: [sizemap]
order: 2
summary: Every plan's latest backup as one map, so what takes the space shows at a glance.
---
The size map, from **Size map** in the Space used card on [Backup Plans](app:overview), shows every plan's latest backup as one map.

## Reading the map
Each plan is a block, as big as the files it holds. Inside each plan its folders are blocks too, nested and coloured by how deep they are. With only one plan the map starts at its folders.

The sizes are of the files themselves, before de-duplication and compression, so the map's total won't match the space used.

## Zoom in and out
Click a plan or a folder to zoom into it, and the path bar above the map to come back out. `⌘`-click one to open it in [Restore](app:restore).

## One plan's map
Restore has the same map for one snapshot of one plan: **Size map**, beside **Files** above its list. See [Restore](help:restore).
