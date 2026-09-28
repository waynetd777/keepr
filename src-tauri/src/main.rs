// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
