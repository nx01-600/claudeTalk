//! Named Win32 mutexes shared with the other claudeTalk processes.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A named mutex. `acquire` returns a guard only if it got ownership; an
/// abandoned mutex (its owner died) counts as owned.
pub struct NamedMutex(HANDLE);

unsafe impl Send for NamedMutex {}

impl NamedMutex {
    pub fn new(name: &str) -> Option<Self> {
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, wide(name).as_ptr()) };
        if h.is_null() {
            None
        } else {
            Some(Self(h))
        }
    }

    pub fn acquire(&self, timeout_ms: u32) -> Option<MutexGuard<'_>> {
        let r = unsafe { WaitForSingleObject(self.0, timeout_ms) };
        if r == WAIT_OBJECT_0 || r == WAIT_ABANDONED {
            Some(MutexGuard(self))
        } else {
            None
        }
    }
}

impl Drop for NamedMutex {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub struct MutexGuard<'a>(&'a NamedMutex);

impl Drop for MutexGuard<'_> {
    fn drop(&mut self) {
        unsafe { ReleaseMutex((self.0).0) };
    }
}

/// Runs `f` holding `Local\claudetalk_sessions`. Like v0.5 it waits up to 3 s
/// and runs anyway if the lock never comes: a stuck process must not freeze
/// the hooks.
pub fn with_session_lock<T>(f: impl FnOnce() -> T) -> T {
    let m = NamedMutex::new("Local\\claudetalk_sessions");
    let _g = m.as_ref().and_then(|m| m.acquire(3000));
    f()
}
