//! Starting the dictation daemon (voice-daemon-ensure.ps1 in v0.5).

use ct_core::lock::wide;
use ct_core::paths;
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

/// pythonw.exe of the dictation venv, if dictation is installed. Same lookup
/// order as v0.5: the path setup-voice.ps1 saved, the repo's .venv, then the
/// default %LOCALAPPDATA%\claudeTalk\venv.
fn python() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(saved) = std::fs::read_to_string(paths::state_dir().join("venv-path.txt")) {
        let saved = saved.trim_start_matches('\u{feff}').trim();
        if !saved.is_empty() {
            candidates.push(PathBuf::from(saved).join("Scripts\\pythonw.exe"));
        }
    }
    candidates.push(plugin_root().join("voice-input\\.venv\\Scripts\\pythonw.exe"));
    candidates.push(paths::local_dir().join("venv\\Scripts\\pythonw.exe"));
    candidates.into_iter().find(|p| p.exists())
}

/// Starts dictation in --auto mode unless it runs already or isn't installed.
/// Returns what happened, for /dictation.
pub fn ensure() -> &'static str {
    if running() {
        return "running";
    }
    let Some(py) = python() else { return "not-installed" };
    let script_dir = plugin_root().join("voice-input");
    use std::os::windows::process::CommandExt;
    let spawned = std::process::Command::new(py)
        .arg(script_dir.join("daemon_cli.py"))
        .arg("--auto")
        .current_dir(&script_dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000 | 0x0000_0008 | 0x0100_0000)
        .spawn()
        .or_else(|_| {
            std::process::Command::new(python().unwrap())
                .arg(script_dir.join("daemon_cli.py"))
                .arg("--auto")
                .current_dir(&script_dir)
                .creation_flags(0x0800_0000 | 0x0000_0008)
                .spawn()
        });
    if spawned.is_ok() {
        "started"
    } else {
        "failed"
    }
}
