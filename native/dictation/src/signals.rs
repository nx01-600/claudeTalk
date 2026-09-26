//! Cross-process signals with the speaker (claudetalk.exe speaker), as named
//! manual-reset events instead of files polled every 30 ms:
//!   Local\claudetalk_speaking   set while a phrase plays
//!   Local\claudetalk_ducking    set while the user dictates: the speaker
//!                               ramps its volume down and reads slower
//! ducking.flag is still written, for a v0.6.0 speaker.

use ct_core::lock::wide;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, ResetEvent, SetEvent, WaitForSingleObject};

struct Ev(HANDLE);
unsafe impl Send for Ev {}
unsafe impl Sync for Ev {}

fn event(name: &str, cell: &'static OnceLock<Ev>) -> HANDLE {
    cell.get_or_init(|| Ev(unsafe { CreateEventW(std::ptr::null(), 1, 0, wide(name).as_ptr()) })).0
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
