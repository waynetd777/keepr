// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keepr Helper on Windows, as Keepr sees it: asking it for a still copy or the change journal
//! (helper_service_win.rs is the service), and installing or removing it.
//!
//! The helper is a copy of Keepr.exe in Program Files (KeeprHelper.exe), where only an
//! administrator can change it, run as a service by SYSTEM. Installing it needs an administrator
//! once: Settings asks Windows to run `Keepr.exe --helper-install <user>` elevated, with the usual
//! prompt. Without it Keepr backs up as before, without a still copy and comparing every file.

use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::core::{HSTRING, PCWSTR};

/// The service's name, and what Services shows.
pub const SERVICE: &str = "KeeprHelper";
const DISPLAY: &str = "Keepr Helper";
pub const PIPE: &str = r"\\.\pipe\keepr-helper";
/// What the helper and the app agree on: a helper of another version is offered for updating.
pub const VERSION: u32 = 1;
/// The users the helper answers, by SID, in HKLM (only an administrator can change it).
pub const REG_KEY: &str = r"SOFTWARE\Keepr Helper";

/// Where the helper is installed: %ProgramFiles%\Keepr Helper\KeeprHelper.exe.
fn install_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramW6432")
        .or_else(|| std::env::var_os("ProgramFiles"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    pf.join("Keepr Helper")
}

fn installed_exe() -> PathBuf {
    install_dir().join("KeeprHelper.exe")
}

/// The SIDs the helper answers, from the registry.
pub fn allowed_users() -> Vec<String> {
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let mut buf = [0u16; 4096];
    let mut size = (buf.len() * 2) as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(REG_KEY),
            &HSTRING::from("Users"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if read.is_err() {
        return vec![];
    }
    String::from_utf16_lossy(&buf[..(size as usize / 2).saturating_sub(1)])
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

// ---- talking to it ----

/// A connection to the helper. A still copy made on it lasts as long as it does.
pub struct Conn {
    out: std::fs::File,
    input: BufReader<std::fs::File>,
}

impl Conn {
    pub fn open() -> Result<Conn, String> {
        use std::os::windows::io::AsRawHandle;
        let f = std::fs::OpenOptions::new().read(true).write(true).open(PIPE).map_err(|_| "Keepr Helper isn't running.".to_string())?;
        // Only the service, in session 0, may answer: not a program that took the pipe's name.
        let mut session = u32::MAX;
        let h = windows::Win32::Foundation::HANDLE(f.as_raw_handle());
        let checked = unsafe { windows::Win32::System::Pipes::GetNamedPipeServerSessionId(h, &mut session) };
        if checked.is_err() || session != 0 {
            return Err("Something other than Keepr Helper answered on its pipe.".into());
        }
        let input = BufReader::new(f.try_clone().map_err(|e| e.to_string())?);
        Ok(Conn { out: f, input })
    }

    pub fn call(&mut self, req: Value) -> Result<Value, String> {
        writeln!(self.out, "{req}").and_then(|_| self.out.flush()).map_err(|e| format!("Keepr Helper stopped answering: {e}"))?;
        let mut line = String::new();
        self.input.read_line(&mut line).map_err(|e| format!("Keepr Helper stopped answering: {e}"))?;
        let v: Value = serde_json::from_str(&line).map_err(|_| "Keepr Helper's answer couldn't be read.".to_string())?;
        match v["error"].as_str() {
            Some(e) => Err(e.to_string()),
            None => Ok(v),
        }
    }
}

/// One request on a connection of its own.
pub fn call(req: Value) -> Result<Value, String> {
    Conn::open()?.call(req)
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Answering on its pipe.
    pub running: bool,
    /// Its service is installed (running or not).
    pub installed: bool,
    /// The version it answers to, and the app's own version it was copied from.
    pub version: u32,
    pub app: String,
    /// Older than this app expects: install it again to update it.
    pub outdated: bool,
}

pub fn status() -> Status {
    let installed = installed_exe().is_file();
    match call(json!({ "op": "hello" })) {
        Ok(v) => {
            let version = v["version"].as_u64().unwrap_or(0) as u32;
            Status {
                running: true,
                installed: true,
                version,
                app: v["app"].as_str().unwrap_or_default().to_string(),
                outdated: version != VERSION,
            }
        }
        Err(_) => Status { installed, ..Default::default() },
    }
}

/// Whether backups can use the helper now: running, and of this app's version. Asked before every
/// backup and for every source, so the answer is kept for half a minute.
static SEEN: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

pub fn available() -> bool {
    let mut s = SEEN.lock().unwrap();
    if let Some((at, ok)) = *s {
        if at.elapsed() < Duration::from_secs(30) {
            return ok;
        }
    }
    let st = status();
    let ok = st.running && !st.outdated;
    *s = Some((Instant::now(), ok));
    ok
}

/// The next `available` asks again.
fn forget_availability() {
    *SEEN.lock().unwrap() = None;
}

// ---- installing ----

/// Runs this program elevated (Windows asks first) with `args`, and waits for it. Ok(exit code),
/// or why it didn't run (the prompt was declined).
fn run_elevated(exe: &std::path::Path, args: &str) -> Result<u32, String> {
    use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let (verb, file, params) = (HSTRING::from("runas"), HSTRING::from(exe.as_os_str()), HSTRING::from(args));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: 0,
        ..Default::default()
    };
    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| {
            if e.code() == windows::Win32::Foundation::ERROR_CANCELLED.to_hresult() {
                "Windows didn't get an administrator's go-ahead, so nothing changed.".to_string()
            } else {
                e.message()
            }
        })?;
        if info.hProcess.is_invalid() {
            return Ok(0);
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 0u32;
        let _ = GetExitCodeProcess(info.hProcess, &mut code);
        let _ = windows::Win32::Foundation::CloseHandle(info.hProcess);
        Ok(code)
    }
}

/// Settings' Install: installs the helper (or updates it) for this user, asking Windows for an
/// administrator first.
pub fn install() -> Result<Status, String> {
    let sid = crate::places_win::user_sid().ok_or("Couldn't tell which Windows account this is.")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let code = run_elevated(&exe, &format!("--helper-install {sid}"))?;
    forget_availability();
    if code != 0 {
        return Err(format!("Keepr Helper couldn't be installed (error {code}). {}", read_note()));
    }
    // Ready once it answers.
    for _ in 0..50 {
        let s = status();
        if s.running {
            return Ok(s);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(status())
}

/// Settings' Remove: stops and removes the helper.
pub fn uninstall() -> Result<Status, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let code = run_elevated(&exe, "--helper-uninstall")?;
    forget_availability();
    if code != 0 {
        return Err(format!("Keepr Helper couldn't be removed (error {code}). {}", read_note()));
    }
    Ok(status())
}

/// What the elevated step last wrote about a failure, for the message.
fn note_file() -> PathBuf {
    std::env::temp_dir().join("keepr-helper-setup.txt")
}

fn read_note() -> String {
    std::fs::read_to_string(note_file()).unwrap_or_default().trim().to_string()
}

/// `Keepr.exe --helper-…`: what the elevated step and the service run. Some(exit code) when the
/// arguments were for the helper.
pub fn main(args: &[String]) -> Option<i32> {
    let r = match args.first().map(String::as_str) {
        Some("--helper-service") => return Some(crate::helper_service_win::run()),
        Some("--helper-install") => elevated_install(args.get(1).map(String::as_str).unwrap_or_default()),
        Some("--helper-uninstall") => elevated_uninstall(),
        _ => return None,
    };
    Some(match r {
        Ok(()) => 0,
        Err(e) => {
            // The elevated process has no window; the app reads this to say what went wrong.
            let _ = std::fs::write(note_file(), &e);
            eprintln!("{e}");
            1
        }
    })
}

fn elevated_install(sid: &str) -> Result<(), String> {
    use windows_service::service::{ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType};
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    if !(sid.starts_with("S-1-") && sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        return Err(format!("{sid:?} isn't a Windows account's SID"));
    }
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)
        .map_err(|e| e.to_string())?;
    // An earlier helper is stopped first, so its program can be replaced.
    if let Ok(old) = manager.open_service(SERVICE, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE) {
        let _ = old.stop();
        wait_stopped(&old);
        old.delete().map_err(|e| format!("The old Keepr Helper couldn't be removed: {e}"))?;
        drop(old);
        std::thread::sleep(Duration::from_millis(500));
    }
    std::fs::create_dir_all(install_dir()).map_err(|e| e.to_string())?;
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    keepr_engine::backend::retry(|| std::fs::copy(&me, installed_exe()))
        .map_err(|e| format!("Couldn't copy Keepr Helper into Program Files: {e}"))?;
    add_user(sid)?;
    let info = ServiceInfo {
        name: SERVICE.into(),
        display_name: DISPLAY.into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: installed_exe(),
        launch_arguments: vec!["--helper-service".into()],
        dependencies: vec![],
        account_name: None,
        account_password: None,
    };
    let svc = manager
        .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(|e| format!("The Keepr Helper service couldn't be made: {e}"))?;
    let _ = svc.set_description("Makes a still copy of a drive (Volume Shadow Copy) and reads its change journal for Keepr's backups.");
    svc.start::<&str>(&[]).map_err(|e| format!("Keepr Helper didn't start: {e}"))?;
    Ok(())
}

fn wait_stopped(svc: &windows_service::service::Service) {
    for _ in 0..50 {
        match svc.query_status() {
            Ok(s) if s.current_state != windows_service::service::ServiceState::Stopped => std::thread::sleep(Duration::from_millis(200)),
            _ => return,
        }
    }
}

/// Adds a user to those the helper answers.
fn add_user(sid: &str) -> Result<(), String> {
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_LOCAL_MACHINE, REG_SZ};
    let mut users = allowed_users();
    if !users.iter().any(|u| u.eq_ignore_ascii_case(sid)) {
        users.push(sid.to_string());
    }
    let wide: Vec<u16> = users.join(";").encode_utf16().chain([0]).collect();
    unsafe {
        RegSetKeyValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(REG_KEY),
            &HSTRING::from("Users"),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        )
    }
    .ok()
    .map_err(|e| format!("Couldn't note who may use Keepr Helper: {}", e.message()))
}

