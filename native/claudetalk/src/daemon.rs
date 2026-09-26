//! Starting the dictation daemon (voice-daemon-ensure.ps1 in v0.5): the
//! native bin\claudetalk-dictation.exe, next to this one.

use ct_core::lock::wide;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::OpenMutexW;

const SYNCHRONIZE: u32 = 0x0010_0000;

/// Plugin root: this exe lives in <root>\bin.
pub fn plugin_root() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent()?.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

/// The daemon holds this mutex for its whole life.
pub fn running() -> bool {
    let h = unsafe { OpenMutexW(SYNCHRONIZE, 0, wide("Local\\claudeTalk-dictation").as_ptr()) };
    if h.is_null() {
        return false;
    }
    unsafe { CloseHandle(h) };
    true
}

fn dictation_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.parent()?.join("claudetalk-dictation.exe");
    exe.exists().then_some(exe)
}

/// Starts dictation in --auto mode unless it runs already. Returns what
/// happened, for /dictation.
pub fn ensure() -> &'static str {
    if running() {
        return "running";
    }
    let Some(exe) = dictation_exe() else { return "not-installed" };
    use std::os::windows::process::CommandExt;
    const DETACHED: u32 = 0x0000_0008;
    const BREAKAWAY: u32 = 0x0100_0000;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("--auto")
        .current_dir(plugin_root())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Outlive the hook even if Claude Code puts hooks in a job.
    let spawned = cmd.creation_flags(DETACHED | BREAKAWAY).spawn().or_else(|_| cmd.creation_flags(DETACHED).spawn());
    if spawned.is_ok() {
        "started"
    } else {
        "failed"
    }
}
