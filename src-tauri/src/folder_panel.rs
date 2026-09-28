//! Choosing folders, with one macOS open panel kept for the life of the app.
//!
//! On this Mac (macOS 27), making a second NSOpenPanel in a process takes about 26 seconds: the
//! panel waits on its out-of-process view service, and the first panel's service isn't let go.
//! Reusing the first panel is instant. The dialog plugin makes a new panel every time, so every
//! Choose after the first stalled; this keeps one and reuses it.

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
use objc2_foundation::{NSString, NSURL};
use std::cell::RefCell;

thread_local! {
    static PANEL: RefCell<Option<Retained<NSOpenPanel>>> = const { RefCell::new(None) };
}

/// Shows the panel and returns the folders chosen (none if cancelled). Main thread only.
pub fn choose(mtm: MainThreadMarker, title: &str, multiple: bool, start: Option<&str>) -> Vec<String> {
    PANEL.with(|cell| {
        let mut cell = cell.borrow_mut();
        let panel = cell.get_or_insert_with(|| NSOpenPanel::openPanel(mtm));
        panel.setCanChooseDirectories(true);
        panel.setCanChooseFiles(false);
        panel.setCanCreateDirectories(true);
        panel.setAllowsMultipleSelection(multiple);
        panel.setMessage(Some(&NSString::from_str(title)));
        panel.setPrompt(Some(&NSString::from_str("Choose")));
        if let Some(s) = start.filter(|s| std::path::Path::new(s).is_dir()) {
            panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(&NSString::from_str(s))));
        }
        if panel.runModal() != NSModalResponseOK {
            return vec![];
        }
        panel.URLs().iter().filter_map(|u| u.path()).map(|p| p.to_string()).collect()
    })
}
