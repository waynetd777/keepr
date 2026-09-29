// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What changed since the last backup, from macOS's own record of file-system events
//! (FSEvents). Replaying the record since the event id noted at the last backup gives the folders
//! that changed; every other folder is taken from the last snapshot without being looked at,
//! which is what keeps an hourly backup of a million files quick.
//!
//! The record isn't trusted when macOS says it dropped events, its ids wrapped or the root
//! changed; then (and for anything not on the startup disk) the backup looks at everything. The
//! weekly full backup re-reads everything regardless.

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use std::ffi::{c_void, CStr};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[repr(C)]
struct Context {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

type Callback = extern "C" fn(stream: *mut c_void, info: *mut c_void, n: usize, paths: *mut c_void, flags: *const u32, ids: *const u64);

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn FSEventStreamCreate(
        alloc: *const c_void,
        cb: Callback,
        ctx: *const Context,
        paths: *const c_void,
        since: u64,
        latency: f64,
        flags: u32,
    ) -> *mut c_void;
    fn FSEventStreamSetDispatchQueue(stream: *mut c_void, queue: *mut c_void);
    fn FSEventStreamStart(stream: *mut c_void) -> bool;
    fn FSEventStreamStop(stream: *mut c_void);
    fn FSEventStreamInvalidate(stream: *mut c_void);
    fn FSEventStreamRelease(stream: *mut c_void);
    fn FSEventsGetCurrentEventId() -> u64;
}

extern "C" {
    fn dispatch_queue_create(label: *const std::ffi::c_char, attr: *mut c_void) -> *mut c_void;
}

const NO_DEFER: u32 = 0x02;
const MUST_SCAN_SUBDIRS: u32 = 0x01;
const USER_DROPPED: u32 = 0x02;
const KERNEL_DROPPED: u32 = 0x04;
const IDS_WRAPPED: u32 = 0x08;
const HISTORY_DONE: u32 = 0x10;
const ROOT_CHANGED: u32 = 0x20;

#[derive(Default)]
struct Collected {
    paths: Vec<PathBuf>,
    rescan: Vec<PathBuf>,
    untrusted: bool,
    done: bool,
}

struct Shared {
    c: Mutex<Collected>,
    cv: Condvar,
}

fn clean(p: &str) -> PathBuf {
    let p = p.trim_end_matches('/');
    let p = p.strip_prefix(crate::still::DATA).unwrap_or(p);
    PathBuf::from(if p.is_empty() { "/" } else { p })
}

extern "C" fn callback(_s: *mut c_void, info: *mut c_void, n: usize, paths: *mut c_void, flags: *const u32, _ids: *const u64) {
    let shared = unsafe { &*(info as *const Shared) };
    let paths = paths as *const *const std::ffi::c_char;
    let mut c = shared.c.lock().unwrap();
    for i in 0..n {
        let f = unsafe { *flags.add(i) };
        if f & HISTORY_DONE != 0 {
            c.done = true;
            continue;
        }
        let p = clean(&unsafe { CStr::from_ptr(*paths.add(i)) }.to_string_lossy());
        if f & (USER_DROPPED | KERNEL_DROPPED | IDS_WRAPPED | ROOT_CHANGED) != 0 {
            c.untrusted = true;
        } else if f & MUST_SCAN_SUBDIRS != 0 {
            c.rescan.push(p);
        } else {
            c.paths.push(p);
        }
    }
    if c.done {
        shared.cv.notify_all();
    }
}

pub fn now_id() -> u64 {
    unsafe { FSEventsGetCurrentEventId() }
}

/// The folders under `roots` that changed since event `since`, or None if that can't be trusted.
pub fn since(roots: &[PathBuf], since: u64) -> Option<keepr_engine::backup::Changes> {
    if since == 0 || since > now_id() || roots.is_empty() {
        return None;
    }
    let shared = Box::new(Shared { c: Mutex::new(Collected::default()), cv: Condvar::new() });
    let ctx = Context {
        version: 0,
        info: &*shared as *const Shared as *mut c_void,
        retain: std::ptr::null(),
        release: std::ptr::null(),
        copy_description: std::ptr::null(),
    };
    let arr = CFArray::from_CFTypes(&roots.iter().map(|r| CFString::new(&r.to_string_lossy())).collect::<Vec<_>>());
    let result = unsafe {
        let stream =
            FSEventStreamCreate(std::ptr::null(), callback, &ctx, arr.as_concrete_TypeRef() as *const c_void, since, 0.0, NO_DEFER);
        if stream.is_null() {
            return None;
        }
        let q = dispatch_queue_create(c"keepr.fsevents".as_ptr(), std::ptr::null_mut());
        FSEventStreamSetDispatchQueue(stream, q);
        if !FSEventStreamStart(stream) {
            FSEventStreamInvalidate(stream);
            FSEventStreamRelease(stream);
            return None;
        }
        let guard = shared.c.lock().unwrap();
        let (guard, timeout) = shared.cv.wait_timeout_while(guard, Duration::from_secs(60), |c| !c.done).unwrap();
        let ok = !timeout.timed_out() && !guard.untrusted;
        let out = ok.then(|| (guard.paths.clone(), guard.rescan.clone()));
        drop(guard);
        FSEventStreamStop(stream);
        FSEventStreamInvalidate(stream);
        FSEventStreamRelease(stream);
        out
    };
    let (paths, rescan) = result?;
    Some(keepr_engine::backup::Changes::from_events(paths, rescan))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_a_change_in_the_record() {
        let t = tempfile::tempdir().unwrap();
        let dir = std::fs::canonicalize(t.path()).unwrap();
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::create_dir_all(dir.join("b")).unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let start = now_id();
        std::fs::write(dir.join("b/new.txt"), b"x").unwrap();
        std::thread::sleep(Duration::from_millis(1500));
        let Some(ch) = since(std::slice::from_ref(&dir), start) else { return }; // no record on this volume
        assert!(!ch.unchanged(&dir.join("b")), "{:?}", ch);
        assert!(ch.unchanged(&dir.join("a")));
        assert_eq!(clean("/System/Volumes/Data/Users/w/"), PathBuf::from("/Users/w"));
    }
}
