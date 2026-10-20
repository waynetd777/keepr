// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! The form paths take in Keepr: in snapshots, plans, the app and its page.
//!
//! Every path is written as macOS writes it, with `/` between the parts, on Windows too: there `/`
//! is the system drive (C:\), so C:\Users\wayne is /Users/wayne, another drive keeps its letter
//! (D:/Photos) and a share is //server/share. A snapshot's sources, the restore screen's folders
//! and a plan's leave-out rules are then the same on both, and a backup made on a Mac reads the
//! same on a PC. `native` turns an app path into what Windows opens, and `app` turns what the
//! system gave back into an app path; on macOS both leave a path as it is.

use std::path::{Path, PathBuf};

/// The system drive's letter and colon ("C:"), from %SystemDrive%.
#[cfg(windows)]
pub fn system_drive() -> &'static str {
    static DRIVE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DRIVE.get_or_init(|| {
        std::env::var("SystemDrive")
            .ok()
            .filter(|d| d.len() == 2 && d.ends_with(':'))
            .map(|d| d.to_uppercase())
            .unwrap_or_else(|| "C:".into())
    })
}

/// What the file system opens for an app path: on Windows "/" is the system drive's root and
/// every `/` is a `\`; elsewhere the path as it is.
pub fn native(path: &str) -> PathBuf {
    #[cfg(windows)]
    {
        // A share (//server/share) isn't on the system drive.
        let p = if path.starts_with('/') && !path.starts_with("//") { format!("{}{path}", system_drive()) } else { path.to_string() };
        // A drive on its own ("D:") is the folder the process is in on that drive; its root is meant.
        let p = if p.len() == 2 && p.ends_with(':') { format!("{p}/") } else { p };
        PathBuf::from(p.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    PathBuf::from(path)
}

/// The app's form of a path the system gave (a folder picked, the home folder, a file found in a
/// walk): on Windows `/` between the parts and the system drive's root as "/"; elsewhere the path
/// as it is.
pub fn app(path: &Path) -> String {
    let s = path.to_string_lossy();
    #[cfg(windows)]
    {
        // \\?\C:\… is C:\…, and \\?\UNC\server\share is \\server\share.
        let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{rest}")
        } else {
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        };
        let s = s.replace('\\', "/");
        let drive = system_drive();
        let on_system =
            s.len() >= 2 && s.is_char_boundary(2) && s[..2].eq_ignore_ascii_case(drive) && (s.len() == 2 || s[2..].starts_with('/'));
        let s = if on_system {
            s[2..].to_string()
        } else if s.len() >= 2 && s.as_bytes()[1] == b':' {
            format!("{}{}", s[..1].to_uppercase(), &s[1..])
        } else {
            s
        };
        let t = s.trim_end_matches('/');
        if t.is_empty() {
            "/".into()
        } else if t.len() == 2 && t.ends_with(':') {
            format!("{t}/")
        } else {
            t.to_string()
        }
    }
    #[cfg(not(windows))]
    s.into_owned()
}

/// `name` inside the folder `dir`, both in the app's form.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// The home folder in the app's form: $HOME, or on Windows %USERPROFILE%.
pub fn home() -> String {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(|h| app(Path::new(&h))).unwrap_or_default()
    }
    #[cfg(not(windows))]
    std::env::var("HOME").unwrap_or_default()
}

/// Whether an app path is absolute: "/…", or on Windows a drive's ("D:/…").
pub fn is_absolute(path: &str) -> bool {
    path.starts_with('/') || (cfg!(windows) && path.len() >= 3 && path.as_bytes()[1] == b':' && path.as_bytes()[2] == b'/')
}

/// The folder holding an app path ("/Users/w" for "/Users/w/Documents"; "D:/" for "D:/Photos"),
/// None at a root.
pub fn parent(path: &str) -> Option<String> {
    let t = path.trim_end_matches('/');
    let i = t.rfind('/')?;
    Some(if i == 0 {
        "/".into()
    } else if i == 2 && t.as_bytes()[1] == b':' {
        t[..3].to_string()
    } else {
        t[..i].to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_paths() {
        assert_eq!(join("/", "Users"), "/Users");
        assert_eq!(join("/Users", "w"), "/Users/w");
        assert_eq!(parent("/Users/w/Documents").as_deref(), Some("/Users/w"));
        assert_eq!(parent("/Users").as_deref(), Some("/"));
        assert_eq!(parent("D:/Photos").as_deref(), Some("D:/"));
        assert_eq!(parent("Photos"), None);
        assert!(is_absolute("/Users/w"));
        assert!(!is_absolute("s3://bucket"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_both_ways() {
        let d = system_drive();
        assert_eq!(app(&PathBuf::from(format!(r"{d}\Users\w\Documents"))), "/Users/w/Documents");
        assert_eq!(app(&PathBuf::from(format!(r"\\?\{d}\Users\w"))), "/Users/w");
        assert_eq!(app(&PathBuf::from(format!(r"{d}\"))), "/");
        assert_eq!(app(Path::new(r"d:\Photos\")), "D:/Photos");
        assert_eq!(app(Path::new(r"Z:\")), "Z:/");
        assert_eq!(app(Path::new(r"\\nas\Backups\Keepr")), "//nas/Backups/Keepr");
        assert_eq!(app(Path::new(r"\\?\UNC\nas\Backups")), "//nas/Backups");
        assert_eq!(native("/Users/w"), PathBuf::from(format!(r"{d}\Users\w")));
        assert_eq!(native("Z:"), PathBuf::from(r"Z:\"));
        assert_eq!(native("D:/Photos"), PathBuf::from(r"D:\Photos"));
        assert_eq!(native("//nas/Backups"), PathBuf::from(r"\\nas\Backups"));
        assert!(is_absolute("D:/Photos"));
    }
}
