//! Starting the dictation daemon (voice-daemon-ensure.ps1 in v0.5). The
//! daemon is downloaded on first use (see fetch.rs).

use ct_core::lock::wide;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::OpenMutexW;

const SYNCHRONIZE: u32 = 0x0010_0000;
const DETACHED: u32 = 0x0000_0008;
const NO_WINDOW: u32 = 0x0800_0000;
const BREAKAWAY: u32 = 0x0100_0000;

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

fn spawn(exe: &Path, args: &[&str], flags: u32) -> bool {
    use std::os::windows::process::CommandExt;
    ct_core::procs::keep_std_handles();
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .current_dir(plugin_root())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Outlive the hook even if Claude Code puts hooks in a job.
    cmd.creation_flags(flags | BREAKAWAY).spawn().or_else(|_| cmd.creation_flags(flags).spawn()).is_ok()
}

/// Starts dictation in --auto mode unless it runs already. If the daemon
/// isn't downloaded yet, a detached `claudetalk fetch-dictation` gets it and
/// then starts it, so the hook returns at once. Returns what happened.
pub fn ensure() -> &'static str {
    crate::fetch::remember_self();
    if running() {
        return "running";
    }
    match crate::fetch::dictation_exe() {
        Some(exe) => {
            if spawn(&exe, &["--auto"], DETACHED) {
                "started"
            } else {
                "failed"
            }
        }
        None => {
            let me = std::env::current_exe().unwrap_or_default();
            if spawn(&me, &["fetch-dictation", "--start"], DETACHED | NO_WINDOW) {
                "downloading"
            } else {
                "failed"
            }
        }
    }
}

/// `claudetalk fetch-dictation [--start]`: downloads the daemon for this
/// version (and starts it). Prints the path.
pub fn fetch_command(start: bool) -> i32 {
    let exe = match crate::fetch::dictation_exe() {
        Some(e) => e,
        None => match crate::fetch::download() {
            Ok(e) => e,
            Err(e) => {
                ct_core::log::log(&format!("fetch-dictation: {e}"));
                eprintln!("claudeTalk: could not download the dictation app: {e}");
                return 1;
            }
        },
    };
    println!("{}", exe.display());
    if start && !running() {
        spawn(&exe, &["--auto"], DETACHED);
    }
    0
}
