//! The UI thread: one hidden window for raw input, the tray and worker
//! events, plus the pill, the two panels and the text field. Timers run
//! only while something is on screen, so an idle daemon sleeps.

pub mod editor;
pub mod gfx;
pub mod glass;
pub mod panel;
pub mod pill;
pub mod talk;
pub mod tray;
pub mod window;

use crate::app::{Shared, UiEvent, Watchdog, MSG_EVENT};
use crate::config::hotkey_label;
use crate::hotkey::{self, Chord};
use editor::Editor;
use glass::{Material, INSET};
use panel::{Action, Kind, Panel, PANEL_W};
use pill::{Click, Pill};
use talk::{Act, TalkPill};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::sync::Arc;
use tray::Tray;
use window::{wide, Layered};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const WM_MOUSELEAVE_: u32 = 0x02A3;
const ID_MAIN: isize = 0;
const ID_PILL: isize = 1;
const ID_PANEL: isize = 2;
const ID_VOICE: isize = 3;
const ID_EDITOR: isize = 4;
const ID_TALK: isize = 5;

const T_TRACK: usize = 2;
const T_CONFIG: usize = 3;
const T_WATCHDOG: usize = 4;
const T_CAPTURE: usize = 5;
const T_LIVE: usize = 6;
const T_BADGE: usize = 7;
const T_SPEAK: usize = 8;
/// Live glass: one window's backdrop is re-captured and re-blurred per tick,
/// in turns, so no frame pays for two blurs.
const LIVE_MS: u32 = 45;

