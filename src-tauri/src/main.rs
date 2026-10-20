// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

// No console window beside the app on Windows, in a debug build too. The command line attaches to
// the terminal it was started from instead (cli::attach_console).
#![cfg_attr(windows, windows_subsystem = "windows")]

/// The app menu labels an unbundled process by its executable name in `tauri dev`; set the real
/// name before AppKit starts. The bundled .app takes its name from Info.plist anyway.
#[cfg(target_os = "macos")]
fn set_process_name() {
    use objc2_foundation::{NSProcessInfo, NSString};
    NSProcessInfo::processInfo().setProcessName(&NSString::from_str("Keepr"));
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = keepr_lib::cli(&args) {
        std::process::exit(code);
    }
    #[cfg(target_os = "macos")]
    set_process_name();
    keepr_lib::run()
}
