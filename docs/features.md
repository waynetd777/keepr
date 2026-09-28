# Features

Keepr is a backup app for macOS. It keeps every version of the files you choose, on a drive, a NAS, a cloud folder or an S3 bucket, and lets you go back to any moment and restore any file as it was then. The first backup copies everything; after that, each one stores only what changed, and every snapshot still restores as a complete copy. Backups can be encrypted, so the place that holds them can't read them.

<a href="images/index.md#features"><picture><source media="(prefers-color-scheme: dark)" srcset="images/overview-dark.png"><img alt="Backup Plans: three plans with their last and next backups, the space they use, and the destinations" src="images/overview-light.png"></picture></a>

| Page | What's in it |
|---|---|
| [Backup plans](plans.md) | What to back up and leave out, when, the versions to keep, encryption |
| [Restore](restore.md) | Going back to a moment, a file's versions, comparing, finding a file in any backup |
| [Destinations](destinations.md) | Folders and drives, SMB shares, cloud folders, S3, Backblaze B2 and Cloudflare R2 |
| [Day to day](day-to-day.md) | The menu bar, Activity, notifications, Settings, and how the backups are stored |

## First run

Keepr isn't notarised by Apple, so the first time you open it macOS says it can't check the app for malicious software. Click **Done**, open System Settings › Privacy & Security, scroll down and click **Open Anyway** beside Keepr, then **Open Anyway** again and enter your password. It opens normally after that.

The Welcome screen has two steps: **Add a destination**, where the backups go, then **Make a plan**, what to back up. A new plan's first backup starts as soon as you create it.

macOS asks for some permissions along the way:

- **Full Disk Access.** Keepr needs it to back up every folder and to read a still copy of the startup disk. Turn it on in System Settings › Privacy & Security › Full Disk Access; Keepr doesn't ask for it itself. Without it, macOS asks separately for Documents, Desktop and Downloads.
- **Keychain.** Keepr keeps its passwords and keys in one item in your login keychain, and reads it when it starts, so any prompt comes then, while you're there. Click **Always Allow**.
- **Local network.** Asked the first time Keepr looks for SMB servers on your network.
- **Removable and network volumes.** Asked the first time Keepr reads or writes a drive or share.
- **Notifications.** Asked the first time Keepr has something to tell you.

To have Keepr start when you log in, turn on **Open at Login** in [Settings](day-to-day.md#settings).

## The window

The sidebar has **Backup Plans** (⌘1), **Restore** (⌘2), **Activity** (⌘3) and **Destinations** (⌘4), with **Settings** (⌘,) at the bottom. The toolbar has **Back** (⌘[) and **Forward** (⌘]), a search box, **Find a file in any backup** (⌘K), and **Back up now** (⌘B), which backs up every plan that's on. While a backup runs, the toolbar shows its progress and a **Stop** button.

Closing the window doesn't quit Keepr: it leaves the Dock and keeps backing up from the [menu bar](day-to-day.md#the-menu-bar). Click Keepr in the Dock or the menu bar to bring the window back, and use **Quit Keepr** in the menu bar to stop it.

## Backup Plans

Backup Plans is the home screen. The headline says whether everything is kept, something is backing up, or a plan needs attention.

Each plan has a card with its sources and destination, and:

- **Last backup**, **Backup size now** (everything the plan takes at its destination, every version included), **Oldest version** and **Next**.
- A chart of the last 30 days, one bar per day, taller for more data sent. A red bar is a backup that didn't finish, a stub a day without one. Hover over a bar for that day's backups.
- **Edit…** for the plan's settings, **Restore…**, and a button to back it up now, or stop it while it runs.

A plan that has failed, is waiting (for its drive, say) or hasn't backed up for a while shows as a compact red or amber card with the reason and **Try again**. Drag a card by its grip to change the order of your plans; Restore, the menu bar and the rest follow it. **Add a plan** (⌘N) is at the end.

On the right are **Space used**, across every plan, each destination with its free space, and **Recent**, the last few backups; **All activity** opens [Activity](day-to-day.md#activity).

## Requirements

A Mac with Apple silicon (M1 or later) and macOS 13 or later.
