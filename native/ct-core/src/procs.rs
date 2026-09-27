//! Process lookups with Toolhelp snapshots instead of WMI: v0.5 paid about a
//! second of `Get-CimInstance` per hook for this.

use std::collections::HashMap;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

#[derive(Clone, Debug)]
pub struct Proc {
    pub parent: u32,
    /// Executable file name, lowercase (e.g. "claude.exe").
    pub exe: String,
}

/// Every process running right now, by PID.
pub struct Snapshot(pub HashMap<u32, Proc>);

impl Snapshot {
    pub fn take() -> Self {
        let mut map = HashMap::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return Self(map);
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e);
            while ok != 0 {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let exe = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
                map.insert(e.th32ProcessID, Proc { parent: e.th32ParentProcessID, exe });
                ok = Process32NextW(snap, &mut e);
            }
            CloseHandle(snap);
        }
        Self(map)
    }

    pub fn is_claude(&self, pid: u32) -> bool {
        self.0.get(&pid).is_some_and(|p| p.exe == "claude.exe")
    }

    /// First claude.exe above `pid`, at most 12 steps up (as in v0.5).
    pub fn claude_ancestor(&self, pid: u32) -> Option<u32> {
        let mut current = pid;
        for _ in 0..12 {
            let parent = self.0.get(&current)?.parent;
            if parent == 0 || parent == current {
                return None;
            }
            if self.is_claude(parent) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }
}

/// claude.exe this process runs under (hooks, the MCP server and the Bash
/// tool all descend from it).
pub fn claude_pid() -> Option<u32> {
    if let Some(pid) = std::env::var("CLAUDETALK_CLAUDE_PID").ok().and_then(|v| v.parse().ok()) {
        return Some(pid);
    }
    Snapshot::take().claude_ancestor(std::process::id())
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        handle: windows_sys::Win32::Foundation::HANDLE,
        class: u32,
        info: *mut core::ffi::c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> i32;
}

/// Command line of another process (ProcessCommandLineInformation, Win 8.1+).
pub fn command_line(pid: u32) -> Option<String> {
    const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u64; 4096]; // 32 KiB, 8-byte aligned
        let mut ret = 0u32;
        let status = NtQueryInformationProcess(
            h,
            PROCESS_COMMAND_LINE_INFORMATION,
            buf.as_mut_ptr().cast(),
            (buf.len() * 8) as u32,
            &mut ret,
        );
        CloseHandle(h);
        if status < 0 {
            return None;
        }
        let us = &*(buf.as_ptr() as *const UnicodeString);
        if us.buffer.is_null() {
            return Some(String::new());
        }
        let s = std::slice::from_raw_parts(us.buffer, us.length as usize / 2);
        Some(String::from_utf16_lossy(s))
    }
}

/// `claude -p` / `--print` / `--output-format ...`: a headless subprocess, not
/// a window anyone dictates into or listens to.
pub fn is_headless_cmdline(cmd: &str) -> bool {
    cmd.contains("--output-format")
        || cmd.contains("--print")
        || cmd.split_whitespace().any(|w| w == "-p")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless() {
        assert!(is_headless_cmdline("claude -p hola"));
        assert!(is_headless_cmdline("claude.exe --output-format stream-json"));
        assert!(is_headless_cmdline("claude --print x"));
        assert!(!is_headless_cmdline("claude --resume"));
        assert!(!is_headless_cmdline("claude -pa"));
    }

    #[test]
    fn own_command_line() {
        let me = command_line(std::process::id()).unwrap();
        assert!(!me.is_empty());
    }

    #[test]
    fn snapshot_has_self() {
        let s = Snapshot::take();
        assert!(s.0.contains_key(&std::process::id()));
    }
}

/// Stops the children this process spawns from inheriting its stdin, stdout
/// and stderr. Rust spawns with `bInheritHandles = TRUE`, so a detached
/// daemon started from a hook would keep Claude Code's pipe open, and Claude
/// Code waits for EOF on it until the hook times out. Only inheritance
/// changes: this process still reads and writes its own streams.
pub fn keep_std_handles() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
    for h in [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ] {
        if !h.is_null() {
            unsafe { SetHandleInformation(h as _, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}
