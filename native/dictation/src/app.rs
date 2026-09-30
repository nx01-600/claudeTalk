//! The dictation flow (daemon_cli.py): chord or wake phrase -> record with
//! the pill -> transcribe -> paste (or type into the last Claude session).
//! Recording runs on a worker thread; everything the UI shows goes through
//! `post` to the UI thread.

use crate::audio::{self, Mic, RecordParams};
use crate::config::Config;
use crate::stt::{prompt_for, Transcriber};
use crate::{hotkey, inject, signals, sounds};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const MSG_EVENT: u32 = WM_APP + 1;
const WAKE_START_TIMEOUT_MS: f64 = 6000.0;
/// Prefix of every dictation sent to Claude Code; the plugin's hooks
/// (ct_core::SPOKEN_MARK) look for it.
pub const SPOKEN_MARK: &str = ct_core::SPOKEN_MARK;

pub enum UiEvent {
    RecordingStarted,
    Level(f32),
    Transcribing,
    Finished(bool),
    Stopped,
    Status(String),
}

/// The last Claude Code session the user had in front: where "Oye Claude"
/// sends its text. The pid survives Claude Code retitling the session.
#[derive(Clone, Default)]
pub struct Tracked {
    pub hwnd: isize,
    pub topic: Option<String>,
    pub pid: Option<u32>,
}

pub struct Shared {
    pub config: Mutex<Config>,
    busy: AtomicBool,
    force_stop: AtomicBool,
    cancel: AtomicBool,
    pub tracked: Mutex<Tracked>,
    pub transcriber: Transcriber,
    ui: AtomicIsize,
}

impl Shared {
    pub fn new(config: Config, transcriber: Transcriber) -> Arc<Self> {
        Arc::new(Self {
            config: Mutex::new(config),
            busy: AtomicBool::new(false),
            force_stop: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            tracked: Mutex::new(Tracked::default()),
            transcriber,
            ui: AtomicIsize::new(0),
        })
    }

    pub fn set_ui(&self, hwnd: HWND) {
        self.ui.store(hwnd as isize, Ordering::SeqCst);
    }

    pub fn post(&self, ev: UiEvent) {
        let hwnd = self.ui.load(Ordering::SeqCst) as HWND;
        if hwnd.is_null() {
            return;
        }
        let ptr = Box::into_raw(Box::new(ev));
        if unsafe { PostMessageW(hwnd, MSG_EVENT, 0, ptr as isize) } == 0 {
            drop(unsafe { Box::from_raw(ptr) });
        }
    }

    pub fn busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    fn language(&self) -> Option<String> {
        let l = self.config.lock().unwrap().str("language");
        (l != "auto").then_some(l)
    }

    /// Chord tap: the first starts, a second one cuts the recording short.
    pub fn on_press(self: &Arc<Self>) {
        if self.busy.swap(true, Ordering::SeqCst) {
            self.force_stop.store(true, Ordering::SeqCst);
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || me.worker(false));
    }

