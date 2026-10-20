// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Keepr Helper, the Windows service (`KeeprHelper.exe --helper-service`, running as SYSTEM): what
//! only an administrator may do for a backup, done on Keepr's behalf.
//!
//! - **A still copy**: a shadow copy of a drive (Volume Shadow Copy, through WMI's
//!   Win32_ShadowCopy), so a backup reads every file as it was at one moment, as macOS's APFS
//!   snapshot gives it (still.rs). Keepr reads the copy itself, as the user, so the files' own
//!   permissions still decide what it can read. A copy lives while the connection that asked for
//!   it is open, and is deleted when it closes, or when the service next starts after a crash.
//! - **The change journal**: the folders where anything changed on an NTFS drive since a point in
//!   its journal, as macOS's FSEvents gives them (fsevents.rs), for only the folders the caller
//!   names and can open itself.
//!
//! It answers on a named pipe (helper_win.rs PIPE), one JSON request and one reply per line, to the
//! users it was installed for (HKLM\SOFTWARE\Keepr Helper, Users) and administrators only, and to
//! no other computer.

use crate::helper_win::{PIPE, SERVICE, VERSION};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::FromRawHandle;
use std::path::PathBuf;
use std::sync::Mutex;
use windows::core::{BSTR, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, GENERIC_READ, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
use windows::Win32::Security::{RevertToSelf, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileIdType, GetFinalPathNameByHandleW, OpenFileById, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, VOLUME_NAME_DOS,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE,
    RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Ioctl::{FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0, USN_JOURNAL_DATA_V0};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR, VT_I4, VT_UI4};
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY,
    WBEM_FLAG_RETURN_WBEM_COMPLETE, WBEM_INFINITE,
};
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

windows_service::define_windows_service!(ffi_service_main, service_main);

/// `--helper-service`: hands the process to the service manager, which calls `service_main`.
pub fn run() -> i32 {
    match windows_service::service_dispatcher::start(SERVICE, ffi_service_main) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Keepr Helper runs as a Windows service, started by Windows: {e}");
            1
        }
    }
}

fn status(state: ServiceState) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: std::time::Duration::from_secs(5),
        process_id: None,
    }
}

fn service_main(_args: Vec<OsString>) {
    let handle = match service_control_handler::register(SERVICE, |ctrl| match ctrl {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            // Every copy goes with the service: none is left behind on the drive.
            forget_all();
            std::process::exit(0);
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    }) {
        Ok(h) => h,
        Err(_) => return,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _ = CoInitializeSecurity(None, -1, None, None, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE, None);
    }
    // Copies left by a helper that ended without deleting them (a crash, a power cut).
    for id in leftovers() {
        let _ = delete_shadow(&id);
    }
    save_leftovers(&[]);
    let _ = handle.set_service_status(status(ServiceState::Running));
    serve();
    let _ = handle.set_service_status(status(ServiceState::Stopped));
}

// ---- the pipe ----

/// Who may connect: SYSTEM, administrators, and the users the helper was installed for.
fn security_descriptor() -> Option<PSECURITY_DESCRIPTOR> {
    let users = crate::helper_win::allowed_users();
    let mut sddl = String::from("D:P(A;;GA;;;SY)(A;;GA;;;BA)");
    for sid in users.iter().filter(|s| s.starts_with("S-1-") && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')) {
        sddl.push_str(&format!("(A;;GRGW;;;{sid})"));
    }
    let mut sd = PSECURITY_DESCRIPTOR::default();
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&HSTRING::from(sddl), SDDL_REVISION_1, &mut sd, None) }.ok()?;
    Some(sd)
}

