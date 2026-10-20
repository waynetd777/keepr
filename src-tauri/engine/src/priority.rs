// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Backups give way to whatever the computer is doing: their threads run below normal priority,
//! and a backup reads and processes files on half the cores, so the rest stay free.
//!
//! On macOS the threads are "utility" quality of service (below the default; macOS throttles their
//! disk use too when something else needs the disk), on Windows THREAD_PRIORITY_BELOW_NORMAL. A
//! backup on an idle computer runs about as fast as before: most time goes on the disk or the
//! network, not the processor.

/// Lowers the calling thread's priority, for good.
pub fn background_thread() {
    #[cfg(target_os = "macos")]
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0);
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

/// How many threads a backup reads files with: half the cores, at least one.
pub fn readers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).div_ceil(2).max(1)
}

/// The pool backups read files on: `readers` threads, each below normal priority.
pub fn pool() -> &'static rayon::ThreadPool {
    static POOL: std::sync::OnceLock<rayon::ThreadPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(readers())
            .thread_name(|i| format!("keepr-read-{i}"))
            .start_handler(|_| background_thread())
            .build()
            .expect("a thread pool")
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_on_half_the_cores() {
        let n = std::thread::available_parallelism().map_or(2, |n| n.get());
        assert_eq!(super::readers(), n.div_ceil(2));
        assert_eq!(super::pool().current_num_threads(), super::readers());
    }
}
