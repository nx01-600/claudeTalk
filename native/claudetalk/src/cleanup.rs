//! `claudetalk cleanup [--yes]`: removes what older claudeTalk versions left
//! on the system and the native versions no longer use. Without --yes it
//! only lists what it would remove. Only claudeTalk's own leftovers are
//! touched; shared tools (ffmpeg, a system-wide edge-tts) are reported, not
//! removed, since other programs may use them.

use std::path::{Path, PathBuf};

struct Found {
    path: PathBuf,
    why: &'static str,
}

fn dir_size(p: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return 0 };
    if meta.is_file() {
        return meta.len();
    }
    std::fs::read_dir(p).map(|d| d.flatten().map(|e| dir_size(&e.path())).sum()).unwrap_or(0)
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_default())
}

fn push_if(v: &mut Vec<Found>, p: PathBuf, why: &'static str) {
    if p.exists() {
        v.push(Found { path: p, why });
    }
}

fn scan() -> (Vec<Found>, Vec<String>) {
    let mut v = Vec::new();
    let mut notes = Vec::new();
    let root = crate::daemon::plugin_root();
    let appdata = ct_core::paths::state_dir();
    let local = ct_core::paths::local_dir();

    // v0.1-0.5: Python dictation daemon and its virtual environment (~3 GB)
    push_if(&mut v, root.join("voice-input"), "Python dictation daemon (replaced by bin\\claudetalk-dictation.exe)");
    push_if(&mut v, local.join("venv"), "Python venv made by setup-voice.ps1");
    let saved = appdata.join("venv-path.txt");
    if let Ok(text) = std::fs::read_to_string(&saved) {
        let venv = PathBuf::from(text.trim_start_matches('\u{feff}').trim());
        // only a venv that is clearly claudeTalk's
        let s = venv.to_string_lossy().to_lowercase();
        if venv.join("Scripts").join("python.exe").exists() && (s.contains("claudetalk") || s.contains("voice-input")) {
            push_if(&mut v, venv, "Python venv recorded in venv-path.txt");
        }
    }
    push_if(&mut v, saved, "pointer to the old Python venv");
    // v0.5 scripts that may linger next to the new files
    for f in ["setup-voice.ps1", "dictation.vbs", "make-icon.py", "talk-common.ps1", "speak.ps1", "say-server.ps1", "talk-context.ps1", "voice-daemon-ensure.ps1", "voice-toggle.ps1"] {
        push_if(&mut v, root.join("scripts").join(f), "old PowerShell/VBScript/Python script");
    }

    // faster-whisper's copy of the model (the native daemon uses a GGML one)
    let hf = env_path("USERPROFILE").join(".cache").join("huggingface").join("hub");
    for name in ["models--mobiuslabsgmbh--faster-whisper-large-v3-turbo", "models--deepdml--faster-whisper-large-v3-turbo-ct2"] {
        push_if(&mut v, hf.join(name), "faster-whisper model used by the Python daemon");
    }

    // models the native daemon doesn't load
    let keep = [crate::MODEL_FILES[0], crate::MODEL_FILES[1]];
    if let Ok(d) = std::fs::read_dir(local.join("models")) {
        for e in d.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !keep.contains(&name.as_str()) {
                v.push(Found { path: e.path(), why: "model file not used by this version" });
            }
        }
    }

    // daemons downloaded for other versions
    if let Ok(d) = std::fs::read_dir(crate::fetch::versions_dir()) {
        for e in d.flatten() {
            if e.path().is_dir() && e.file_name().to_string_lossy() != crate::fetch::VERSION {
                v.push(Found { path: e.path(), why: "dictation app downloaded for another version" });
            }
        }
    }

    // older copies of the plugin in Claude Code's cache (each old one kept a
    // copy of the Python venv)
    let cache = env_path("USERPROFILE").join(".claude").join("plugins").join("cache").join("claudeTalk").join("claudeTalk");
    if root.starts_with(&cache) {
        if let Ok(d) = std::fs::read_dir(&cache) {
            for e in d.flatten() {
                if e.path().is_dir() && !root.starts_with(e.path()) {
                    v.push(Found { path: e.path(), why: "older copy of the plugin in Claude Code's cache" });
                }
            }
        }
    }

    // leftovers of the old speech worker
    if let Ok(d) = std::fs::read_dir(ct_core::paths::temp_dir()) {
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with("claudetalk_txt_") {
                v.push(Found { path: e.path(), why: "temporary text of the old edge-tts worker" });
            }
        }
    }

    if which("ffplay.exe") || which("ffmpeg.exe") {
        notes.push("ffmpeg is installed. claudeTalk no longer needs it, but other tools may (e.g. local transcription skills): remove it only if nothing else uses it (winget uninstall Gyan.FFmpeg).".into());
    }
    if which("edge-tts.exe") {
        notes.push("edge-tts is on the PATH. claudeTalk no longer needs it: `pip uninstall edge-tts` in that Python if nothing else uses it.".into());
    }
    (v, notes)
}

