---
title: Ask
kind: screen
screens: []
order: 13
summary: Ask an AI about what takes the space in your backups and why a run failed, and have it suggest changes to a plan for you to check and apply.
---
**Ask**, in the toolbar (`⌘J`), opens a chat beside the window. Ask what takes the space, why a backup stored so much, what went wrong with a run, or how to do something in Keepr. The **Ask** buttons on the Size Map and in Activity start a chat about that folder or run.

## Who answers
Ask uses an AI tool already installed and signed in on this Mac: Claude Code, Codex, Antigravity or GitHub Copilot. Choose the model at the top of the chat. With none installed, Ask is greyed out and says what to install.

## What's sent
Your question goes to that tool with what Keepr knows: each plan's settings and status, its biggest folders and what grew in them, the space they take stored, folder names that repeat (such as node_modules in 40 places), and the recent runs. Folders are named, with their file types by size, when they last changed and whether a project's .gitignore lists them. Files aren't named, and nothing of their contents is sent. An Ask button for a run adds that run's message and log. **What's sent** in the chat shows the exact text.

To keep a plan out of it, turn on **Leave this plan out of Ask** in the plan; Ask then sees only its name. To stop Ask altogether, turn it off in [Settings](app:settings).

## Changes it suggests
When a change to a plan would help, the answer can suggest one: things to leave out, a size limit, how long to keep versions, or the schedule. Keepr shows what it would do before anything changes: the settings before and after, what in the latest backup would be left out and how big it is, how many snapshots the next tidy-up would remove, and warnings about anything that changed this week or isn't a folder tools rebuild. **Apply** saves it as the plan editor would.

Ask can't change the folders a plan backs up, its destination or its encryption, and can't remove snapshots, tidy up or remove a plan.

## Undo
Each change applied from Ask is in [Activity](app:activity) as a plan change, with **Undo** to put the plan back as it was.
