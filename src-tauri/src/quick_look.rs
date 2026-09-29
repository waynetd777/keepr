// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Quick Look on a file, in the system's preview panel.
//!
//! `qlmanage -p` would do it in one line, but it is Apple's debugging tool and marks its window
//! "[DEBUG]". The real panel wants a controller in the key window's responder chain: one object,
//! put in after the window, that hands the panel the file to show.

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{NSResponder, NSWindow};
use objc2_foundation::{NSInteger, NSString, NSURL};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource};
use std::cell::RefCell;

pub struct Ivars {
    url: RefCell<Option<Retained<NSURL>>>,
}

define_class!(
    #[unsafe(super(NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Previewer;

    unsafe impl NSObjectProtocol for Previewer {}

    unsafe impl QLPreviewPanelDataSource for Previewer {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            self.ivars().url.borrow().is_some() as NSInteger
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(&self, _panel: Option<&QLPreviewPanel>, _index: NSInteger) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            self.ivars().url.borrow().clone().map(ProtocolObject::from_retained)
        }
    }

    // QLPreviewPanelController, an informal protocol.
    impl Previewer {
        #[unsafe(method(acceptsPreviewPanelControl:))]
        fn accepts(&self, _panel: Option<&QLPreviewPanel>) -> bool {
            true
        }

        #[unsafe(method(beginPreviewPanelControl:))]
        fn begin(&self, panel: Option<&QLPreviewPanel>) {
            if let Some(p) = panel {
                unsafe { p.setDataSource(Some(ProtocolObject::from_ref(self))) };
            }
        }

        #[unsafe(method(endPreviewPanelControl:))]
        fn end(&self, panel: Option<&QLPreviewPanel>) {
            if let Some(p) = panel {
                unsafe { p.setDataSource(None) };
            }
        }
    }
);

thread_local! {
    // Kept for the life of the app: the panel doesn't retain its data source.
    static PREVIEWER: RefCell<Option<Retained<Previewer>>> = const { RefCell::new(None) };
}

/// Shows `path` in the Quick Look panel over `window`. Main thread only.
pub fn show(mtm: MainThreadMarker, window: &NSWindow, path: &std::path::Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let me = PREVIEWER.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(|| {
                let this = Previewer::alloc(mtm).set_ivars(Ivars { url: RefCell::new(None) });
                unsafe { msg_send![super(this), init] }
            })
            .clone()
    });
    *me.ivars().url.borrow_mut() = Some(url);
    // Into the chain just after the window, once.
    let next = unsafe { window.nextResponder() };
    if next.as_deref().map(|n| std::ptr::eq(n, &**me as &NSResponder)) != Some(true) {
        unsafe {
            me.setNextResponder(next.as_deref());
            window.setNextResponder(Some(&me));
        }
    }
    unsafe {
        let panel = QLPreviewPanel::sharedPreviewPanel(mtm).expect("Quick Look panel");
        panel.updateController();
        if panel.currentController().is_none() {
            panel.setDataSource(Some(ProtocolObject::from_ref(&*me)));
        }
        panel.reloadData();
        panel.setCurrentPreviewItemIndex(0);
        panel.makeKeyAndOrderFront(None);
    }
}