fn which(exe: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(exe).exists()))
        .unwrap_or(false)
}

/// Start Menu / desktop shortcuts that still launch dictation.vbs, or point
/// at a copy of the plugin that no longer exists (after an update).
fn fix_shortcuts(apply: bool) -> Vec<String> {
    let mut out = Vec::new();
    let Some(exe) = crate::fetch::dictation_exe() else { return out };
    let script = format!(
        "$sh = New-Object -ComObject WScript.Shell; \
         foreach ($p in @(\"$env:APPDATA\\Microsoft\\Windows\\Start Menu\\Programs\\claudeTalk.lnk\", \"$([Environment]::GetFolderPath('Desktop'))\\claudeTalk Dictation.lnk\")) {{ \
           if (Test-Path $p) {{ $s = $sh.CreateShortcut($p); \
             if ($s.Arguments -like '*dictation.vbs*' -or $s.TargetPath -like '*pythonw*' -or -not (Test-Path $s.TargetPath)) {{ \
               Write-Output $p; if ({apply}) {{ $s.TargetPath = '{exe}'; $s.Arguments = ''; $s.IconLocation = '{exe},0'; $s.Save() }} }} }} }}",
        apply = if apply { "$true" } else { "$false" },
        exe = exe.display()
    );
    use std::os::windows::process::CommandExt;
    if let Ok(o) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(0x0800_0000)
        .output()
    {
        out.extend(String::from_utf8_lossy(&o.stdout).lines().map(str::to_string));
    }
    out
}

pub fn run(apply: bool) -> i32 {
    let (found, notes) = scan();
    let mut total = 0u64;
    for f in &found {
        let size = dir_size(&f.path);
        total += size;
        let mut line = format!("{:>9.1} MB  {}  ({})", size as f64 / 1e6, f.path.display(), f.why);
        if apply {
            let r = if f.path.is_dir() { std::fs::remove_dir_all(&f.path) } else { std::fs::remove_file(&f.path) };
            if let Err(e) = r {
                line += &format!("  -> not removed: {e}");
            }
        }
        println!("{line}");
    }
    for s in fix_shortcuts(apply) {
        println!("  shortcut {}: {s}", if apply { "repointed to claudetalk-dictation.exe" } else { "points to an old launcher" });
    }
    if found.is_empty() {
        println!("claudeTalk: nothing left over to clean.");
    } else if apply {
        println!("claudeTalk: removed {:.1} GB of leftovers.", total as f64 / 1e9);
    } else {
        println!("claudeTalk: {:.1} GB can be removed. Run `claudetalk cleanup --yes` to do it.", total as f64 / 1e9);
    }
    for n in notes {
        println!("note: {n}");
    }
    0
}

/// Cheap check for the SessionStart hook (no sizes, no deep walks): true
/// when an older version left something behind.
pub fn leftovers_present() -> bool {
    let root = crate::daemon::plugin_root();
    let local = ct_core::paths::local_dir();
    if root.join("voice-input").exists()
        || root.join("scripts").join("setup-voice.ps1").exists()
        || local.join("venv").exists()
        || ct_core::paths::state_dir().join("venv-path.txt").exists()
    {
        return true;
    }
    let cache = env_path("USERPROFILE").join(".claude").join("plugins").join("cache").join("claudeTalk").join("claudeTalk");
    let old_daemons = std::fs::read_dir(crate::fetch::versions_dir())
        .map(|d| d.flatten().any(|e| e.path().is_dir() && e.file_name().to_string_lossy() != crate::fetch::VERSION))
        .unwrap_or(false);
    old_daemons
        || root.starts_with(&cache)
            && std::fs::read_dir(&cache)
            .map(|d| d.flatten().any(|e| e.path().is_dir() && !root.starts_with(e.path())))
            .unwrap_or(false)
}

/// What the SessionStart hook tells Claude when leftovers exist.
pub fn session_notice() -> String {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "claudetalk.exe".into());
    format!(
        "claudeTalk was upgraded to its native version, and files from older versions are still on this computer \
         (the old Python dictation environment, older plugin copies in Claude Code's cache: often several GB). \
         At a natural moment, tell the user once and offer to clean up. To see what would go: \"{exe}\" cleanup. \
         To remove it, after the user agrees: \"{exe}\" cleanup --yes. It only removes claudeTalk's own leftovers."
    )
}