struct App {
    shared: Arc<Shared>,
    main: HWND,
    pill: Pill,
    talk: TalkPill,
    panel: Panel,
    voice: Panel,
    editor: Editor,
    tray: Tray,
    chord: Chord,
    watchdog: Option<Watchdog>,
    badge_token: u64,
    /// Round robin over the windows whose glass T_LIVE refreshes.
    live_turn: usize,
    /// The pill's spot from the last drag, kept while the pill or the
    /// capsule is on screen (see saved_drag).
    anchor: Option<(i32, i32)>,
    /// Frames the talking capsule has been gliding (its glass is refreshed
    /// every few of them, and once when it stops).
    glide: u32,
    /// Polls in a row with phrases queued and no speaker (signals::heal_speaker).
    stranded: u32,
    frame_on: bool,
    tracking_mouse: [bool; 6],
    cursor: *const u16,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Worker events that arrived while the app was busy in a modal loop
    /// (tray menu, message box); handled on the next chance.
    static PENDING: RefCell<Vec<UiEvent>> = const { RefCell::new(Vec::new()) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

fn material(shared: &Shared) -> Material {
    let g = shared.config.lock().unwrap().f64("glass", 60.0);
    Material { glass: (g / 100.0).clamp(0.0, 1.0) as f32 }
}

impl App {
    fn cfg_bool(&self, key: &str) -> bool {
        self.shared.config.lock().unwrap().bool(key)
    }

    fn top(&self) -> bool {
        self.shared.config.lock().unwrap().str("position") == "top"
    }

    fn capturable(&self) -> bool {
        self.cfg_bool("show_in_capture")
    }

    /// Where the pill goes when it appears (window origin): the remembered
    /// spot with "Remember dragged spot" on; otherwise where the pill or the
    /// talking capsule was last dragged, for as long as one of them stays
    /// on screen.
    fn saved_drag(&self) -> Option<(i32, i32)> {
        let c = self.shared.config.lock().unwrap();
        if !c.bool("remember_drag") {
            return self.anchor;
        }
        let a = c.get("drag_pos").as_array()?;
        Some((a.first()?.as_f64()? as i32, a.get(1)?.as_f64()? as i32))
    }

    fn panel_open(&self) -> bool {
        self.panel.visible()
    }

    fn start_frames(&mut self) {
        if !self.frame_on {
            self.frame_on = true;
            pacer::run(true);
        }
        let live = !self.capturable() && (self.pill.win.visible || self.panel.win.visible || self.talk.win.visible);
        unsafe {
            if live {
                SetTimer(self.main, T_LIVE, LIVE_MS, None);
            } else {
                KillTimer(self.main, T_LIVE);
            }
        }
    }

    fn stop_frames(&mut self) {
        self.frame_on = false;
        pacer::run(false);
    }

    fn redraw_all(&mut self) {
        self.redraw(true, true);
    }

    fn redraw(&mut self, pill: bool, panels: bool) {
        let m = material(&self.shared);
        let top = self.top();
        if pill && (self.pill.win.visible || self.pill.win.opacity > 0.0) {
            let pm = self.pill.render(m, top);
            self.pill.win.present(&pm);
        }
        if pill && self.talk.win.visible {
            let pm = self.talk.render(m);
            self.talk.win.present(&pm);
        }
        if panels {
            let cfg = self.shared.config.lock().unwrap();
            for p in [&mut self.panel, &mut self.voice] {
                if p.win.visible {
                    let pm = p.render(&cfg, m);
                    p.win.present(&pm);
                }
            }
        }
    }

    /// One animation frame. The pill repaints every frame while it's up
    /// (its bars move); the panels only while something in them moves.
    fn frame(&mut self) {
        let _t = perf::Timer::new("frame");
        let mut pill_on = false;
        if self.pill.win.visible || self.pill.win.opacity > 0.0 {
            pill_on = self.pill.tick() && self.pill.win.visible;
        }
        if self.talk.win.visible {
            self.aim_talk();
            let level = ct_core::voice_link::get().map(|l| l.level()).unwrap_or(0.0);
            let (on, moved) = self.talk.tick(level);
            // While it glides the glass follows every 4th frame (a blur is
            // several ms), and once more where it lands.
            if moved {
                self.glide += 1;
            }
            let landed = !moved && self.glide > 0;
            if !self.capturable() && ((moved && self.glide % 4 == 0) || landed) {
                self.talk.refresh_background(material(&self.shared), false);
            }
            if landed {
                self.glide = 0;
            }
            pill_on |= on;
        }
        let panel_was = self.panel.win.visible;
        let mut panels_moving = false;
        {
            let cfg = self.shared.config.lock().unwrap();
            for p in [&mut self.panel, &mut self.voice] {
                if p.win.visible && p.animating(&cfg) {
                    p.tick(&cfg);
                    panels_moving = true;
                }
            }
        }
        if panel_was && !self.panel.win.visible {
            self.on_panel_closed();
        }
        self.redraw(pill_on, panels_moving);
        if !pill_on && !panels_moving {
            self.stop_frames();
        }
        if !self.pill.win.visible && !self.talk.win.visible {
            self.anchor = None; // both gone: the next ones start from home
        }
        if !self.pill.win.visible && !self.panel.win.visible && !self.voice.win.visible && !self.talk.win.visible {
            unsafe { KillTimer(self.main, T_LIVE) };
        }
    }

    /// Re-captures what's behind the visible windows; repaints the ones
    /// whose backdrop changed.
    fn refresh_backgrounds(&mut self) {
        let _t = perf::Timer::new("glass");
        let m = material(&self.shared);
        let cap = self.capturable();
        let mut pill = self.pill.win.visible && self.pill.refresh_background(m, cap);
        pill |= self.talk.win.visible && self.talk.refresh_background(m, cap);
        let mut panels = false;
        if self.panel.win.visible {
            panels |= self.panel.refresh_background(m, cap);
        }
        if self.voice.win.visible {
            panels |= self.voice.refresh_background(m, cap);
        }
        if (pill && !self.frame_on) || panels {
            self.redraw(pill, panels);
        }
    }

    /// T_LIVE: re-captures the backdrop of the next visible window in turn
    /// and repaints it if what's behind changed.
    fn refresh_live_one(&mut self) {
        let _t = perf::Timer::new("glass1");
        let m = material(&self.shared);
        let visible = [self.pill.win.visible, self.talk.win.visible, self.panel.win.visible, self.voice.win.visible];
        let Some(k) = (1..=4).map(|i| (self.live_turn + i) % 4).find(|&k| visible[k]) else { return };
        self.live_turn = k;
        match k {
            0 | 1 => {
                let changed = if k == 0 { self.pill.refresh_background(m, false) } else { self.talk.refresh_background(m, false) };
                if changed && !self.frame_on {
                    self.redraw(true, false);
                }
            }
            _ => {
                let changed = if k == 2 { self.panel.refresh_background(m, false) } else { self.voice.refresh_background(m, false) };
                if changed {
                    self.redraw(false, true);
                }
            }
        }
    }

    // --- worker events -------------------------------------------------------------

    fn event(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::RecordingStarted => {
                let (top, saved, cap) = (self.top(), self.saved_drag(), self.capturable());
                let was_visible = self.pill.win.visible;
                self.pill.fade_in(top, saved, cap);
                if !was_visible {
                    self.pill.refresh_background(material(&self.shared), cap);
                    self.pill.win.visible = true;
                    let pm = self.pill.render(material(&self.shared), top);
                    self.pill.win.present(&pm);
                    self.pill.win.visible = false;
                    self.pill.win.show();
                }
                self.start_frames();
            }
            UiEvent::Level(l) => self.pill.set_level(l),
            UiEvent::Transcribing => self.pill.show_busy(),
            UiEvent::Finished(ok) => {
                if let Some(token) = self.pill.show_result(ok) {
                    self.badge_token = token;
                    unsafe { SetTimer(self.main, T_BADGE, pill::badge_total_ms() as u32, None) };
                }
            }
            UiEvent::Stopped => {
                let open = self.panel_open();
                self.pill.fade_out(open);
            }
            UiEvent::Status(s) => self.tray.set_label(&s),
        }
    }

    // --- "Claude is talking" pill -------------------------------------------------

    /// Centered where the dictation pill goes; to its left while it's up.
    fn aim_talk(&mut self) {
        let top = self.top();
        let (cx, cy, s) = self.pill.center(top, self.saved_drag());
        let beside = self.pill.win.visible && !self.pill.closing;
        let cx = if beside { cx - (pill::WIDTH / 2.0 + talk::GAP + talk::WIDTH / 2.0) * s } else { cx };
        self.talk.aim(cx, cy, s);
    }

    /// Polled a few times a second: shows the pill while Claude's voice
    /// plays (or is about to), hides it once Claude has been quiet a moment.
    fn poll_talk(&mut self) {
        self.test_trigger();
        self.stranded = crate::signals::heal_speaker(self.stranded);
        let talking = self.cfg_bool("speaking_indicator") && crate::wake::claude_is_talking();
        if talking || self.talk.win.visible {
            // The phrase playing is still the first file in the queue; the
            // rest wait. Answers from another session get their own color.
            let pending = ct_core::queue::pending();
            self.talk.queued = pending.len().saturating_sub(1);
            let playing = std::fs::read_to_string(ct_core::paths::player_session_file()).unwrap_or_default();
            let playing = playing.lines().next().unwrap_or("").trim().to_string();
            self.talk.other_session = pending
                .iter()
                .skip(1)
                .filter_map(|p| ct_core::queue::read_item(p))
                .any(|it| it.session != playing);
            self.talk.paused = ct_core::voice_link::get().is_some_and(|l| l.paused());
            self.talk.volume = self.shared.config.lock().unwrap().f64("tts_volume", 100.0).round() as i32;
        }
        if self.talk.heard(talking) {
            let cap = self.capturable();
            self.aim_talk();
            self.talk.fade_in(cap);
            if !self.talk.win.visible {
                self.talk.refresh_background(material(&self.shared), cap);
                self.talk.win.visible = true;
                let pm = self.talk.render(material(&self.shared));
                self.talk.win.present(&pm);
                self.talk.win.visible = false;
                self.talk.win.show();
            }
            self.start_frames();
        } else if self.talk.shown() && (self.talk.quiet() || !self.cfg_bool("speaking_indicator")) {
            self.talk.fade_out();
            self.start_frames();
        }
    }

    /// Test instances only (CLAUDETALK_NS set): `Local\claudetalk_test_record`
    /// plus the namespace shows or hides the dictation pill as if a recording
    /// started or ended, without the mic, the keyboard or pasting anything,
    /// so live layout and timing tests can't disturb the user's windows.
    fn test_trigger(&mut self) {
        thread_local!(static TEST: bool = std::env::var("CLAUDETALK_NS").is_ok_and(|v| !v.is_empty()));
        if !TEST.with(|t| *t) {
            return;
        }
        use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
        thread_local!(static EV: HANDLE = unsafe { CreateEventW(std::ptr::null(), 0, 0, ct_core::lock::named(r"Local\claudetalk_test_record").as_ptr()) });
        if EV.with(|h| unsafe { WaitForSingleObject(*h, 0) }) == WAIT_OBJECT_0 {
            if self.pill.win.visible && !self.pill.closing {
                self.event(UiEvent::Stopped);
            } else {
                self.event(UiEvent::RecordingStarted);
            }
        }
    }

    /// The capsule's buttons and volume slider.
    fn talk_actions(&mut self, acts: Vec<Act>) {
        for a in acts {
            match a {
                // Only the phrase playing goes: the answers queued behind it
                // (from any session) still play, each with its own voice.
                // Talk mode and dictation stay on.
                Act::Stop => {
                    if let Some(link) = ct_core::voice_link::get() {
                        link.set_paused(false);
                    }
                    self.talk.paused = false;
                    ct_core::queue::signal_cut();
                    if self.talk.queued == 0 {
                        self.talk.silenced();
                    }
                }
                Act::TogglePause => {
                    if let Some(link) = ct_core::voice_link::get() {
                        link.set_paused(!link.paused());
                        self.talk.paused = link.paused();
                    }
                }
                // Only the phrase playing goes; the next one starts (playing).
                Act::Skip => {
                    if let Some(link) = ct_core::voice_link::get() {
                        link.set_paused(false);
                    }
                    self.talk.paused = false;
                    ct_core::queue::signal_cut();
                }
                Act::Volume(v) => {
                    if let Some(link) = ct_core::voice_link::get() {
                        link.push_volume(v);
                    }
                }
                Act::VolumeDone(v) => {
                    if let Some(link) = ct_core::voice_link::get() {
                        link.push_volume(v);
                    }
                    self.shared.config.lock().unwrap().set("tts_volume", json!(v));
                    self.after_change("tts_volume");
                }
                // The capsule and the pill move as one group: dragging the
                // capsule drags the pill beside it, and sets where the pill
                // (and so the capsule) appears next.
                Act::Dragged(cx, cy) => {
                    let s = self.talk.scale;
                    let top = self.top();
                    if self.pill.win.visible && !self.pill.closing {
                        let px = cx + (pill::WIDTH / 2.0 + talk::GAP + talk::WIDTH / 2.0) * s;
                        let (x, y) = Pill::origin_for_center(px, cy, top, s);
                        self.pill.win.x = x;
                        self.pill.win.y = y;
                    }
                    self.redraw(true, false);
                }
                Act::DragEnd => {
                    let s = self.talk.scale;
                    let top = self.top();
                    let (x, y) = if self.pill.win.visible && !self.pill.closing {
                        (self.pill.win.x, self.pill.win.y)
                    } else {
                        let (cx, cy) = (
                            self.talk.win.x as f32 + (INSET + talk::WIDTH / 2.0) * s,
                            self.talk.win.y as f32 + self.talk.cap_center_y() * s,
                        );
                        Pill::origin_for_center(cx, cy, top, s)
                    };
                    self.anchor = Some((x, y));
                    if self.cfg_bool("remember_drag") {
                        self.shared.config.lock().unwrap().set("drag_pos", json!([x, y]));
                    }
                    self.refresh_backgrounds();
                }
                Act::Capture(on) => unsafe {
                    if on {
                        SetCapture(self.talk.win.hwnd);
                    } else {
                        ReleaseCapture();
                    }
                },
            }
        }
        self.start_frames();
    }

    // --- panels ---------------------------------------------------------------------

    fn open_panel_near_pill(&mut self) {
        let top = self.top();
        let (px, py, pw, ph) = self.pill.pill_rect_on_screen(top);
        let s = self.pill.scale;
        let m = window::monitor_at((px + pw / 2.0) as i32, (py + ph / 2.0) as i32, true).unwrap();
        self.panel.scale = s;
        self.voice.scale = s;
        let (w, h) = self.panel.phys_size();
        let mut x = (px + pw) as i32 - ((PANEL_W + INSET) * s).round() as i32;
        self.panel.opens_down = self.pill.opens_down(top);
        let y = if self.panel.opens_down {
            (py + ph) as i32 + ((6.0 - INSET) * s).round() as i32
        } else {
            py as i32 - ((6.0 + INSET) * s).round() as i32 - (self.panel.content_height() * s).round() as i32
        };
        let _ = h;
        x = x.clamp(m.work.left + ((8.0 - INSET) * s) as i32, m.work.right - w as i32 + ((INSET - 8.0) * s) as i32);
        self.open_panel_at(x, y);
        self.pill.gear_angle_target = 120.0;
    }

    fn open_standalone(&mut self) {
        let m = window::primary();
        self.panel.scale = m.scale;
        self.voice.scale = m.scale;
        let (w, h) = self.panel.phys_size();
        let x = m.work.left + ((m.work.right - m.work.left) - w as i32) / 2;
        let y = m.work.bottom - h as i32 - (96.0 * m.scale) as i32;
        self.panel.opens_down = false;
        self.open_panel_at(x, y);
    }

    fn open_panel_at(&mut self, x: i32, y: i32) {
        let m = material(&self.shared);
        {
            let cfg = self.shared.config.lock().unwrap();
            self.panel.open_at(x, y, &cfg, m);
        }
        // companion to the right (or left without room), bottoms aligned
        // (tops when the panel opens downward)
        let s = self.panel.scale;
        let gap = 10.0;
        let work = window::primary().work;
        let mut cx = x + ((PANEL_W + gap) * s).round() as i32;
        if cx as f32 + (PANEL_W + INSET) * s > work.right as f32 - 8.0 * s {
            cx = x - ((PANEL_W + gap) * s).round() as i32;
        }
        let mut cy = y;
        if !self.panel.opens_down {
            cy += ((self.panel.content_height() - self.voice.content_height()) * s).round() as i32;
        }
        {
            let cfg = self.shared.config.lock().unwrap();
            self.voice.open_at(cx, cy, &cfg, m);
        }
        self.panel.win.show();
        self.voice.win.show();
        self.redraw_all();
        self.start_frames();
    }

    fn close_panels(&mut self) {
        if let Some((key, value)) = self.editor.finish(true) {
            self.commit_text(&key, &value);
        }
        let mut acts = self.panel.close();
        acts.extend(self.voice.close());
        self.run_actions(acts, Kind::Dictation);
        self.start_frames();
    }

    fn on_panel_closed(&mut self) {
        self.pill.gear_angle_target = 0.0;
        if self.pill.hide_pending {
            self.pill.hide_pending = false;
            self.pill.fade_out(false);
        }
    }

    fn commit_text(&mut self, key: &str, value: &str) {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if !value.is_empty() && self.shared.config.lock().unwrap().str(key) != value {
            self.set(key, json!(value), Kind::Voice);
        }
        self.redraw_all();
    }

    fn preview(&self, sample: &str) {
        let (voice, rate, vol) = {
            let c = self.shared.config.lock().unwrap();
            (c.str("tts_voice"), c.str("tts_rate"), c.f64("tts_volume", 100.0) as i64)
        };
        if let Some(exe) = crate::claudetalk_exe() {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new(exe)
                .args(["preview", &voice, &rate, &vol.to_string(), sample])
                .creation_flags(0x0800_0000)
                .spawn();
        }
    }

    fn set(&mut self, key: &str, value: Value, from: Kind) {
        if key == "lang" {
            // A language moves the voice, the dictation and the wake phrase
            // along (ct_core::lang::apply writes them); take them all in.
            if let Err(e) = ct_core::lang::apply(value.as_str().unwrap_or_default()) {
                println!("[settings] language: {e}");
            }
            let changed = self.shared.config.lock().unwrap().reload_if_changed();
            for k in changed {
                self.after_change(&k);
            }
            return;
        }
        self.shared.config.lock().unwrap().set(key, value);
        self.after_change(key);
        if from == Kind::Voice && (key == "tts_voice" || key == "tts_rate") {
            self.preview("voice");
        }
    }

    /// Side effects of a setting that changed (from a panel or on disk).
    fn after_change(&mut self, key: &str) {
        println!("[settings] {key} = {}", self.shared.config.lock().unwrap().get(key));
        match key {
            "hotkey" => {
                self.chord.keys = self.shared.config.lock().unwrap().hotkey();
                let label = hotkey_label(&self.chord.keys);
                self.tray.set_label(&label);
            }
            "glass" => self.refresh_backgrounds(),
            "speaking_indicator" => self.poll_talk(),
            "lang" => {
                ct_core::lang::reload();
                let cfg = self.shared.config.lock().unwrap();
                self.panel.relabel(&cfg);
                self.voice.relabel(&cfg);
                drop(cfg);
                let label = hotkey_label(&self.chord.keys);
                self.tray.set_label(&label);
                self.refresh_backgrounds();
            }
            "position" => {
                // picking Bottom/Top forgets any dragged spot
                let has = !self.shared.config.lock().unwrap().get("drag_pos").is_null();
                if has {
                    self.shared.config.lock().unwrap().set("drag_pos", Value::Null);
                }
                if self.pill.win.visible {
                    let top = self.top();
                    self.pill.place(top, None);
                    self.refresh_backgrounds();
                }
            }
            "show_in_capture" => {
                let cap = self.capturable();
                for w in [&self.pill.win, &self.panel.win, &self.voice.win] {
                    w.set_capturable(cap);
                }
                self.start_frames();
            }
            _ => {}
        }
        self.redraw_all();
    }

    fn run_actions(&mut self, acts: Vec<Action>, from: Kind) {
        for a in acts {
            match a {
                Action::Set(k, v) => self.set(&k, v, from),
                Action::CaptureStarted => {
                    self.chord.paused = true;
                    unsafe { SetTimer(self.main, T_CAPTURE, 30, None) };
                }
                Action::CaptureFinished => {
                    self.chord.paused = false;
                    unsafe { KillTimer(self.main, T_CAPTURE) };
                }
                Action::Close => self.close_panels(),
                Action::Quit => {
                    self.close_panels();
                    quit();
                }
                Action::EditText { key, value, x, y, w, h } => {
                    let s = self.voice.scale;
                    self.editor.open(&key, &value, x, y, w, h, s);
                }
                Action::Preview(sample) => self.preview(sample),
            }
        }
        self.start_frames();
    }

    // --- mouse on the glass windows -----------------------------------------------------

    fn mouse(&mut self, id: isize, msg: u32) {
        let (sx, sy) = window::cursor_pos();
        let top = self.top();
        let open = self.panel_open();
        let hwnd = match id {
            ID_PILL => self.pill.win.hwnd,
            ID_PANEL => self.panel.win.hwnd,
            ID_TALK => self.talk.win.hwnd,
            _ => self.voice.win.hwnd,
        };
        if msg == WM_MOUSEMOVE && !self.tracking_mouse[id as usize] {
            let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
            unsafe { TrackMouseEvent(&mut tme) };
            self.tracking_mouse[id as usize] = true;
        }
        match id {
            ID_TALK => match msg {
                WM_MOUSEMOVE => {
                    let (hand, acts) = self.talk.mouse_move(sx, sy);
                    self.cursor = if hand { IDC_HAND } else { IDC_SIZEALL };
                    if self.talk.needs_glass() {
                        let cap = self.capturable();
                        self.talk.refresh_background(material(&self.shared), cap);
                    }
                    self.talk_actions(acts);
                }
                WM_MOUSELEAVE_ => {
                    self.tracking_mouse[ID_TALK as usize] = false;
                    self.talk.mouse_leave();
                }
                WM_LBUTTONDOWN => {
                    let acts = self.talk.mouse_down(sx, sy);
                    self.talk_actions(acts);
                }
                WM_LBUTTONUP => {
                    let acts = self.talk.mouse_up();
                    self.talk_actions(acts);
                }
                _ => {}
            },
            ID_PILL => match msg {
                WM_MOUSEMOVE => {
                    let (gear, drag_started) = self.pill.mouse_move(sx, sy, top, open);
                    if drag_started && open {
                        self.close_panels();
                    }
                    self.cursor = if gear { IDC_HAND } else { IDC_SIZEALL };
                    if self.pill.dragging {
                        self.redraw_all();
                    }
                }
                WM_MOUSELEAVE_ => {
                    self.tracking_mouse[1] = false;
                    self.pill.mouse_leave(open);
                }
                WM_LBUTTONDOWN => {
                    if let Click::TogglePanel = self.pill.mouse_down(sx, sy, top) {
                        if open {
                            self.close_panels();
                        } else {
                            self.open_panel_near_pill();
                        }
                    } else {
                        unsafe { SetCapture(hwnd) };
                    }
                }
                WM_LBUTTONUP => {
                    unsafe { ReleaseCapture() };
                    if self.pill.mouse_up() {
                        let (x, y) = (self.pill.win.x, self.pill.win.y);
                        self.anchor = Some((x, y));
                        if self.cfg_bool("remember_drag") {
                            self.shared.config.lock().unwrap().set("drag_pos", json!([x, y]));
                        }
                        // the live glass follows by itself; the screen-share
                        // snapshot can't be retaken while the pill is up
                        self.refresh_backgrounds();
                    }
                }
                _ => {}
            },
            _ => {
                let kind = if id == ID_PANEL { Kind::Dictation } else { Kind::Voice };
                let acts = {
                    let cfg = self.shared.config.lock().unwrap();
                    let p = if id == ID_PANEL { &mut self.panel } else { &mut self.voice };
                    match msg {
                        WM_MOUSEMOVE => {
                            let (acts, pointer) = p.mouse_move(sx, sy, &cfg);
                            self.cursor = if pointer { IDC_HAND } else { IDC_ARROW };
                            acts
                        }
                        WM_MOUSELEAVE_ => {
                            self.tracking_mouse[id as usize] = false;
                            p.mouse_leave();
                            Vec::new()
                        }
                        WM_LBUTTONDOWN => {
                            unsafe { SetCapture(hwnd) };
                            p.mouse_down(sx, sy, &cfg)
                        }
                        WM_LBUTTONUP => {
                            unsafe { ReleaseCapture() };
                            p.mouse_up()
                        }
                        _ => Vec::new(),
                    }
                };
                self.run_actions(acts, kind);
            }
        }
        self.redraw_all();
        self.start_frames();
    }

    fn timer(&mut self, id: usize) {
        match id {

            T_SPEAK => self.poll_talk(),
            T_LIVE => {
                if !self.capturable() {
                    self.refresh_live_one();
                }
            }
            T_TRACK => self.shared.track_claude_window(),
            T_CONFIG => {
                let changed = self.shared.config.lock().unwrap().reload_if_changed();
                for key in changed {
                    self.after_change(&key);
                }
            }
            T_WATCHDOG => {
                if self.watchdog.as_ref().is_some_and(|w| w.should_exit(&self.shared)) {
                    quit();
                }
            }
            T_CAPTURE => {
                let mut acts = self.panel.capture_tick();
                acts.extend(self.voice.capture_tick());
                self.run_actions(acts, Kind::Dictation);
            }
            T_BADGE => {
                unsafe { KillTimer(self.main, T_BADGE) };
                if self.badge_token == self.pill.phase_token {
                    let open = self.panel_open();
                    self.pill.fade_out(open);
                    self.start_frames();
                }
            }
            _ => {}
        }
    }

    fn tray_event(&mut self, lp: LPARAM) {
        let ev = (lp & 0xFFFF) as u32;
        if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
            match self.tray.menu() {
                tray::CMD_SETTINGS => self.open_standalone(),
                tray::CMD_QUIT => {
                    if tray::confirm_quit() {
                        quit();
                    }
                }
                _ => {}
            }
        }
    }
}

