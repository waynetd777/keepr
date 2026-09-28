# Development

Tauri 2, with a React and TypeScript frontend (Vite) and a Rust backend. The backup engine, `src-tauri/engine/` (the `keepr-engine` crate), does the chunking, packs, encryption, snapshots, restore, checks and pruning, with no knowledge of the app. The app, `src-tauri/src/`, holds the plans and destinations, the job queue and scheduler, places (folders, SMB shares, cloud folders, S3), the Keychain and the menu bar. The frontend is everything you see.

## Commands

| Command | What it does |
|---|---|
| `make dev` | Build the help, then run the app with hot reload |
| `make check` | The engine's and the app's Rust tests, and the TypeScript check |
| `make app` | Bump the version (1.0.0 → 1.0.1) and build the .app, signed with the identity in `signing.local` if there is one |
| `make install-app` | Build it and replace the copy in /Applications |
| `make dmg` | Pack the built app into `Keepr.dmg` for a release |
| `make help` | Build the Help Book from `docs/` |
| `make screenshots` | Retake the screenshots in `docs/images/` |
| `make icons` | Redraw the icon artwork and regenerate the icon set |
| `make sign-check` | Show how the installed app is signed |

- Quit the installed Keepr before `make dev`: two Keeprs share `~/Library/Application Support/Keepr` and would both run the schedule.
- On macOS 27, release builds link with Rust's lld against the macOS 26 SDK, because macOS 27's linker sometimes breaks proc-macro builds ("can't find crate"). The Makefile explains.
- Tests never take APFS snapshots of the Mac.

Every release build gets the next patch version (`tools/bump_version.py`, which keeps `tauri.conf.json`, `package.json`, `Cargo.toml` and the lock files in step) and one build number, stamped on the app, its Help Book and the binary; Settings shows both. For a minor or major step, give the version: `make app VERSION=1.1.0`. Commit the bump with the release.

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
| `--back-up <plan id>…` | Back up those plans, print the results; exit 1 if any didn't complete |
| `--verify-restore <plan id> <folder>` | Restore the latest snapshot into the folder and compare it byte for byte with the sources; exit 3 on a mismatch |
| `--rename-plan-folder <plan id>` | Rename a plan's backup folder to match its name (with the app not running) |
| `--list-destination <destination id>` | List what's in an S3 destination |
| `--copy-plan-password <from id> <to id>` | Copy one plan's password in the Keychain to another |
| `--remove-path <plan id> <path>` | Take a path out of every snapshot |

`KEEPR_NO_SCHEDULE` stops the scheduler and `KEEPR_NO_STILL` the still copies; `KEEPR_SCENE` is the screenshot mode.

## Signing and permissions

Ad-hoc builds make macOS forget Keepr's Full Disk Access and Keychain permission after every rebuild. To stop that:

1. Create a self-signed code-signing certificate (`signing.local.example` says how). Keepr has its own, `Keepr Dev`; don't share one between apps.
2. Copy `signing.local.example` to `signing.local` (untracked) and put the certificate's name in it.
3. Give the installed app Full Disk Access once (System Settings › Privacy & Security).

The Keychain still asks once after each new build, since a self-signed app has no Team ID; Keepr reads its item at startup so the prompt comes then.

## Open at Login

Works only in the app from `make install-app`, not under `make dev`. It registers the installed bundle with SMAppService, which survives rebuilds while the bundle identifier stays `com.wayned.keepr`.

## Help

The app's Help menu (**Keepr Help**, ⌘?) opens an Apple Help Book built from the user guides in `docs/` (all but this one). `tools/helpbook.py` converts them with pandoc, a page per `##` section, styled like the app, with search indexes from `hiutil`. The release build runs it and copies the book into the app's Resources; `src-tauri/Info.plist` registers it. The book carries the app's version, which every release build bumps, because macOS keeps showing a cached book until its version changes. `make install-app` also clears the Help cache (`~/Library/Caches/com.apple.helpd/`) and re-registers the app, since the old book cached at the same path otherwise makes Help show "The selected content is currently unavailable". Under `make dev`, Help opens the pages in the browser instead.

## Writing the docs

Every new or changed feature updates its guide in `docs/` in the same change, and so the help. Keep them short:

- Only what someone needs to use the feature: what it does, how to do it, the keys.
- Short sentences in plain words. One idea per sentence or bullet.
- A `##` section per topic: each becomes a help page, and its first sentence is the page's summary in Help search. `###` headings split up anything longer.
- Developer detail goes here, not in the user guides, and the guides don't link here: Help hasn't this page, so `make help` stops on a link to it.

Run `make help` to check the result.

## Screenshots

`make screenshots` retakes every image in `docs/images/`, in both themes, from demo data it makes in `.demo/` (gitignored): sample folders in a demo home folder, a backup folder as the destination, a drive that isn't connected, and a few backups with edits between them. The app runs with `HOME` set to the demo's home, so paths show as `~/Documents`, and nothing outside `.demo/` is read or written. `--fresh` remakes the demo; scenes are in `tools/screenshots/scenes.json`, and `{home}` in a scene is the demo home's path. It needs Screen Recording permission for the terminal, Pillow and swiftc.

The docs show each screenshot in the reader's theme with a `<picture>` element, linked to `docs/images/index.md`, which lists them all. A new screenshot goes in that list too.