fn serve() {
    let mut first = true;
    loop {
        // Read afresh for each connection: installing for another user adds them while it runs.
        let Some(sd) = security_descriptor() else {
            std::thread::sleep(std::time::Duration::from_secs(5));
            continue;
        };
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };
        // The first instance must be this service's own, so no other program has taken the name.
        let mode = if first { PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE } else { PIPE_ACCESS_DUPLEX };
        let pipe = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(PIPE),
                mode,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                255,
                65536,
                65536,
                0,
                Some(&sa),
            )
        };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(sd.0)));
        }
        if pipe.is_invalid() {
            std::thread::sleep(std::time::Duration::from_secs(1));
            continue;
        }
        first = false;
        // A client that connected between the create and this call is connected already.
        let connected = unsafe { ConnectNamedPipe(pipe, None) };
        if let Err(e) = connected {
            if e.code() != windows::Win32::Foundation::ERROR_PIPE_CONNECTED.to_hresult() {
                unsafe {
                    let _ = CloseHandle(pipe);
                }
                continue;
            }
        }
        let raw = pipe.0 as usize;
        std::thread::spawn(move || client(raw));
    }
}

/// One connection: its requests, one per line, until it closes; then its copies are deleted.
fn client(raw: usize) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let pipe = HANDLE(raw as *mut std::ffi::c_void);
    let file = unsafe { std::fs::File::from_raw_handle(raw as *mut std::ffi::c_void) };
    let Ok(mut out) = file.try_clone() else { return };
    let mut mine: Vec<String> = Vec::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else { break };
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) => answer(pipe, &req, &mut mine).unwrap_or_else(|e| json!({ "error": e })),
            Err(e) => json!({ "error": format!("not a request: {e}") }),
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
    for id in mine {
        let _ = delete_shadow(&id);
        forget(&id);
    }
}

fn answer(pipe: HANDLE, req: &Value, mine: &mut Vec<String>) -> Result<Value, String> {
    let s = |k: &str| req[k].as_str().map(str::to_string).ok_or(format!("{k} is missing"));
    match req["op"].as_str().unwrap_or_default() {
        "hello" => Ok(json!({ "version": VERSION, "app": env!("CARGO_PKG_VERSION") })),
        "snapshot" => {
            let volume = drive_root(&s("volume")?)?;
            let (id, device) = create_shadow(&volume)?;
            remember(&id);
            mine.push(id.clone());
            Ok(json!({ "id": id, "device": device }))
        }
        "release" => {
            let id = s("id")?;
            if let Some(i) = mine.iter().position(|m| *m == id) {
                mine.remove(i);
                delete_shadow(&id)?;
                forget(&id);
            }
            Ok(json!({ "ok": true }))
        }
        "journal" => {
            let volume = drive_root(&s("volume")?)?;
            let d = query_journal(&volume)?;
            Ok(json!({ "journal": d.UsnJournalID.to_string(), "next": d.NextUsn }))
        }
        "changes" => {
            let volume = drive_root(&s("volume")?)?;
            let journal: u64 = s("journal")?.parse().map_err(|_| "bad journal")?;
            let since = req["since"].as_i64().ok_or("since is missing")?;
            let roots: Vec<String> =
                req["roots"].as_array().ok_or("roots are missing")?.iter().filter_map(|r| r.as_str().map(str::to_string)).collect();
            // Only folders the caller may open itself, so nobody learns of another's.
            for r in &roots {
                if !client_can_open(pipe, r) {
                    return Err(format!("{r} can't be opened by the user asking"));
                }
            }
            Ok(match changes(&volume, journal, since, &roots)? {
                Some(dirs) => json!({ "dirs": dirs }),
                None => json!({ "untrusted": true }),
            })
        }
        op => Err(format!("unknown request {op:?}")),
    }
}

/// "C:\" from "C:", "c:\" or "C:\…": a drive's root, letters only (no mounted-folder volumes).
fn drive_root(v: &str) -> Result<String, String> {
    let b = v.as_bytes();
    if b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        Ok(format!("{}:\\", v[..1].to_uppercase()))
    } else {
        Err(format!("{v} isn't a drive"))
    }
}

