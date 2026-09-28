//! The Help menu's "Keepr Help" (⌘?), opening the Help Book tools/helpbook.py builds from docs/.
//! The app's Info.plist names the book, so macOS also searches it from the Help menu's search
//! field. A dev build has no bundle to hold the book, so there it opens the built pages in the browser.

use tauri::menu::{MenuItem, HELP_SUBMENU_ID};
use tauri::{AppHandle, Manager};

pub const MENU_ID: &str = "help";
const BOOK: &str = "Keepr.help";
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
        let _: () = objc2::msg_send![item, setKeyEquivalentModifierMask: 1usize << 20]; // NSEventModifierFlagCommand
    }
}

/// The book in the app's Resources, when running from a bundle.
fn bundled(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().resource_dir().ok().map(|d| d.join(BOOK)).filter(|p| p.exists())
}

pub fn show(app: &AppHandle) {
    if bundled(app).is_some() {
        #[cfg(target_os = "macos")]
        unsafe {
            use objc2::runtime::{AnyClass, AnyObject};
            let Some(cls) = AnyClass::get(c"NSApplication") else { return };
            let nsapp: *mut AnyObject = objc2::msg_send![cls, sharedApplication];
            let _: () = objc2::msg_send![nsapp, showHelp: std::ptr::null::<AnyObject>()];
        }
        return;
    }
    // Only a dev build looks in the source tree (and only it has the path compiled in).
    #[cfg(debug_assertions)]
    {
        use tauri_plugin_opener::OpenerExt;
        let page = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gen/help").join(BOOK).join("Contents/Resources/en.lproj/index.html");
        if page.exists() {
            let _ = app.opener().open_path(page.to_string_lossy(), None::<&str>);
        } else {
            eprintln!("no help built: run python3 tools/helpbook.py");
        }
    }
}
