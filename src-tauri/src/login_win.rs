// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Open at Login on Windows, as login_item.rs is on macOS: the app in the user's Run key
//! (HKCU\Software\Microsoft\Windows\CurrentVersion\Run), started with --login so it stays in the
//! notification area (`started_at_login`), as a login launch stays in the menu bar on macOS. The
//! uninstaller takes the entry away (windows/hooks.nsh).
//!
//! Windows' Settings › Apps › Startup (and Task Manager's Startup apps) can switch a Run entry
//! off without removing it: it keeps that in Explorer's StartupApproved\Run, a binary value per
//! name whose first byte is odd when it is off. That is macOS's "approve it in System Settings":
//! only the user can switch it back on, there.

use windows::core::HSTRING;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "Keepr";
/// What a launch from the Run key is started with.
pub const ARG: &str = "--login";

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Status {
    Enabled,
    NotRegistered,
    /// In the Run key, but switched off in Windows' Startup apps.
    RequiresApproval,
}

impl Status {
    pub fn is_on(self) -> bool {
        self == Status::Enabled
    }
}

/// The command the Run value holds, or None when there is none.
fn get() -> Option<String> {
    let mut buf = [0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(RUN),
            &HSTRING::from(NAME),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    read.is_ok().then(|| String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)]))
}

/// Switched off in Windows' Startup apps.
fn switched_off() -> bool {
    let mut buf = [0u8; 12];
    let mut size = buf.len() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(APPROVED),
            &HSTRING::from(NAME),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    read.is_ok() && size > 0 && buf[0] & 1 == 1
}

/// This copy of the app, quoted, with --login.
fn command() -> Option<String> {
    Some(format!("\"{}\" {ARG}", std::env::current_exe().ok()?.display()))
}

pub fn available() -> bool {
    command().is_some()
}

/// Enabled when the Run key starts this copy of the app and Startup apps hasn't switched it off.
pub fn status() -> Status {
    match get() {
        Some(held) if command().is_some_and(|c| c.eq_ignore_ascii_case(&held)) => {
            if switched_off() {
                Status::RequiresApproval
            } else {
                Status::Enabled
            }
        }
        _ => Status::NotRegistered,
    }
}

pub fn set(on: bool) -> Status {
    let (run, name) = (HSTRING::from(RUN), HSTRING::from(NAME));
    unsafe {
        match command().filter(|_| on) {
            Some(cmd) => {
                let wide: Vec<u16> = cmd.encode_utf16().chain([0]).collect();
                let _ = RegSetKeyValueW(HKEY_CURRENT_USER, &run, &name, REG_SZ.0, Some(wide.as_ptr().cast()), (wide.len() * 2) as u32);
            }
            None => {
                let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, &run, &name);
            }
        }
    }
    status()
}

/// Windows' Startup apps, where an entry switched off there is switched back on.
pub fn open_settings() {
    let _ = tauri_plugin_opener::open_url("ms-settings:startupapps", None::<&str>);
}

/// Whether the Run key started this copy, rather than someone opening it.
pub fn started_at_login() -> bool {
    std::env::args().skip(1).any(|a| a == ARG)
}