/// Animation frames at an even 60 fps. WM_TIMER can't do it: its messages
/// have the lowest priority, get coalesced and follow Windows' ~15.6 ms
/// tick, so a 16 ms timer gave 40 fps with uneven gaps. A small thread
/// sleeps to each frame's due time (1 ms resolution while it runs) and posts
/// one message; it never queues a second one before the UI took the first,
/// so a slow frame drops the next instead of piling them up.
mod pacer {
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

    pub const MSG_FRAME: u32 = WM_APP + 7;
    const PERIOD: Duration = Duration::from_micros(16_667);

    static RUN: Mutex<bool> = Mutex::new(false);
    static WAKE: Condvar = Condvar::new();
    static POSTED: AtomicBool = AtomicBool::new(false);
    static HWND: AtomicIsize = AtomicIsize::new(0);

    pub fn start(hwnd: windows_sys::Win32::Foundation::HWND) {
        HWND.store(hwnd as isize, Ordering::Relaxed);
        std::thread::spawn(|| loop {
            {
                let mut on = RUN.lock().unwrap();
                while !*on {
                    on = WAKE.wait(on).unwrap();
                }
            }
            unsafe { windows_sys::Win32::Media::timeBeginPeriod(1) };
            let mut next = Instant::now();
            while *RUN.lock().unwrap() {
                next += PERIOD;
                let now = Instant::now();
                if next > now {
                    std::thread::sleep(next - now);
                } else {
                    next = now; // fell behind: don't try to catch up
                }
                if !POSTED.swap(true, Ordering::AcqRel) {
                    unsafe { PostMessageW(HWND.load(Ordering::Relaxed) as _, MSG_FRAME, 0, 0) };
                }
            }
            unsafe { windows_sys::Win32::Media::timeEndPeriod(1) };
        });
    }

