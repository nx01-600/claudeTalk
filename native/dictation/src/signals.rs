//! Cross-process signals with the speaker (claudetalk.exe speaker), as named
//! manual-reset events instead of files polled every 30 ms:
//!   Local\claudetalk_speaking   set while a phrase plays
//!   Local\claudetalk_ducking    set while the user dictates: the speaker
//!                               ramps its volume down and reads slower
//! ducking.flag is still written, for a v0.6.0 speaker.

use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};

struct Ev(HANDLE);
unsafe impl Send for Ev {}
unsafe impl Sync for Ev {}

fn event(name: &str, cell: &'static OnceLock<Ev>) -> HANDLE {
    cell.get_or_init(|| Ev(unsafe { CreateEventW(std::ptr::null(), 1, 0, ct_core::lock::named(name).as_ptr()) })).0
}

fn speaking_ev() -> HANDLE {
    static E: OnceLock<Ev> = OnceLock::new();
    event(ct_core::queue::SPEAKING_EVENT, &E)
}

fn ducking_ev() -> HANDLE {
    static E: OnceLock<Ev> = OnceLock::new();
    event(ct_core::queue::DUCKING_EVENT, &E)
}

pub fn speaking() -> bool {
    let h = speaking_ev();
    !h.is_null() && unsafe { WaitForSingleObject(h, 0) } == WAIT_OBJECT_0
}

pub fn set_ducking(on: bool) {
    let h = ducking_ev();
    let flag = ct_core::paths::ducking_flag();
    unsafe {
        if on {
            SetEvent(h);
            let _ = std::fs::write(flag, b"");
        } else {
            ResetEvent(h);
            let _ = std::fs::remove_file(flag);
        }
    }
}

fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code) != 0;
        CloseHandle(h);
        ok && code == STILL_ACTIVE as u32
    }
}

/// Repairs what a speaker that died mid-phrase (killed, crashed) leaves
/// behind, so no answer is lost and the talking pill doesn't stay up:
/// - the "speaking" signal and the player files of a dead process go;
/// - phrases still queued with nobody draining them get a new speaker,
///   after `stranded_polls` polls in a row (queue::add starts one by
///   itself; this is only the safety net).
///   Returns the updated count of polls with a stranded queue.
pub fn heal_speaker(stranded_polls: u32) -> u32 {
    let pid_file = ct_core::paths::player_pid_file();
    if let Ok(text) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = text.trim().parse::<u32>() {
            if !process_alive(pid) {
                println!("[speaker] player {pid} died mid-phrase; clearing its signals");
                unsafe { ResetEvent(speaking_ev()) };
                let _ = std::fs::remove_file(&pid_file);
                let _ = std::fs::remove_file(ct_core::paths::player_session_file());
                if let Some(link) = ct_core::voice_link::get() {
                    link.set_level(0.0);
                    link.set_paused(false);
                }
            }
        }
    }
    if ct_core::queue::pending().is_empty() || speaking() {
        return 0;
    }
    let free = ct_core::lock::NamedMutex::new(ct_core::queue::SPEAKER_MUTEX).is_some_and(|m| m.acquire(0).is_some());
    if !free {
        return 0;
    }
    if stranded_polls + 1 < 10 {
        return stranded_polls + 1;
    }
    if let Some(exe) = crate::claudetalk_exe() {
        use std::os::windows::process::CommandExt;
        println!("[speaker] phrases were waiting with no speaker; starting one");
        // No inherited stdio: a detached child can't take console handles
        // (the daemon has some when started from a terminal).
        let r = std::process::Command::new(exe)
            .arg("speaker")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0800_0000 | 0x0000_0008)
            .spawn();
        if let Err(e) = r {
            println!("[speaker] could not start one: {e}");
        }
    }
    0
}