/// Whether the connected user can list the folder, judged as that user.
fn client_can_open(pipe: HANDLE, dir: &str) -> bool {
    unsafe {
        if ImpersonateNamedPipeClient(pipe).is_err() {
            return false;
        }
        let h = CreateFileW(
            &HSTRING::from(dir),
            0x0001,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        );
        let _ = RevertToSelf();
        match h {
            Ok(h) => {
                let _ = CloseHandle(h);
                true
            }
            Err(_) => false,
        }
    }
}

// ---- shadow copies, through WMI ----

fn wmi() -> Result<IWbemServices, String> {
    unsafe {
        let loc: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message())?;
        let svc = loc
            .ConnectServer(&BSTR::from("ROOT\\CIMV2"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)
            .map_err(|e| e.message())?;
        CoSetProxyBlanket(&svc, 10, 0, PCWSTR::null(), RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)
            .map_err(|e| e.message())?;
        Ok(svc)
    }
}

fn bstr_variant(s: &str) -> VARIANT {
    let mut v = VARIANT::default();
    unsafe {
        (*v.Anonymous.Anonymous).vt = VT_BSTR;
        (*v.Anonymous.Anonymous).Anonymous.bstrVal = std::mem::ManuallyDrop::new(BSTR::from(s));
    }
    v
}

/// A property of a WMI object, as text (a string) or a number.
fn get(obj: &IWbemClassObject, name: &str) -> Result<Value, String> {
    let mut v = VARIANT::default();
    unsafe {
        obj.Get(&HSTRING::from(name), 0, &mut v, None, None).map_err(|e| e.message())?;
        let inner = &v.Anonymous.Anonymous;
        let out = if inner.vt == VT_BSTR {
            json!(inner.Anonymous.bstrVal.to_string())
        } else if inner.vt == VT_I4 || inner.vt == VT_UI4 {
            json!(inner.Anonymous.lVal)
        } else {
            Value::Null
        };
        let _ = VariantClear(&mut v);
        Ok(out)
    }
}

/// A shadow copy of the drive at `volume` ("C:\"): its id and the device it's read through
/// (\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy7).
fn create_shadow(volume: &str) -> Result<(String, String), String> {
    let svc = wmi()?;
    unsafe {
        let mut class = None;
        svc.GetObject(&BSTR::from("Win32_ShadowCopy"), WBEM_FLAG_RETURN_WBEM_COMPLETE, None, Some(&mut class), None)
            .map_err(|e| e.message())?;
        let class = class.ok_or("no Win32_ShadowCopy class")?;
        let (mut insig, mut outsig) = (None, None);
        class.GetMethod(&HSTRING::from("Create"), 0, &mut insig, &mut outsig).map_err(|e| e.message())?;
        let input = insig.ok_or("no Create signature")?.SpawnInstance(0).map_err(|e| e.message())?;
        let (vol, ctx) = (bstr_variant(volume), bstr_variant("ClientAccessible"));
        input.Put(&HSTRING::from("Volume"), 0, &vol, 0).map_err(|e| e.message())?;
        input.Put(&HSTRING::from("Context"), 0, &ctx, 0).map_err(|e| e.message())?;
        for mut v in [vol, ctx] {
            let _ = VariantClear(&mut v);
        }
        let mut out = None;
        svc.ExecMethod(
            &BSTR::from("Win32_ShadowCopy"),
            &BSTR::from("Create"),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            &input,
            Some(&mut out),
            None,
        )
        .map_err(|e| e.message())?;
        let out = out.ok_or("no answer from Create")?;
        let rc = get(&out, "ReturnValue")?.as_i64().unwrap_or(-1);
        if rc != 0 {
            return Err(format!("Windows didn't make a shadow copy of {volume} (Win32_ShadowCopy.Create returned {rc})"));
        }
        let id = get(&out, "ShadowID")?.as_str().unwrap_or_default().to_string();
        if !(id.starts_with('{') && id.ends_with('}') && id.len() == 38) {
            return Err(format!("Windows gave an odd shadow copy id, {id:?}"));
        }
        let rows = svc
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(format!("SELECT DeviceObject FROM Win32_ShadowCopy WHERE ID='{id}'")),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .map_err(|e| e.message())?;
        let mut row = [None];
        let mut n = 0u32;
        let _ = rows.Next(WBEM_INFINITE, &mut row, &mut n);
        let device = match (n, &row[0]) {
            (1, Some(r)) => get(r, "DeviceObject")?.as_str().unwrap_or_default().to_string(),
            _ => String::new(),
        };
        if !device.starts_with(r"\\?\GLOBALROOT\Device\") {
            let _ = delete_shadow(&id);
            return Err("Windows made a shadow copy but didn't say where it is".into());
        }
        Ok((id, device))
    }
}