fn elevated_uninstall() -> Result<(), String> {
    use windows::Win32::System::Registry::{RegDeleteTreeW, HKEY_LOCAL_MACHINE};
    use windows_service::service::ServiceAccess;
    use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(|e| e.to_string())?;
    if let Ok(svc) = manager.open_service(SERVICE, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE) {
        let _ = svc.stop();
        wait_stopped(&svc);
        svc.delete().map_err(|e| format!("The Keepr Helper service couldn't be removed: {e}"))?;
    }
    unsafe {
        let _ = RegDeleteTreeW(HKEY_LOCAL_MACHINE, &HSTRING::from(REG_KEY));
    }
    let dir = install_dir();
    let me = std::env::current_exe().ok();
    if me.as_ref().is_some_and(|m| m.starts_with(&dir)) {
        // Run from the helper's own copy (the uninstaller's hook): it can't delete itself while
        // running, so a moment after it ends.
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &format!("ping -n 3 127.0.0.1 >NUL & rmdir /S /Q \"{}\"", dir.display())]);
        let _ = crate::process::no_window(&mut c).spawn();
    } else {
        let _ = keepr_engine::backend::retry(|| {
            std::fs::remove_dir_all(&dir).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) })
        });
    }
    let data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join("Keepr Helper");
    let _ = std::fs::remove_dir_all(data);
    Ok(())
}
