// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keepr's website, and macOS's standard About panel opened by Keepr itself so the website under
//! the copyright can be clicked: Tauri's About item passes the credits as plain text, which the
//! panel shows but can't follow.

use tauri::menu::{MenuItemBuilder, MenuItemKind};
use tauri::AppHandle;

/// Keepr's website. The same as `WEBSITE` in src/Settings.tsx, whose Website button opens it (a
/// test below keeps them the same).
pub const WEBSITE: &str = "https://keepr.davies.co.za/";

/// The app menu's About item.
pub const ABOUT_ID: &str = "app:about";
/// Help › Keepr Website.
pub const WEBSITE_ID: &str = "help:website";

/// Puts Keepr's own About item first in the app menu, in place of the platform's, so it can open
/// the panel below. macOS only; elsewhere the platform's own About item stays.
pub fn set_about_item(app: &AppHandle) -> tauri::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() else { return Ok(()) };
    if app_menu.get(ABOUT_ID).is_some() {
        return Ok(());
    }
    app_menu.remove_at(0)?;
    app_menu.insert(&MenuItemBuilder::with_id(ABOUT_ID, "About Keepr").build(app)?, 0)?;
    Ok(())
}

/// Opens the website in the browser.
pub fn open_website(app: &AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    if let Err(e) = app.opener().open_url(WEBSITE, None::<&str>) {
        eprintln!("website: {e}");
    }
}

/// Opens the About panel: name, version, copyright, icon, and the website under them.
pub fn show(app: &AppHandle) {
    let version = app.package_info().version.to_string();
    let _ = app.run_on_main_thread(move || {
        let link = WEBSITE.trim_start_matches("https://").trim_end_matches('/');
        panel("Keepr", &version, "© 2026 Wayne Davies · GPL-3.0-or-later", include_bytes!("../icons/128x128@2x.png"), link, WEBSITE);
    });
}

/// Shows the panel: the name, version, copyright and icon (a PNG), with `link` under the copyright,
/// centred and clickable, opening `url`. Called on the main thread.
#[cfg(target_os = "macos")]
fn panel(name: &str, version: &str, copyright: &str, icon_png: &[u8], link: &str, url: &str) {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use objc2_foundation::{NSDictionary, NSString, NSURL};

    /// NSTextAlignmentCenter on macOS (Left 0, Right 1, Center 2).
    const CENTER: isize = 2;

    unsafe {
        let data: Retained<AnyObject> =
            msg_send![class!(NSData), dataWithBytes: icon_png.as_ptr().cast::<std::ffi::c_void>(), length: icon_png.len()];
        let image: Option<Retained<AnyObject>> = msg_send![msg_send![class!(NSImage), alloc], initWithData: &*data];

        let ns_url = NSURL::URLWithString(&NSString::from_str(url));
        let size: f64 = msg_send![class!(NSFont), smallSystemFontSize];
        let font: Retained<AnyObject> = msg_send![class!(NSFont), systemFontOfSize: size];
        let para: Retained<AnyObject> = msg_send![class!(NSMutableParagraphStyle), new];
        let _: () = msg_send![&*para, setAlignment: CENTER];
        let mut keys = vec![NSString::from_str("NSFont"), NSString::from_str("NSParagraphStyle")];
        let mut vals: Vec<&AnyObject> = vec![&font, &para];
        if let Some(u) = &ns_url {
            keys.push(NSString::from_str("NSLink"));
            vals.push(u.as_ref());
        }
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let attrs = NSDictionary::from_slices(&key_refs, &vals);
        let credits: Retained<AnyObject> =
            msg_send![msg_send![class!(NSAttributedString), alloc], initWithString: &*NSString::from_str(link), attributes: &*attrs];

        let name = NSString::from_str(name);
        let version = NSString::from_str(version);
        let copyright = NSString::from_str(copyright);
        let mut okeys = vec![
            NSString::from_str("ApplicationName"),
            NSString::from_str("ApplicationVersion"),
            NSString::from_str("Copyright"),
            NSString::from_str("Credits"),
        ];
        let mut ovals: Vec<&AnyObject> = vec![name.as_ref(), version.as_ref(), copyright.as_ref(), &credits];
        if let Some(img) = &image {
            okeys.push(NSString::from_str("ApplicationIcon"));
            ovals.push(img);
        }
        let okey_refs: Vec<&NSString> = okeys.iter().map(|k| &**k).collect();
        let options = NSDictionary::from_slices(&okey_refs, &ovals);
        let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![&*app, activateIgnoringOtherApps: true];
        let _: () = msg_send![&*app, orderFrontStandardAboutPanelWithOptions: &*options];
    }
}

#[cfg(not(target_os = "macos"))]
fn panel(_name: &str, _version: &str, _copyright: &str, _icon_png: &[u8], _link: &str, _url: &str) {}

#[cfg(test)]
mod tests {
    #[test]
    fn website_matches_the_settings_screen() {
        let settings = include_str!("../../src/Settings.tsx");
        assert!(settings.contains(&format!("export const WEBSITE = \"{}\";", super::WEBSITE)));
    }
}