    /// The wake phrase: like a first chord tap, from the wake listener.
    pub fn on_wake(self: &Arc<Self>) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        std::thread::spawn(move || me.worker(true));
    }

    fn should_cancel(&self, esc: &hotkey::EscapeGrab) -> bool {
        if self.force_stop.load(Ordering::SeqCst) || self.cancel.load(Ordering::SeqCst) {
            return true;
        }
        if esc.pressed() || hotkey::is_key_down(hotkey::VK_ESCAPE) {
            self.cancel.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    fn done(&self) {
        self.force_stop.store(false, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);
        self.busy.store(false, Ordering::SeqCst);
    }

    fn worker(self: Arc<Self>, woken: bool) {
        let hwnd = inject::foreground();
        // Started on the desktop or taskbar: nothing there takes text, so the
        // dictation goes to the last Claude session, like "Oye Claude".
        let to_claude = woken || inject::is_shell_surface(hwnd);
        if to_claude && !woken {
            println!("[target] focus is on the desktop/taskbar; sending to the last Claude session");
        }
        let mut target = self.tracked.lock().unwrap().clone();
        if to_claude && inject::claude_topic(target.hwnd as HWND).is_none() {
            // never seen in front (or that window is gone): take the
            // front-most Claude Code window on screen
            let (h, topic) = inject::find_claude_window();
            println!("[wake] no tracked Claude session; using {topic:?}");
            target = Tracked { hwnd: h as isize, topic, pid: target.pid };
        }
        let (sound, silence_ms, sens, auto_enter, lang) = {
            let c = self.config.lock().unwrap();
            (c.bool("sound"), c.f64("silence_ms", 2000.0) as usize, c.f64("sensitivity", 50.0), c.bool("auto_enter"), c.str("language"))
        };
        self.post(UiEvent::RecordingStarted);
        signals::set_ducking(true);
        if sound {
            sounds::chime_start();
        }
        println!("[recording] speak now...");
        let (margin, peak_ratio) = audio::sensitivity(sens);
        let params = RecordParams {
            silence_hold_ms: silence_ms,
            silence_margin: margin,
            peak_ratio,
            start_timeout_ms: woken.then_some(WAKE_START_TIMEOUT_MS),
        };
        let esc = hotkey::EscapeGrab::new();
        let result = match Mic::open() {
            Ok(mic) => audio::record_until_silence(&mic, &params, || self.should_cancel(&esc), |l| self.post(UiEvent::Level(l))).ok(),
            Err(e) => {
                println!("[recording] mic error: {e}");
                None
            }
        };
        drop(esc);
        signals::set_ducking(false);
        let cancelled = self.cancel.load(Ordering::SeqCst);
        let Some(mut pcm) = result.filter(|p| !p.is_empty() && !cancelled) else {
            self.post(UiEvent::Stopped);
            println!("[cancelled] discarded");
            self.done();
            return;
        };
        audio::noise_gate(&mut pcm, peak_ratio);

        self.post(UiEvent::Transcribing);
        println!("[transcribing] {:.1}s of audio", pcm.len() as f64 / audio::SAMPLE_RATE as f64);
        let t0 = Instant::now();
        let text = match self.transcriber.transcribe(&pcm, self.language().as_deref(), &prompt_for(&lang)) {
            Ok(t) => t,
            Err(e) => {
                println!("[transcribing] failed: {e}");
                String::new()
            }
        };
        // the length only: what the user said never goes into the log
        println!("[text] {} words ({:.2}s)", text.split_whitespace().count(), t0.elapsed().as_secs_f64());
        if text.is_empty() {
            self.post(UiEvent::Stopped);
            println!("[empty] nothing to paste");
        } else {
            let mut text = text;
            if to_claude || inject::is_claude_window(hwnd) {
                // tells Claude (and the talk hooks) this prompt was spoken
                text = format!("{SPOKEN_MARK}{text}");
            }
            let ok = if to_claude {
                inject::paste_into_window(&text, target.hwnd as HWND, target.topic.as_deref(), auto_enter, target.pid)
            } else {
                inject::paste_if_focus_unchanged(&text, hwnd, auto_enter)
            };
            self.post(UiEvent::Finished(ok));
            println!("{}", if ok { "[pasted]" } else { "[not pasted] text left in the clipboard (see [diag] above)" });
        }
        self.done();
    }

    /// Every 500 ms from the UI thread: remembers the Claude window in front.
    pub fn track_claude_window(self: &Arc<Self>) {
        let hwnd = inject::foreground();
        let Some(topic) = inject::claude_topic(hwnd) else { return };
        {
            let t = self.tracked.lock().unwrap();
            if t.hwnd == hwnd as isize && t.topic.as_deref() == Some(topic.as_str()) {
                return;
            }
        }
        {
            let mut t = self.tracked.lock().unwrap();
            t.hwnd = hwnd as isize;
            t.topic = Some(topic.clone());
        }
        // Finding its claude.exe attaches to consoles: off the UI thread.
        let me = self.clone();
        std::thread::spawn(move || {
            let pid = inject::console_pid(Some(&topic));
            let mut t = me.tracked.lock().unwrap();
            if pid.is_some() && t.topic.as_deref() == Some(topic.as_str()) {
                t.pid = pid;
            }
            println!("[wake] Claude session: {topic:?} (pid {:?})", t.pid);
        });
    }
}

// --- lifetime in --auto mode ------------------------------------------------------

/// Alive AND still a Claude Code claude.exe (guards against PID reuse and
/// the Claude desktop app, also Claude.exe, under WindowsApps).
fn is_live_claude(snap: &ct_core::procs::Snapshot, pid: u32) -> bool {
    snap.is_claude(pid) && !process_path(pid).to_lowercase().contains("\\windowsapps\\")
}

fn process_path(pid: u32) -> String {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::*;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..size as usize])
        }
    }
}

fn live_sessions(snap: &ct_core::procs::Snapshot) -> Vec<u32> {
    let path = ct_core::paths::sessions_txt();
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    let pids: Vec<u32> = text.split_whitespace().filter_map(|l| l.parse().ok()).collect();
    let alive: Vec<u32> = pids.iter().copied().filter(|&p| is_live_claude(snap, p)).collect();
    if alive != pids {
        let _ = std::fs::write(&path, alive.iter().map(|p| format!("{p}\n")).collect::<String>());
    }
    alive
}

/// Interactive claude.exe processes running now, by command line (only
/// asked when sessions.txt says none are left).
fn interactive_claude_pids(snap: &ct_core::procs::Snapshot) -> Vec<u32> {
    snap.0
        .iter()
        .filter(|(_, p)| p.exe == "claude.exe")
        .map(|(&pid, _)| pid)
        .filter(|&pid| is_live_claude(snap, pid))
        .filter(|&pid| {
            ct_core::procs::command_line(pid).is_some_and(|c| !ct_core::procs::is_headless_cmdline(&c) && !c.contains("--type="))
        })
        .collect()
}

pub struct Watchdog {
    started: Instant,
}

impl Watchdog {
    pub fn new() -> Self {
        Self { started: Instant::now() }
    }

    /// True when the daemon should exit: no interactive Claude Code session
    /// is left (and it isn't the manually launched app).
    pub fn should_exit(&self, shared: &Shared) -> bool {
        if ct_core::paths::state_dir().join("persistent.flag").exists() {
            return false;
        }
        let snap = ct_core::procs::Snapshot::take();
        if !live_sessions(&snap).is_empty() {
            return false;
        }
        if self.started.elapsed().as_secs() < 30 {
            return false; // the hook may still be writing the first session
        }
        if shared.busy() {
            return false; // never cut a dictation in progress
        }
        let found = interactive_claude_pids(&snap);
        if !found.is_empty() {
            let _ = std::fs::write(ct_core::paths::sessions_txt(), found.iter().map(|p| format!("{p}\n")).collect::<String>());
            println!("[auto] sessions.txt had no live session; re-registered {found:?}");
            return false;
        }
        println!("[auto] no Claude Code session left; dictation exits");
        true
    }
}
