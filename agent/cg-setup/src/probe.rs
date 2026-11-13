//! PROBE ONLY (Session 70, never lands): what a window of cg-setup sees when
//! Burn starts it. With `--ui=<level>` among its arguments it records, to
//! `C:\cg-probe\probe-<pid>.log`: its session, the console's, its window
//! station's flags and desktop, its parent, the foreground window's owner, a
//! top-level window of its own after `ShowWindow` and `SetForegroundWindow`,
//! and, while the level is 4, the same readings each second for the seconds
//! in `C:\cg-probe\hold.txt` (default 150), the parent's life included. The
//! argument is then removed and the verb runs as it would.
#![allow(clippy::all, unsafe_code, dead_code)]

use std::ffi::c_void;
use std::io::Write;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTSGetActiveConsoleSessionId,
};
use windows_sys::Win32::System::StationsAndDesktops::{
    GetProcessWindowStation, GetThreadDesktop, GetUserObjectInformationW, UOI_FLAGS, UOI_NAME,
    USEROBJECTFLAGS,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenProcess, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, DispatchMessageW, FlashWindowEx, GetForegroundWindow,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, PeekMessageW,
    SetForegroundWindow, ShowWindow, SystemParametersInfoW, TranslateMessage, FLASHWINFO,
    FLASHW_ALL, FLASHW_TIMERNOFG, MSG, PM_REMOVE, SPI_GETFOREGROUNDLOCKTIMEOUT, SW_SHOWNORMAL,
    WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct Log(std::fs::File, Instant);

impl Log {
    fn line(&mut self, text: &str) {
        let _ = writeln!(self.0, "{:>7.2}s {}", self.1.elapsed().as_secs_f64(), text);
        let _ = self.0.flush();
    }
}

fn processes() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() || snap as isize == -1 {
            return out;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut entry);
        while ok != 0 {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            out.push((
                entry.th32ProcessID,
                entry.th32ParentProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..len]),
            ));
            ok = Process32NextW(snap, &mut entry);
        }
        CloseHandle(snap);
    }
    out
}

fn name_of(pid: u32, all: &[(u32, u32, String)]) -> String {
    all.iter()
        .find(|p| p.0 == pid)
        .map(|p| p.2.clone())
        .unwrap_or_else(|| "?".into())
}

fn object_name(handle: HANDLE) -> String {
    let mut buf = [0u16; 256];
    let mut needed = 0u32;
    let ok = unsafe {
        GetUserObjectInformationW(
            handle,
            UOI_NAME,
            buf.as_mut_ptr() as *mut c_void,
            (buf.len() * 2) as u32,
            &mut needed,
        )
    };
    if ok == 0 {
        return format!("error {}", std::io::Error::last_os_error());
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(0);
    String::from_utf16_lossy(&buf[..len])
}

fn window_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn foreground(all: &[(u32, u32, String)]) -> (HWND, String) {
    let fg = unsafe { GetForegroundWindow() };
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(fg, &mut pid) };
    (
        fg,
        format!(
            "foreground hwnd={:?} pid={} exe={} title={:?}",
            fg,
            pid,
            name_of(pid, all),
            window_text(fg)
        ),
    )
}

