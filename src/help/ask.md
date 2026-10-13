---
title: Ask
kind: screen
screens: []
order: 13
summary: Ask an AI about what takes the space in your backups and why a run failed, and have it suggest changes to a plan for you to check and apply.
---
**Ask**, in the toolbar (`⌘J`), opens a chat beside the window. Ask what takes the space, why a backup stored so much, what went wrong with a run, or how to do something in Keepr. The **Ask** buttons on the Size Map and in Activity start a chat about that folder or run.

## Who answers
Ask uses an AI tool already installed and signed in on this Mac: Claude Code, Codex, Antigravity or GitHub Copilot. Choose the model at the top of the chat. With none installed, the question box and the Ask buttons are greyed out and Ask says what to install; sign in to it, then reopen Keepr.

## What's sent
With the first question of each chat, Ask sends the AI tool Keepr's instructions, this help, and what Keepr knows about your backups. Follow-up questions don't send the figures again: only the question with Keepr's instructions and this help (GitHub Copilot keeps those from the first question), plus what an Ask button adds. Switching to a different AI tool sends the figures again, since it hasn't seen them.

- **Each plan:** its name and id, whether it's on, its schedule, whether it's encrypted, the folders it backs up (a share by its server's name, an S3 bucket only as "an S3 bucket"), its destination's name, what it leaves out (its rules, and whether it leaves out what .gitignore lists, cloud-only files, what apps mark and files over a size), how long it keeps versions, how many snapshots it has and since when, when it last backed up, and its latest backup's size, file count, growth this week and space stored.
- **Its biggest folders**, and those that grew most this week: each folder's path, its size, how much it grew, the space it takes stored and stored this week, its file types by share of size (such as "60% .js"), when its newest file changed, how much of it changed in the last 30 days, and whether its project's .gitignore lists it.
- **Folder names that repeat**, with their total size and how many there are, such as node_modules in 40 places.
- **Folders whose old versions are still stored** though they're not in the latest backup, with the space they take, and the space the folder listings and data no snapshot uses take.
- **The last 15 runs:** when, which plan, what kind, the result, the space stored and the run's message, with any file paths taken out.
- **The space used** by the plans Ask sees, and the date and time.
- **Why a plan's backup couldn't be read**, if it couldn't, with any paths taken out.

Files aren't named, except where one of your rules names one, and nothing of their contents is sent. An Ask button adds what it's about: on the Size Map, the folders in the folder on screen with their sizes and growth; on a run in Activity, its whole message, how many files changed, the space stored and the last 40 lines of its log, which can name files.

To keep a plan out of it, tick **Leave this plan out of Ask** in the plan: Ask then sees only its name, the plan's Ask buttons are greyed out, and Ask can't change it. To stop Ask altogether, turn it off in [Settings](app:settings); then nothing is sent.

## Changes it suggests
When a change to a plan would help, the answer can suggest one: things to leave out, a size limit, how long to keep versions, or the schedule. Keepr shows what it would do before anything changes: the settings before and after, what in the latest backup would be left out and how big it is, how many snapshots the next tidy-up would remove, and warnings, such as anything that changed this week, a folder tools don't rebuild, a change that leaves out a quarter of the plan or more, or one that stops scheduled backups. **Apply** saves it as the plan editor would.

Ask can't change the folders a plan backs up, its destination or its encryption, and can't remove snapshots, tidy up or remove a plan. Keeping fewer versions does mean the next tidy-up removes snapshots, which the card says first.

## Undo
**Undo** is on the card once a change is applied, and each change applied from Ask is in [Activity](app:activity) as a plan change, with **Undo**. It puts back only the settings the change made; if you've changed those since, Keepr says so and leaves the plan as it is.
