// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Help menu's "Keepr Help" (⌘?), opening the window's help drawer (src/help).

use tauri::menu::{MenuItem, HELP_SUBMENU_ID};
use tauri::{AppHandle, Emitter};

pub const MENU_ID: &str = "help";
const TITLE: &str = "Keepr Help";

/// Adds the item to the Help menu of the app's default menu. Main thread, as setup is.
pub fn add_to_menu(app: &AppHandle) -> tauri::Result<()> {
    let Some(help) = app.menu().and_then(|m| m.get(HELP_SUBMENU_ID)).and_then(|i| i.as_submenu().cloned()) else { return Ok(()) };
    help.append(&MenuItem::with_id(app, MENU_ID, TITLE, true, None::<&str>)?)?;
    #[cfg(target_os = "macos")]
    set_shortcut();
    Ok(())
}

/// ⌘?, which the menu's accelerators can't spell (they'd give ⇧⌘/): set on the AppKit item itself.
#[cfg(target_os = "macos")]
fn set_shortcut() {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;
    let Some(cls) = AnyClass::get(c"NSApplication") else { return };
    unsafe {
        let nsapp: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
        let menu: *mut AnyObject = objc2::msg_send![nsapp, helpMenu];
        if menu.is_null() {
            return;
        }
        let title = NSString::from_str(TITLE);
        let item: *mut AnyObject = objc2::msg_send![menu, itemWithTitle: &*title];
        if item.is_null() {
            return;
        }
        let key = NSString::from_str("?");
        let _: () = objc2::msg_send![item, setKeyEquivalent: &*key];
        let _: () = objc2::msg_send![item, setKeyEquivalentModifierMask: 1usize << 20];
        // NSEventModifierFlagCommand
    }
}

/// The help drawer, in the window, brought forward first.
pub fn show_drawer(app: &AppHandle) {
    crate::show_main(app);
    let _ = app.emit_to("main", "help", ());
}