    pub fn run(on: bool) {
        *RUN.lock().unwrap() = on;
        if on {
            WAKE.notify_one();
        }
    }

    /// The UI picked up the frame message: the next one may be posted.
    pub fn taken() {
        POSTED.store(false, Ordering::Release);
    }
}

/// `CLAUDETALK_PERF=1`: every 2 s the log gets how long frames and glass
/// refreshes took (count, average, worst), to find what makes animations
/// stutter. Off by default: costs one env check per process.
mod perf {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::time::{Duration, Instant};

    thread_local! {
        static ON: bool = std::env::var_os("CLAUDETALK_PERF").is_some();
        static STATS: RefCell<(Instant, BTreeMap<&'static str, (u32, Duration, Duration)>)> = RefCell::new((Instant::now(), BTreeMap::new()));
    }

    pub struct Timer(Option<(&'static str, Instant)>);

    impl Timer {
        pub fn new(name: &'static str) -> Self {
            Timer(ON.with(|on| *on).then(|| (name, Instant::now())))
        }
    }

    impl Drop for Timer {
        fn drop(&mut self) {
            let Some((name, t0)) = self.0 else { return };
            let took = t0.elapsed();
            STATS.with(|st| {
                let mut st = st.borrow_mut();
                let e = st.1.entry(name).or_insert((0, Duration::ZERO, Duration::ZERO));
                e.0 += 1;
                e.1 += took;
                e.2 = e.2.max(took);
                if st.0.elapsed() >= Duration::from_secs(2) {
                    let line: Vec<String> = st
                        .1
                        .iter()
                        .map(|(k, (n, sum, max))| format!("{k} {n}x avg {:.1} ms max {:.1} ms", sum.as_secs_f64() * 1000.0 / *n as f64, max.as_secs_f64() * 1000.0))
                        .collect();
                    println!("[perf] {}", line.join(" | "));
                    st.1.clear();
                    st.0 = Instant::now();
                }
            });
        }
    }
}

/// Turning dictation off from the tray or the panel: also forget the
/// "launched by hand" flag, so a later --auto instance can exit (v0.5's
/// panel button forgot to).
fn quit() {
    let _ = std::fs::remove_file(ct_core::paths::state_dir().join("persistent.flag"));
    unsafe { PostQuitMessage(0) };
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let id = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    match msg {
        WM_MOUSEACTIVATE if id != ID_EDITOR => return MA_NOACTIVATE as LRESULT,
        WM_SETCURSOR if id == ID_PILL || id == ID_PANEL || id == ID_VOICE || id == ID_TALK => {
            if let Some(c) = with_app(|a| a.cursor) {
                SetCursor(LoadCursorW(std::ptr::null_mut(), c));
                return 1;
            }
        }
        WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSELEAVE_ if (ID_PILL..=ID_VOICE).contains(&id) || id == ID_TALK => {
            with_app(|a| a.mouse(id, msg));
            return 0;
        }
        _ => {}
    }
    if id == ID_EDITOR {
        if msg == editor::MSG_COMMIT || msg == editor::MSG_CANCEL {
            with_app(|a| {
                if let Some((k, v)) = a.editor.finish(msg == editor::MSG_COMMIT) {
                    a.commit_text(&k, &v);
                }
            });
            return 0;
        }
        if let Some(Some(r)) = with_app(|a| a.editor.handle(msg, wp, lp)) {
            return r;
        }
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    if id == ID_MAIN {
        match msg {
            WM_INPUT => {
                let fired = with_app(|a| a.chord.update()).unwrap_or(false);
                if fired {
                    with_app(|a| a.shared.on_press());
                }
            }
            WM_TIMER => {
                with_app(|a| a.timer(wp));
                return 0;
            }
            pacer::MSG_FRAME => {
                pacer::taken();
                with_app(|a| {
                    if a.frame_on {
                        a.frame();
                    }
                });
                return 0;
            }
            MSG_EVENT => {
                let ev = *Box::from_raw(lp as *mut UiEvent);
                PENDING.with(|p| p.borrow_mut().push(ev));
                with_app(|a| {
                    for ev in PENDING.with(|p| std::mem::take(&mut *p.borrow_mut())) {
                        a.event(ev);
                    }
                });
                return 0;
            }
            tray::MSG_TRAY => {
                with_app(|a| a.tray_event(lp));
                return 0;
            }
            _ => {
                if with_app(|a| a.taskbar_created == msg).unwrap_or(false) {
                    with_app(|a| a.tray.readd());
                    return 0;
                }
            }
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// Runs the UI until "Turn off" or the --auto watchdog quits.
pub fn run(shared: Arc<Shared>, auto: bool) {
    unsafe {
        let class = wide("ClaudeTalkDictation");
        let hinst = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
        };
        RegisterClassW(&wc);
        let main = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("claudeTalk dictation").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        SetWindowLongPtrW(main, GWLP_USERDATA, ID_MAIN);
        pacer::start(main);
        shared.set_ui(main);
        if !hotkey::register(main) {
            println!("[hotkey] raw input registration failed");
        }
        let keys = shared.config.lock().unwrap().hotkey();
        let mut tray = Tray::new(main);
        tray.label = hotkey_label(&keys);
        tray.show();
        let app = App {
            shared: shared.clone(),
            main,
            pill: Pill::new(Layered::create(&class, ID_PILL)),
            talk: TalkPill::new(Layered::create(&class, ID_TALK)),
            panel: Panel::new(Layered::create(&class, ID_PANEL), Kind::Dictation),
            voice: Panel::new(Layered::create(&class, ID_VOICE), Kind::Voice),
            editor: Editor::create(&class, ID_EDITOR),
            tray,
            chord: Chord::new(keys),
            watchdog: auto.then(Watchdog::new),
            badge_token: 0,
            live_turn: 0,
            anchor: None,
            glide: 0,
            stranded: 0,
            frame_on: false,
            tracking_mouse: [false; 6],
            cursor: IDC_ARROW,
            taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        };
        APP.with(|a| *a.borrow_mut() = Some(app));
        SetTimer(main, T_TRACK, 500, None);
        SetTimer(main, T_CONFIG, 1000, None);
        SetTimer(main, T_SPEAK, 150, None);
        if auto {
            SetTimer(main, T_WATCHDOG, 5000, None);
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().take() {
                app.tray.remove();
            }
        });
    }
}

/// `--render-test DIR`: paints the pill in each phase and both panels over
/// a synthetic backdrop into PNGs, to check the drawing without a screen
/// (the real windows are invisible to screen capture).
pub fn render_test(dir: &std::path::Path, cfg: &crate::config::Config, scale: f32) {
    use pill::Phase;
    let _ = std::fs::create_dir_all(dir);
    let class = wide("ClaudeTalkRenderTest");
    unsafe {
        let wc = WNDCLASSW { lpfnWndProc: Some(DefWindowProcW), lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
    }
    let m = Material { glass: (cfg.f64("glass", 60.0) / 100.0) as f32 };
    // backdrop: diagonal color bands with some text-like stripes
    let backdrop = |w: u32, h: u32| {
        let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
        for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
            let t = (x + y) / (w + h) as f32;
            let stripe = if (y as u32 / 6) % 3 == 0 && (x as u32 / 40) % 2 == 0 { 60.0 } else { 0.0 };
            px[0] = (30.0 + 200.0 * t + stripe).min(255.0) as u8;
            px[1] = (90.0 + 80.0 * (1.0 - t) + stripe).min(255.0) as u8;
            px[2] = (200.0 - 150.0 * t + stripe).min(255.0) as u8;
            px[3] = 255;
        }
        pm
    };
    let mut p = Pill::new(Layered::create(&class, ID_PILL));
    p.scale = scale;
    let (w, h) = (((pill::WIDTH + 2.0 * INSET) * scale).ceil() as u32, ((pill::HEIGHT + 14.0 + 2.0 * INSET) * scale).ceil() as u32);
    p.test_backdrop(&backdrop(w, h), m);
    let shots = [
        ("pill-record", Phase::Record, 0.0, [0.2, 0.5, 0.9, 0.6, 0.3], true),
        ("pill-busy", Phase::Busy, 0.0, [0.1, 0.3, 0.2, 0.1, 0.08], true),
        ("pill-done-mid", Phase::Done, 170.0, [0.2, 0.3, 0.4, 0.3, 0.2], true),
        ("pill-done-ok", Phase::Done, 1000.0, [0.0; 5], true),
        ("pill-done-clipboard", Phase::Done, 1000.0, [0.0; 5], false),
    ];
    for (name, phase, ms, levels, ok) in shots {
        p.test_state(phase, ms, levels, ok);
        let _ = p.render(m, false).save_png(dir.join(format!("{name}.png")));
    }
    let mut t = TalkPill::new(Layered::create(&class, ID_TALK));
    t.scale = scale;
    let (w, h) = t.phys_size();
    for (name, paused, queued, pop, other) in [("talk", false, 0, false, false), ("talk-queue-paused", true, 2, false, false), ("talk-volume", false, 3, true, true)] {
        t.test_state(&backdrop(w, h), m, paused, queued, pop, other);
        let _ = t.render(m).save_png(dir.join(format!("{name}.png")));
    }
    for (kind, name) in [(Kind::Dictation, "panel-dictation"), (Kind::Voice, "panel-voice")] {
        let mut pn = Panel::new(Layered::create(&class, ID_PANEL), kind);
        pn.scale = scale;
        let (w, h) = pn.phys_size();
        pn.test_backdrop(&backdrop(w, h), m, cfg);
        let _ = pn.render(cfg, m).save_png(dir.join(format!("{name}.png")));
        if kind == Kind::Dictation {
            pn.test_confirming();
            let _ = pn.render(cfg, m).save_png(dir.join("panel-confirm.png"));
        }
    }
    let _ = tray::paint_icon(64).save_png(dir.join("icon-64.png"));
}
