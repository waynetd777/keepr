---
title: Command line
kind: screen
screens: []
order: 12
summary: Back up, check, restore and see how plans are doing from Terminal, for scripts and health checks.
---
Keepr also runs in Terminal as `keepr`, for scripts, scheduled jobs and checking on your backups from elsewhere. `keepr --help` lists the commands, and `keepr <command> --help` explains one.

## Set it up
Link the app's program into a folder on your PATH, once:

`ln -s /Applications/Keepr.app/Contents/MacOS/Keepr ~/.local/bin/keepr`

For tab completion, `keepr completions zsh` prints a script for zsh (bash and fish work too).

## See how things are
- `keepr plans` and `keepr destinations` list them, with their ids.
- `keepr status` shows each plan's status, last and next backup. It exits 1 if any plan needs attention, so a monitor can run it.
- `keepr snapshots <plan>` lists a plan's snapshots.

A plan is named by its name (in quotes if it has spaces) or its id. Add `--json` for output a script can read.

## Back up, check and tidy
- `keepr back-up <plan>` backs up now; `--all` backs up every plan that's on, `--full` reads every file again.
- `keepr check <plan>` reads a sample of the stored data back; `--all-data` reads all of it.
- `keepr tidy <plan>` applies the plan's [versions to keep](help:plan) now.

These change the backup, so they refuse while the Keepr app is open: quit it first (Quit Keepr in the menu bar panel). Ctrl-C stops one at a safe point, as Stop does.

## Restore
`keepr restore <plan> <path> --to <folder>` restores a file or folder, by its full path on the Mac, from the newest snapshot into that folder. `--original` puts it back where it was instead, `--snapshot <id>` picks an older snapshot, and `--conflict replace` or `skip` says what to do with a file already there (the default keeps both).

`keepr verify-restore <plan> <folder>` restores the newest snapshot into an empty folder and compares every file with the original, to prove the backup can be restored.

## Remove from the backup
`keepr remove-path <plan> <path> --yes` takes a file or folder out of every snapshot and frees its space, as [Remove from backup](app:restore) does. It can't be undone, so it asks for `--yes`.
