// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Starting other programs.

use std::process::Command;

/// On Windows, runs a console program without opening a console window for it (the app has none
/// of its own to share, main.rs); elsewhere, nothing.
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

/// A command run as the user would run it in their shell: zsh as a login shell on macOS (so ~ and
/// their PATH work), PowerShell on Windows. Its output, both streams, is the caller's to read.
pub fn shell(script: &str) -> Command {
    #[cfg(windows)]
    {
        let mut c = Command::new("powershell.exe");
        // In a script block so that 2>&1 takes every line's errors, and `exit 3` ends PowerShell with 3.
        c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &format!("& {{ {script} }} 2>&1")]);
        no_window(&mut c);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("/bin/zsh");
        c.args(["-lc", &format!("{script} 2>&1")]);
        c
    }
}
