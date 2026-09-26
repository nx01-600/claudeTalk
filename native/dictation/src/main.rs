//! claudetalk-dictation.exe: voice dictation for Claude Code, native port of
//! voice-input/ (daemon_cli.py and friends).
//!
//!   claudetalk-dictation            manual launch (the "app"): stays until
//!                                   turned off from the tray or the gear
//!   claudetalk-dictation --auto     started by the SessionStart hook; exits
//!                                   once no Claude Code session is left
//!
//! Output goes to %TEMP%\claudetalk-dictation.log.

#![windows_subsystem = "windows"]

mod app;
mod audio;
mod config;
mod hotkey;
mod inject;
mod models;
mod signals;
mod sounds;
mod stt;
mod ui;
mod wake;

use std::os::windows::io::AsRawHandle;
use std::sync::Arc;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};

/// The plugin's claudetalk.exe (voice previews): next to this exe in a
/// local build, else where the plugin's hooks last ran from (they record it
/// in %APPDATA%\claudeTalk\claudetalk-path.txt).
pub fn claudetalk_exe() -> Option<std::path::PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join("claudetalk.exe");
    if beside.exists() {
        return Some(beside);
    }
    let saved = std::fs::read_to_string(ct_core::paths::state_dir().join("claudetalk-path.txt")).ok()?;
    let p = std::path::PathBuf::from(saved.trim());
    p.exists().then_some(p)
}

fn redirect_output() {
    let path = std::env::temp_dir().join("claudetalk-dictation.log");
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let h = f.as_raw_handle();
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, h as _);
            SetStdHandle(STD_ERROR_HANDLE, h as _);
        }
        std::mem::forget(f); // stays open for the whole run
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--render-test") {
        let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_else(|| ".".into()));
        let scale = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
        ui::render_test(&dir, &config::Config::load(), scale);
        return;
    }
    redirect_output();
    let auto = std::env::args().any(|a| a == "--auto");

    // Single instance: two would both react to the same chord.
    let _mutex = unsafe { CreateMutexW(std::ptr::null(), 0, ct_core::lock::wide("Local\\claudeTalk-dictation").as_ptr()) };
    if unsafe { GetLastError() } == 183 {
        println!("[info] another dictation instance is already running; this one exits");
        return;
    }
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    // Vulkan's D3D12 translation layer (Dozen) adds nothing next to the real
    // drivers and pulls in the whole D3D12 stack (~150 MB): skip it.
    if std::env::var_os("VK_LOADER_DRIVERS_DISABLE").is_none() {
        std::env::set_var("VK_LOADER_DRIVERS_DISABLE", "*dzn*");
    }
    if !auto {
        // Launched by hand: stays up through Claude Code sessions coming and
        // going, until turned off.
        let _ = std::fs::create_dir_all(ct_core::paths::state_dir());
        let _ = std::fs::write(ct_core::paths::state_dir().join("persistent.flag"), b"");
    }

    let transcriber = stt::Transcriber::new();
    let shared = app::Shared::new(config::Config::load(), transcriber.clone());

    // Models: download once, then load and warm up so the first dictation
    // doesn't wait for the upload or the Vulkan pipelines.
    {
        let shared = shared.clone();
        std::thread::spawn(move || {
            if models::missing() {
                println!("[model] downloading");
                let mut last = 0u64;
                let result = models::ensure(|file, done, total| {
                    let pct = if total > 0 { done * 100 / total } else { 0 };
                    if pct != last && file == stt::WHISPER_FILE {
                        last = pct;
                        shared.post(app::UiEvent::Status(format!("downloading the voice model {pct}%")));
                    }
                });
                if let Err(e) = result {
                    println!("[model] download failed: {e}");
                    shared.post(app::UiEvent::Status("voice model download failed (see log)".into()));
                    return;
                }
                let label = config::hotkey_label(&shared.config.lock().unwrap().hotkey());
                shared.post(app::UiEvent::Status(label));
            }
            if std::env::var_os("CLAUDETALK_NO_WARMUP").is_none() {
                shared.transcriber.warm_up();
            }
        });
    }

    let hooks = {
        let (s1, s2, s3, s4, s5, s6) = (shared.clone(), shared.clone(), shared.clone(), shared.clone(), shared.clone(), shared.clone());
        wake::Hooks {
            should_listen: Box::new(move || s1.config.lock().unwrap().bool("wake_word") && ct_core::paths::talk_flag().exists()),
            is_busy: Box::new(move || s2.busy()),
            margin: Box::new(move || audio::sensitivity(s3.config.lock().unwrap().f64("sensitivity", 50.0)).0),
            language: Box::new(move || {
                let l = s4.config.lock().unwrap().str("language");
                (l != "auto").then_some(l)
            }),
            phrase: Box::new(move || s5.config.lock().unwrap().str("wake_phrase")),
            on_wake: Box::new(move || s6.on_wake()),
        }
    };
    wake::spawn(transcriber, hooks);

    let label = config::hotkey_label(&shared.config.lock().unwrap().hotkey());
    println!("Dictation ready: {label}. {}", if auto { "Auto mode (tied to Claude Code)." } else { "Turn off from the tray or the gear." });
    ui::run(Arc::clone(&shared), auto);
    signals::set_ducking(false);
}