fn delete_shadow(id: &str) -> Result<(), String> {
    if !(id.starts_with('{') && id.ends_with('}') && id.len() == 38 && id[1..37].chars().all(|c| c.is_ascii_hexdigit() || c == '-')) {
        return Err(format!("{id:?} isn't a shadow copy id"));
    }
    let svc = wmi()?;
    unsafe {
        svc.ExecMethod(
            &BSTR::from(format!("Win32_ShadowCopy.ID=\"{id}\"")),
            &BSTR::from("Delete"),
            WBEM_FLAG_RETURN_WBEM_COMPLETE,
            None,
            None,
            None,
            None,
        )
        .map_err(|e| e.message())
    }
}

/// The copies made and not yet deleted, kept on disk so a helper that stopped without deleting
/// them deletes them when it starts.
static LIVE: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn leftovers_file() -> PathBuf {
    let base = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("Keepr Helper").join("shadows.txt")
}

fn leftovers() -> Vec<String> {
    std::fs::read_to_string(leftovers_file())
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn save_leftovers(ids: &[String]) {
    let f = leftovers_file();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, ids.join("\n"));
}

fn remember(id: &str) {
    let mut l = LIVE.lock().unwrap();
    l.push(id.to_string());
    save_leftovers(&l);
}

fn forget(id: &str) {
    let mut l = LIVE.lock().unwrap();
    l.retain(|x| x != id);
    save_leftovers(&l);
}

fn forget_all() {
    let ids = std::mem::take(&mut *LIVE.lock().unwrap());
    for id in &ids {
        let _ = delete_shadow(id);
    }
    save_leftovers(&[]);
}

// ---- the change journal ----

struct Volume(HANDLE);

