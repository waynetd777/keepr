# Keepr — build, sign and install the app.

APP      := src-tauri/target/release/bundle/macos/Keepr.app

# The signing certificate is named in signing.local, which is untracked: the name of a keychain
# identity is local to the machine that holds it. Copy signing.local.example and put your own
# self-signed certificate's name in it. Without one the build is signed ad hoc, and macOS forgets
# Keepr's permissions (Full Disk Access, Keychain items) after every rebuild.
-include signing.local
SIGN_ID  := $(APPLE_SIGNING_IDENTITY)
# "-" is an ad-hoc signature: an empty identity makes the bundler fail instead.
export APPLE_SIGNING_IDENTITY := $(if $(SIGN_ID),$(SIGN_ID),-)

.PHONY: check test app install-app dev icons sign-check screenshots

## cargo test (the engine and the app) + TypeScript type-check.
check:
	cd src-tauri && cargo test -p keepr-engine && cargo test --lib -p keepr
	npx tsc --noEmit -p tsconfig.json

test: check

# Release builds strip the builder's home directory out of the binary (Rust bakes absolute
# paths into panic metadata). Debug builds skip this so `make dev` keeps its incremental cache.
RELEASE_RUSTFLAGS := --remap-path-prefix=$(HOME)=/build --remap-path-scope=object
# macOS 27's linker (ld-27037) sometimes writes a library whose string table dyld refuses
# ("mis-aligned LINKEDIT string pool"), so rustc can't load a proc macro it has just built and
# stops with "can't find crate" (E0463). Release builds link with Rust's own lld instead; lld
# can't read the macOS 27 SDK's .tbd files, so it links against the 26.x SDK when that's there.
LLD_DIR := $(shell rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/gcc-ld
OLD_SDK := $(wildcard /Library/Developer/CommandLineTools/SDKs/MacOSX26*.sdk)
ifneq ($(OLD_SDK),)
RELEASE_RUSTFLAGS += -Clink-arg=-fuse-ld=lld -Clink-arg=-B$(LLD_DIR)
RELEASE_ENV := SDKROOT=$(lastword $(OLD_SDK))
endif

# Each release build bumps the version (tools/bump_version.py: 0.1.0 → 0.1.1) and gets its own
# build number, the same on the app (CFBundleVersion) and the binary (Settings shows both).
BUILD := $(shell date +%Y%m%d.%H%M%S)

## Bump the version and build the .app, signed with the identity in signing.local when there is one.
app:
	@echo "version $$(python3 tools/bump_version.py), build $(BUILD)"
	KEEPR_BUILD=$(BUILD) $(RELEASE_ENV) RUSTFLAGS="$(RELEASE_RUSTFLAGS)" npm run tauri build -- --config '{"bundle":{"macOS":{"bundleVersion":"$(BUILD)"}}}'
	@if [ -n "$(SIGN_ID)" ]; then \
	  codesign -dv --verbose=2 "$(APP)" 2>&1 | grep -E "^Authority=$(SIGN_ID)" >/dev/null \
	    && echo "signed with $(SIGN_ID)" \
	    || { echo "WARNING: app is not signed with $(SIGN_ID)"; exit 1; }; \
	else echo "note: no signing.local, so the app is signed ad hoc"; fi

## Build and replace /Applications/Keepr.app.
install-app: app
	@pkill -x Keepr 2>/dev/null || true
	@rm -rf "/Applications/Keepr.app"
	@ditto "$(APP)" "/Applications/Keepr.app"
	@echo "installed /Applications/Keepr.app"

## Redraw design/icon.png and the tray template, then regenerate the Tauri icon set.
icons:
	@python3 tools/make_icons.py
	@npx tauri icon design/icon.png >/dev/null
	@rm -rf src-tauri/icons/android src-tauri/icons/ios
	@echo "regenerated src-tauri/icons"

## Take screenshots of scenes from tools/screenshots/scenes.json into design/screens/.
screenshots:
	@python3 tools/screenshots.py

sign-check:
	@codesign -dv --verbose=2 "/Applications/Keepr.app" 2>&1 | grep -E "^(Identifier|Authority|Signature|TeamIdentifier)"

dev:
	npm run tauri dev
