# Development

Tauri 2, with a React and TypeScript frontend (Vite) and a Rust backend. The backup engine, `src-tauri/engine/` (the `keepr-engine` crate), does the chunking, packs, encryption, snapshots, restore, checks and pruning, with no knowledge of the app. The app, `src-tauri/src/`, holds the plans and destinations, the job queue and scheduler, places (folders, SMB shares, cloud folders, S3), the Keychain and the menu bar. The frontend is everything you see.

## Commands

| Command | What it does |
|---|---|
| `make dev` | Run the app with hot reload |
| `make check` | The engine's and the app's Rust tests, the TypeScript check and tests, then `make lint` |
| `make lint` | rustfmt, Clippy (warnings are errors), Prettier and ESLint, and a licence header on every source file; checks only |
| `make fmt` | Reformat the Rust and TypeScript, and add the licence header where it's missing |
| `make app` | Bump the version (1.0.0 → 1.0.1) once the current one has a release, and build the .app, signed with the identity in `signing.local` if there is one |
| `make install-app` | Build it and replace the copy in /Applications |
| `make dmg` | Pack the built app into `Keepr.dmg` for a release |
| `make screenshots` | Retake the screenshots in `docs/images/` |
| `make icons` | Redraw the icon artwork and regenerate the icon set |
| `make sign-check` | Show how the installed app is signed |

- Quit the installed Keepr before `make dev`: two Keeprs share `~/Library/Application Support/Keepr` and would both run the schedule.
- On macOS 27, release builds link with Rust's lld against the macOS 26 SDK, because macOS 27's linker sometimes breaks proc-macro builds ("can't find crate"). The Makefile explains.
- Tests never take APFS snapshots of the Mac.

A release build moves to the next patch version once the current one has been released, that is, once `v<version>` is tagged on GitHub (`tools/bump_version.py`, which keeps `tauri.conf.json`, `package.json`, `Cargo.toml` and the lock files in step), so local builds between releases share a version. Every build gets its own build number, stamped on the app and the binary; Settings shows both. For a minor or major step, give the version: `make app VERSION=1.1.0`. Commit the bump with the release.

To publish one: `make app`, `make dmg`, commit and push, then `gh release create v<version> src-tauri/target/release/bundle/dmg/Keepr.dmg`. The README's download link points at the latest release's `Keepr.dmg`, so keep that name. `tools/dmg/make_dmg.py` draws the window's background from `tools/dmg/background.html` (rendered by WebKit) and has Finder lay out the icons, so the first run asks to let the terminal control Finder.

## Where things live

| What | Where |
|---|---|
| Plans, destinations and settings | `~/Library/Application Support/Keepr/config.json` (`KEEPR_DATA` overrides the folder) |
| Run state and history | `~/Library/Application Support/Keepr/state.json` |
| Each run's log | `~/Library/Application Support/Keepr/logs/` |
| Passwords, keys and recovery keys | One login-keychain item, service `Keepr`, account `secrets` |
| Still copies while a backup reads them | `~/Library/Caches/Keepr/still/` |
| A plan's backup | `<destination>/<Plan-Name id>/`: `keepr-repo.json`, `packs/`, `index/`, `snapshots/`, `locks/` |

## Command line

The app's binary (`Keepr.app/Contents/MacOS/Keepr`) also runs without a window:

