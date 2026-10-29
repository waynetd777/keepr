// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! One copy of the app per user session, decided before anything else starts (Windows).
//!
//! tauri-plugin-single-instance decides inside the Tauri builder: it creates a named mutex, and a
//! copy that finds the mutex already there looks for the first copy's hidden message window, hands
//! its arguments over through it (WM_COPYDATA) and exits. The window is made only after the mutex,
//! so a copy started in that gap finds the mutex taken and no window, and the plugin lets it run as
//! a full instance over the same data. A burst of launches (a double-click that lands twice, the
//! Start menu clicked while Defender scans the first start) can leave two copies.
//!
//! So the decision is made here, first thing in `run`, on the ownership of a second named mutex
//! (`<identifier>-launch`): the first copy takes it and keeps it until its process ends; any other
//! copy waits for the plugin's window in the first copy, hands its arguments over through the
//! plugin's own channel (so the plugin's callback in lib.rs runs as it would) and exits without
//! touching the data folder. A copy never runs without owning the lock: one that gets no answer
//! within a minute says so and exits. The plugin stays, as the channel. The way round is Brainstead's.

use std::time::{Duration, Instant};

use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, MessageBoxW, SendMessageTimeoutW, MB_ICONINFORMATION, MB_OK,
    SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_COPYDATA,
};

/// The plugin's payload kind for a working directory and arguments separated by NULs, with no
/// terminator: `WMCOPYDATA_SINGLE_INSTANCE_DATA_NUL` in 2.5.4 (Cargo.lock), the kind its own second
/// copies send. Its older kind, 1542, joins them by `|` and is kept only for older senders.
const PAYLOAD: usize = 1543;
/// How long a second copy keeps trying to hand over: for the first copy's window to appear, and
/// then for its main thread to take the message. The first copy may still be in the builder
/// behind Defender's scan of its start, and pumps messages only once its event loop runs.
const HANDOVER_WAIT: Duration = Duration::from_secs(60);
/// How long one send waits for the first copy to take the arguments. The send is retried until
/// `HANDOVER_WAIT` is up, so this only bounds how often the loop looks again.
const SEND_TIMEOUT_MS: u32 = 2_000;

/// Returns in the first copy, which then owns the lock until its process ends. In any other copy
/// it hands this process's working directory and arguments to the first copy and exits; when that
/// takes more than `HANDOVER_WAIT` it tells the user and exits without them. When the lock can't
/// be made at all the plugin decides later, as it did before.
pub fn claim(identifier: &str) {
    // The plugin's class and window names (`-sic`, `-siw`), without its `semver` feature.
    let class_name = HSTRING::from(format!("{identifier}-sic"));
    let window_name = HSTRING::from(format!("{identifier}-siw"));
    // SAFETY: the name outlives the call; no attributes.
    let Ok(mutex) = (unsafe { CreateMutexW(None, false, &HSTRING::from(format!("{identifier}-launch"))) }) else { return };
    let deadline = Instant::now() + HANDOVER_WAIT;
    loop {
        // Ownership, not existence: a copy that has exited (or crashed) has released it, however
        // many copies still hold a handle, so a launch right after a quit takes over cleanly.
        // SAFETY: `mutex` is a handle this process owns.
        match unsafe { WaitForSingleObject(mutex, 0) } {
            // The handle is kept open, and the ownership with it, for the life of the process.
            WAIT_OBJECT_0 | WAIT_ABANDONED => return,
            WAIT_TIMEOUT => {}
            // The wait itself failed on a handle this process made: the lock is unusable, and the
            // plugin's own check is all that's left.
            _ => return,
        }
        // SAFETY: as above.
        if let Ok(hwnd) = unsafe { FindWindowW(&class_name, &window_name) } {
            if forward(hwnd) {
                // SAFETY: closing a handle this process owns and doesn't use again.
                let _ = unsafe { CloseHandle(mutex) };
                std::process::exit(0);
            }
        }
        if Instant::now() >= deadline {
            // SAFETY: as above.
            let _ = unsafe { CloseHandle(mutex) };
            no_answer();
            std::process::exit(1);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Hands this process's working directory and arguments to the first copy's plugin window, which
/// runs the plugin's callback with them, and says whether it took them. It hasn't while its main
/// thread is still in the builder: the send then times out, or is cut short once the thread counts
/// as hung (`SMTO_ABORTIFHUNG`), and the caller tries again. Lets the first copy bring its window
/// forward first: Windows grants that to the process the user started, and it can pass the right on.
fn forward(hwnd: HWND) -> bool {
    let mut pid = 0;
    // SAFETY: `hwnd` is a window found a moment ago; a stale one makes the calls fail, not crash.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != 0 {
        // SAFETY: a process id.
        let _ = unsafe { AllowSetForegroundWindow(pid) };
    }
    let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    let data = payload(&cwd, std::env::args_os().map(|a| a.to_string_lossy().into_owned()));
    let cds = COPYDATASTRUCT { dwData: PAYLOAD, cbData: data.len() as u32, lpData: data.as_ptr() as *mut _ };
    // SAFETY: `cds` and `data` outlive the synchronous send; the receiver copies the bytes.
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&cds as *const _ as isize),
            SMTO_NORMAL | SMTO_ABORTIFHUNG,
            SEND_TIMEOUT_MS,
            None,
        )
    };
    sent.0 != 0
}

/// The first copy never took the hand-over. Says so, except to a start at sign-in (`--login`,
/// login_win.rs), which nobody is watching.
fn no_answer() {
    if std::env::args().skip(1).any(|a| a == crate::login_item::ARG) {
        return;
    }
    let text = HSTRING::from("Keepr is already starting but hasn't answered in a minute. Try again once it's up.");
    // SAFETY: the strings outlive the call; no owner window.
    let _ = unsafe { MessageBoxW(None, &text, &HSTRING::from("Keepr"), MB_OK | MB_ICONINFORMATION) };
}

/// The plugin's wire format: the working directory, then each argument (the program first), with a
/// NUL before each and none at the end. Neither can contain a NUL on Windows.
fn payload(cwd: &str, args: impl Iterator<Item = String>) -> Vec<u8> {
    let mut data = cwd.to_owned();
    for arg in args {
        data.push('\0');
        data.push_str(&arg);
    }
    data.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::payload;

    #[test]
    fn payload_is_cwd_then_args_nul_separated() {
        let args = ["C:\\app\\Keepr.exe".to_owned(), "--login".to_owned()];
        assert_eq!(payload("C:\\work", args.into_iter()), b"C:\\work\0C:\\app\\Keepr.exe\0--login");
        assert_eq!(payload("C:\\work", std::iter::empty()), b"C:\\work");
    }
}