impl Drop for Volume {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn open_volume(root: &str) -> Result<Volume, String> {
    let device = format!(r"\\.\{}", root.trim_end_matches('\\'));
    let h = unsafe {
        CreateFileW(
            &HSTRING::from(device),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("{root} can't be opened: {}", e.message()))?;
    Ok(Volume(h))
}

fn query_journal(root: &str) -> Result<USN_JOURNAL_DATA_V0, String> {
    let v = open_volume(root)?;
    query(&v).map_err(|e| format!("{root} has no change journal: {e}"))
}

fn query(v: &Volume) -> Result<USN_JOURNAL_DATA_V0, String> {
    let mut d = USN_JOURNAL_DATA_V0::default();
    let mut got = 0u32;
    unsafe {
        windows::Win32::System::IO::DeviceIoControl(
            v.0,
            FSCTL_QUERY_USN_JOURNAL,
            None,
            0,
            Some(&mut d as *mut _ as *mut std::ffi::c_void),
            std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
            Some(&mut got),
            None,
        )
    }
    .map_err(|e| e.message())?;
    Ok(d)
}

/// More changed folders than this, and the backup looks at everything instead: resolving each
/// would take longer than the walk it saves.
const MOST_FOLDERS: usize = 50_000;

/// The folders under `roots` where anything changed since `since` in journal `journal`, as
/// Windows names them (C:\Users\w\Documents\Taxes); None when the journal can't say (a new
/// journal, or one that has since dropped the records asked for).
fn changes(root: &str, journal: u64, since: i64, roots: &[String]) -> Result<Option<Vec<String>>, String> {
    let v = open_volume(root)?;
    let d = query(&v)?;
    if d.UsnJournalID != journal || since < d.FirstUsn || since < d.LowestValidUsn || since > d.NextUsn {
        return Ok(None);
    }
    let end = d.NextUsn;
    let mut parents: HashSet<u64> = HashSet::new();
    let mut start = since;
    let mut buf = vec![0u8; 1 << 16];
    while start < end {
        let req = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: start,
            ReasonMask: 0xFFFF_FFFF,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: journal,
        };
        let mut got = 0u32;
        let read = unsafe {
            windows::Win32::System::IO::DeviceIoControl(
                v.0,
                FSCTL_READ_USN_JOURNAL,
                Some(&req as *const _ as *const std::ffi::c_void),
                std::mem::size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                Some(buf.as_mut_ptr().cast()),
                buf.len() as u32,
                Some(&mut got),
                None,
            )
        };
        if read.is_err() {
            return Ok(None);
        }
        let got = got as usize;
        if got < 8 {
            break;
        }
        let next = i64::from_le_bytes(buf[..8].try_into().unwrap());
        let mut at = 8;
        let mut done = false;
        while at + 60 <= got {
            let len = u32::from_le_bytes(buf[at..at + 4].try_into().unwrap()) as usize;
            if len == 0 || at + len > got {
                break;
            }
            let major = u16::from_le_bytes(buf[at + 4..at + 6].try_into().unwrap());
            // USN_RECORD_V2: the parent's file reference at 16, the record's USN at 24.
            if major == 2 {
                let parent = u64::from_le_bytes(buf[at + 16..at + 24].try_into().unwrap());
                let usn = i64::from_le_bytes(buf[at + 24..at + 32].try_into().unwrap());
                if usn >= end {
                    done = true;
                    break;
                }
                parents.insert(parent);
            } else {
                // A record of a form this doesn't read: say so rather than miss a change.
                return Ok(None);
            }
            at += len;
        }
        if done || next <= start {
            break;
        }
        start = next;
        if parents.len() > MOST_FOLDERS {
            return Ok(None);
        }
    }
    let wanted: Vec<String> = roots.iter().map(|r| r.trim_end_matches('\\').to_lowercase()).collect();
    let under = |p: &str| {
        let low = p.to_lowercase();
        wanted.iter().any(|r| low == *r || low.starts_with(&format!("{r}\\")))
    };
    let mut names: HashMap<u64, Option<String>> = HashMap::new();
    let mut dirs = Vec::new();
    for frn in parents {
        let path = names.entry(frn).or_insert_with(|| path_of(&v, frn)).clone();
        // A folder deleted since: its own parent's record covers it.
        if let Some(p) = path.filter(|p| under(p)) {
            dirs.push(p);
        }
    }
    dirs.sort();
    Ok(Some(dirs))
}

/// A file or folder's path from its file reference: C:\Users\w\Documents.
fn path_of(v: &Volume, frn: u64) -> Option<String> {
    let id = FILE_ID_DESCRIPTOR {
        dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
        Type: FileIdType,
        Anonymous: FILE_ID_DESCRIPTOR_0 { FileId: frn as i64 },
    };
    unsafe {
        let h =
            OpenFileById(v.0, &id, 0x80, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, None, FILE_FLAG_BACKUP_SEMANTICS).ok()?;
        let mut buf = vec![0u16; 1024];
        let mut n = GetFinalPathNameByHandleW(h, &mut buf, VOLUME_NAME_DOS) as usize;
        if n > buf.len() {
            buf = vec![0u16; n + 1];
            n = GetFinalPathNameByHandleW(h, &mut buf, VOLUME_NAME_DOS) as usize;
        }
        let _ = CloseHandle(h);
        if n == 0 || n > buf.len() {
            return None;
        }
        let s = String::from_utf16_lossy(&buf[..n]);
        Some(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
    }
}
