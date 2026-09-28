//! What the Mac is doing that decides whether a backup should wait: the battery, and whether the
//! network is one macOS calls expensive (a personal hotspot).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// On battery power, its charge in percent; None on mains power or a Mac without one.
pub fn battery_percent() -> Option<u32> {
    let out = std::process::Command::new("/usr/bin/pmset").args(["-g", "batt"]).output().ok()?;
    parse_pmset(&String::from_utf8_lossy(&out.stdout))
}

fn parse_pmset(s: &str) -> Option<u32> {
    if !s.contains("'Battery Power'") {
        return None;
    }
    let pct = s.split('%').next()?.rsplit(|c: char| !c.is_ascii_digit()).next()?;
    pct.parse().ok()
}

static EXPENSIVE: AtomicBool = AtomicBool::new(false);

#[link(name = "Network", kind = "framework")]
extern "C" {
    fn nw_path_monitor_create() -> *mut c_void;
    fn nw_path_monitor_set_queue(monitor: *mut c_void, queue: *mut c_void);
    fn nw_path_monitor_set_update_handler(monitor: *mut c_void, handler: &block2::Block<dyn Fn(*mut c_void)>);
    fn nw_path_monitor_start(monitor: *mut c_void);
    fn nw_path_is_expensive(path: *mut c_void) -> bool;
}

extern "C" {
    fn dispatch_queue_create(label: *const std::ffi::c_char, attr: *mut c_void) -> *mut c_void;
}

/// Starts watching the network. macOS marks a path expensive for a personal hotspot (and
/// cellular); the answer is kept up to date in the background.
pub fn watch_network() {
    unsafe {
        let m = nw_path_monitor_create();
        if m.is_null() {
            return;
        }
        let q = dispatch_queue_create(c"keepr.network".as_ptr(), std::ptr::null_mut());
        nw_path_monitor_set_queue(m, q);
        let block = block2::RcBlock::new(|path: *mut c_void| {
            EXPENSIVE.store(!path.is_null() && nw_path_is_expensive(path), Relaxed);
        });
        nw_path_monitor_set_update_handler(m, &block);
        nw_path_monitor_start(m);
        // The monitor lives as long as the app.
        std::mem::forget(block);
    }
}

pub fn on_expensive_network() -> bool {
    EXPENSIVE.load(Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pmset() {
        assert_eq!(parse_pmset("Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged; 0:00 remaining present: true"), None);
        assert_eq!(parse_pmset("Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t18%; discharging; 1:02 remaining present: true"), Some(18));
    }
}