| Flag | What it does |
|---|---|
| `--back-up <plan id>…` | Back up those plans (incremental, straight away, whatever the plan's wait conditions), print each result; exit 1 if any didn't complete (one with warnings counts as complete) |
| `--verify-restore <plan id> <folder>` | Restore the latest snapshot into the folder and compare it byte for byte with the sources, counting a file that changed since the backup as expected; exit 3 on a mismatch. Saves nothing to Keepr's settings or history, so it can run beside the app |
| `--rename-plan-folder <plan id>` | Rename a plan's backup folder to match its name, and print the new name |
| `--list-destination <destination id>` | List every object in an S3, B2 or R2 destination's bucket, with the count and total size |
| `--copy-plan-password <from id> <to id>` | Copy one plan's password in the Keychain to another |
| `--remove-path <plan id> <path>` | Take a file or folder, by its original full path, out of every snapshot and free the space only it used; a source's own path removes that whole source. Prints the result |

Each exits 1 on an error, with the reason on stderr. An unknown flag, or the right flag with the wrong number of arguments, opens the app instead.

A plan's id and a destination's id are their `id` fields in `~/Library/Application Support/Keepr/config.json`; the app doesn't show them. To list the plans:

```sh
python3 -c "import json,os; [print(p['id'], p['name']) for p in json.load(open(os.path.expanduser('~/Library/Application Support/Keepr/config.json')))['plans']]"
```

Quit Keepr before a command that changes a backup (`--back-up`, `--remove-path`, `--rename-plan-folder`): the app and the command would otherwise both work on the same backup and the same history.

`KEEPR_DATA` points Keepr at another settings folder instead of `~/Library/Application Support/Keepr`, for the app and the commands alike. `KEEPR_NO_SCHEDULE` stops the scheduler and `KEEPR_NO_STILL` the still copies; `KEEPR_SCENE` is the screenshot mode.

## Signing and permissions

Ad-hoc builds make macOS forget Keepr's Full Disk Access and Keychain permission after every rebuild. To stop that:

1. Create a self-signed code-signing certificate (`signing.local.example` says how). Keepr has its own, `Keepr Dev`; don't share one between apps.
2. Copy `signing.local.example` to `signing.local` (untracked) and put the certificate's name in it.
3. Give the installed app Full Disk Access once (System Settings › Privacy & Security).

The Keychain still asks once after each new build, since a self-signed app has no Team ID; Keepr reads its item at startup so the prompt comes then.

## Open at Login

Works only in the app from `make install-app`, not under `make dev`. It registers the installed bundle with SMAppService, which survives rebuilds while the bundle identifier stays `com.wayned.keepr`.

## Help

The window's help drawer (`?`, the toolbar's ? button, or **Keepr Help**, ⌘?, in the Help menu) shows a topic per screen and step-by-step guides from `src/help/*.md`: frontmatter (`title`, `kind`: `screen` or `guide`, `screens`, `order`, `summary`) and a `##` section per part, or per step of a guide. Adding a topic is dropping a file in; `src/help/help.test.ts` (`npx vitest run`, in `make check`) checks every screen has one, every guide step links into the app, and every `app:` and `help:` link goes somewhere. `app:` links open a screen (`app:restore`, `app:plans/new`, `app:destinations/add`); `help:` links another topic. The keyboard list in the drawer is read from `src/help/keyboard.md`.

## Writing the docs

Every new or changed feature updates its guide in `docs/` and its help topic in `src/help/` in the same change. Keep both short:

- Only what someone needs to use the feature: what it does, how to do it, the keys.
- Short sentences in plain words. One idea per sentence or bullet.
- In `docs/`, a `##` section per topic, with `###` headings to split up anything longer. A help topic is shorter: what the screen is for and how to do each thing on it, without the reference detail.
- Developer detail goes here, not in the user guides or the help.

## Screenshots

`make screenshots` retakes every image in `docs/images/`, in both themes, from demo data it makes in `.demo/` (gitignored): sample folders in a demo home folder, a backup folder as the destination, a drive that isn't connected, and a few backups with edits between them. The app runs with `HOME` set to the demo's home, so paths show as `~/Documents`, and nothing outside `.demo/` is read or written. `--fresh` remakes the demo, and `-j N` sets how many apps run side by side (4 by default, each with its own copy of the demo's app data). Scenes are in `tools/screenshots/scenes.json`, and `{home}` in a scene is the demo home's path. `KEEPR_SCENE` is the screenshot mode: the window is invisible and takes no focus, so nothing flashes on screen, and the app saves its webview's snapshot to the file in `KEEPR_SNAPSHOT`; the script draws the window's buttons and rounded corners back on, and takes a blank shot again. It needs Pillow.

The docs show each screenshot in the reader's theme with a `<picture>` element, linked to `docs/images/index.md`, which lists them all. A new screenshot goes in that list too.
