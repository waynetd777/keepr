---
title: Backup Plans
kind: screen
screens: [overview]
order: 1
summary: The home screen: every plan's last and next backup, the space it uses, and what needs attention.
---
Backup Plans is the home screen. The headline says whether everything is kept, something is backing up, or a plan needs attention.

## A plan's card
Each plan has a card with its sources and destination, and:

- **Last backup**, **Backup size now** (everything the plan takes at its destination, every version included), **Oldest version** and **Next**.
- A chart of the last 30 days, one bar per day, taller for more data sent. A red bar is a backup that didn't finish, a stub a day without one. Hover over a bar for that day's backups.
- **Edit…** for the plan's settings, **Restore…**, and a button to back it up now, or stop it while it runs.

Drag a card by its grip to change the order of your plans; Restore, the menu bar and the rest follow it.

## When a plan needs attention
A plan that has failed, is waiting (for its drive, say) or hasn't backed up for a while shows as a compact red or amber card with the reason and **Try again**.

- **Waiting**: the backup is due but can't run yet, because its destination isn't connected, the battery is low or the Mac is on a hotspot, as the plan says. Keepr tries again every few minutes.
- **Failed**: the backup didn't finish. Its log in [Activity](app:activity) says why.
- **Hasn't backed up for a while**: Keepr also warns you about it, as set in [Settings](app:settings).

## Space used and Recent
On the right, **Space used** shows the space taken across every plan, and each destination with its free space. **Recent** has the last few backups; **All activity** opens [Activity](app:activity).

**Size map**, in the Space used card, shows every plan's latest backup as one map. See [Size map](help:sizemap).

## Add a plan
**New plan** in the toolbar, **Add a plan** at the end of the cards, or `⌘N` makes a [new plan](app:plans/new). Its first backup starts as soon as you create it. See [A backup plan](help:plan).

## Back up now
**Back up now** in the toolbar (`⌘B`) backs up every plan that's on, straight away, whatever their schedules and conditions say. While a backup runs, the toolbar shows its progress and a **Stop** button.

## The first run
With no plans yet, Backup Plans is a Welcome screen with two steps: **Add a destination**, where the backups go, then **Make a plan**, what to back up. The [Start here](help:guide-start) guide walks through both.