fn pump() {
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Removes `--ui=<level>` from `arguments` and, if it was there, records.
pub fn run(arguments: &mut Vec<std::ffi::OsString>) {
    let Some(index) = arguments
        .iter()
        .position(|a| a.to_str().map_or(false, |s| s.starts_with("--ui=")))
    else {
        return;
    };
    let raw = arguments.remove(index);
    let level = raw
        .to_str()
        .unwrap_or("")
        .trim_start_matches("--ui=")
        .trim_matches('"')
        .to_owned();
    let _ = std::fs::create_dir_all(r"C:\cg-probe");
    let me = unsafe { GetCurrentProcessId() };
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!(r"C:\cg-probe\probe-{me}.log"))
    else {
        return;
    };
    let mut log = Log(file, Instant::now());
    let hold: u64 = std::fs::read_to_string(r"C:\cg-probe\hold.txt")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(150);
    log.line(&format!(
        "argv ui={raw:?} level={level:?} rest={arguments:?} hold={hold}"
    ));
    let all = processes();
    let parent = all.iter().find(|p| p.0 == me).map(|p| p.1).unwrap_or(0);
    let grand = all.iter().find(|p| p.0 == parent).map(|p| p.1).unwrap_or(0);
    log.line(&format!(
        "pid={me} parent={parent} ({}) grandparent={grand} ({})",
        name_of(parent, &all),
        name_of(grand, &all)
    ));
    unsafe {
        let mut session = 0u32;
        ProcessIdToSessionId(me, &mut session);
        log.line(&format!(
            "session={session} active_console_session={}",
            WTSGetActiveConsoleSessionId()
        ));
        let station = GetProcessWindowStation();
        let mut flags: USEROBJECTFLAGS = std::mem::zeroed();
        let mut needed = 0u32;
        let ok = GetUserObjectInformationW(
            station as HANDLE,
            UOI_FLAGS,
            &mut flags as *mut _ as *mut c_void,
            std::mem::size_of::<USEROBJECTFLAGS>() as u32,
            &mut needed,
        );
        log.line(&format!(
            "winsta={} flags_ok={ok} dwFlags={:#x} (WSF_VISIBLE=1)",
            object_name(station as HANDLE),
            flags.dwFlags
        ));
        let desktop = GetThreadDesktop(GetCurrentThreadId());
        log.line(&format!("desktop={}", object_name(desktop as HANDLE)));
        let mut timeout = 0u32;
        SystemParametersInfoW(
            SPI_GETFOREGROUNDLOCKTIMEOUT,
            0,
            &mut timeout as *mut _ as *mut c_void,
            0,
        );
        log.line(&format!("foreground_lock_timeout_ms={timeout}"));
    }
    log.line(&foreground(&all).1);
    if level != "4" {
        log.line("level is not 4: no window, no hold");
        return;
    }
    let parent_handle = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            parent,
        )
    };
    log.line(&format!("parent handle null={}", parent_handle.is_null()));
    let class = wide("STATIC");
    let title = wide("cg-setup probe window");
    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200,
            200,
            420,
            240,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    log.line(&format!(
        "created hwnd={hwnd:?} err={}",
        std::io::Error::last_os_error()
    ));
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let was = ShowWindow(hwnd, SW_SHOWNORMAL);
        pump();
        log.line(&format!(
            "ShowWindow prev_visible={was} visible={} iconic={}",
            IsWindowVisible(hwnd),
            IsIconic(hwnd)
        ));
        let all = processes();
        let (fg, text) = foreground(&all);
        log.line(&format!(
            "after show: ours_is_foreground={} {text}",
            fg == hwnd
        ));
        let set = SetForegroundWindow(hwnd);
        pump();
        let (fg, text) = foreground(&all);
        log.line(&format!(
            "SetForegroundWindow={set} ours_is_foreground={} {text}",
            fg == hwnd
        ));
        if fg != hwnd {
            let info = FLASHWINFO {
                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                hwnd,
                dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG,
                uCount: 0,
                dwTimeout: 0,
            };
            let flashed = FlashWindowEx(&info);
            log.line(&format!("FlashWindowEx={flashed}"));
        }
    }
    let start = Instant::now();
    let mut last = Instant::now() - Duration::from_secs(10);
    while start.elapsed() < Duration::from_secs(hold) {
        pump();
        if last.elapsed() >= Duration::from_secs(1) {
            last = Instant::now();
            let all = processes();
            let (fg, text) = foreground(&all);
            let alive = if parent_handle.is_null() {
                "unknown".to_string()
            } else {
                (unsafe { WaitForSingleObject(parent_handle, 0) } != 0).to_string()
            };
            let rollback: Vec<String> = all
                .iter()
                .filter(|p| p.2.eq_ignore_ascii_case("cg-setup.exe") && p.0 != me)
                .map(|p| format!("{}<-{}", p.0, p.1))
                .collect();
            log.line(&format!(
                "tick parent_alive={alive} visible={} ours_is_foreground={} other_cg_setup={rollback:?} {text}",
                unsafe { IsWindowVisible(hwnd) },
                fg == hwnd
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    unsafe { DestroyWindow(hwnd) };
    log.line("hold over, the verb runs");
}
