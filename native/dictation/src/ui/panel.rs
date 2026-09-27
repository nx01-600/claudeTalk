//! The settings panels (overlay.py: SettingsPanel, VoicePanel): grouped
//! rows with hand-painted controls (switch, segmented control, slider, key
//! capture, text chip) that never take the focus. "Dictation" opens from
//! the gear; "Claude's voice" opens next to it.

use super::gfx::{self, draw_text, rounded, text_width, white, HAlign, TextStyle, VAlign, R};
use super::glass::{self, Material, INSET};
use super::pill::Tween;
use super::window::Layered;
use crate::config::{hotkey_label, Config};
use crate::hotkey;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tiny_skia::{Pixmap, Transform};

pub const PANEL_W: f32 = 328.0;
const PANEL_RADIUS: f32 = 20.0;
const PAD: f32 = 16.0;
const TITLE_H: f32 = 44.0;
const GROUP_TITLE_H: f32 = 28.0;
const ROW_H: f32 = 42.0;
const GROUP_GAP: f32 = 10.0;
const CONTROL_RIGHT: f32 = PANEL_W - PAD - 12.0;
const TOGGLE_W: f32 = 40.0;
const TOGGLE_H: f32 = 24.0;
const SEG_H: f32 = 26.0;
const SEG_MIN_W: f32 = 58.0;
const SLIDER_W: f32 = 132.0;
const SLIDER_KNOB_R: f32 = 8.0;
const SLIDER_VALUE_W: f32 = 44.0;
const TEXT_MAX_W: f32 = 150.0;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Dictation,
    Voice,
}

#[derive(Clone)]
pub struct Range {
    low: f64,
    high: f64,
    step: f64,
    fmt: Option<fn(f64) -> String>,
}

const PERCENT: Range = Range { low: 0.0, high: 100.0, step: 1.0, fmt: None };

/// A switch that isn't a stored boolean: on while the dictation detects the
/// language by itself (`language` = "auto").
const AUTO_LANGUAGE: &str = "\u{1}auto_language";

/// The panel's English text in claudeTalk's language.
pub fn tr(english: &str) -> String {
    ct_core::lang::cached().tr(english)
}

fn switch_on(key: &str, cfg: &Config) -> bool {
    if key == AUTO_LANGUAGE {
        cfg.str("language") == "auto"
    } else {
        cfg.bool(key)
    }
}

fn silence_fmt(v: f64) -> String {
    // Python's f"{v / 1000:g} s"
    let s = format!("{}", v / 1000.0);
    format!("{s} s")
}

#[derive(Clone)]
pub enum Row {
    Group(&'static str),
    Hotkey(&'static str),
    Slider(&'static str, &'static str, Range),
    Toggle(&'static str, &'static str),
    Segment(&'static str, &'static str, Vec<(String, String)>),
    Text(&'static str, &'static str),
    Danger,
}

fn opts(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect()
}

fn rows(kind: Kind) -> Vec<Row> {
    use Row::*;
    let pack = ct_core::lang::cached();
    let languages: Vec<(String, String)> = ct_core::lang::available();
    // Three voices fit in one row: more are split evenly over extra rows
    // that edit the same setting; only the row holding the current voice
    // shows a chip.
    let per_row = pack.voices.len().div_ceil(pack.voices.len().div_ceil(3)).max(1);
    let voice_rows: Vec<Row> = pack
        .voices
        .chunks(per_row)
        .enumerate()
        .map(|(i, chunk)| Segment(if i == 0 { "Voice" } else { "" }, "tts_voice", chunk.iter().map(|v| (v.id.clone(), v.name.clone())).collect()))
        .collect();
    match kind {
        Kind::Dictation => vec![
            Group("Activation"),
            Hotkey("Keys"),
            Slider("Silence cutoff", "silence_ms", Range { low: 500.0, high: 10000.0, step: 250.0, fmt: Some(silence_fmt) }),
            Slider("Mic sensitivity", "sensitivity", PERCENT),
            Toggle("Sound on start", "sound"),
            Toggle("Send with Enter", "auto_enter"),
            Group("Appearance"),
            Slider("Glass", "glass", PERCENT),
            Segment("Position", "position", opts(&[("bottom", "Bottom"), ("top", "Top")])),
            Toggle("Remember dragged spot", "remember_drag"),
            Toggle("Show in screen share", "show_in_capture"),
            Group("Transcription"),
            Segment("Language", "lang", languages),
            Toggle("Detect language automatically", AUTO_LANGUAGE),
            Group(""),
            Danger,
        ],
        Kind::Voice => {
            let mut v = vec![Group("Talk mode")];
            v.extend(voice_rows);
            v.extend([
                Segment("Speed", "tts_rate", opts(&[("-15%", "Slow"), ("+0%", "Normal"), ("+20%", "Fast"), ("+40%", "Faster")])),
                Slider("Volume", "tts_volume", PERCENT),
                Toggle("\u{0}wake", "wake_word"), // label built from the phrase
                Text("Wake phrase", "wake_phrase"),
                Toggle("Speak only when I talk", "speak_only_spoken"),
                Toggle("Show when Claude talks", "speaking_indicator"),
            ]);
            v
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    Control,
    Seg(usize),
    Confirm,
    Cancel,
    Close,
}

/// What the app has to do after a panel event.
pub enum Action {
    Set(String, Value),
    CaptureStarted,
    CaptureFinished,
    Quit,
    Close,
    EditText { key: String, value: String, x: i32, y: i32, w: i32, h: i32 },
    Preview(&'static str),
}

enum Item {
    GroupTitle(usize, f32),
    GroupEnd,
    Row(usize, R),
}

pub struct Panel {
    pub win: Layered,
    pub kind: Kind,
    rows: Vec<Row>,
    pub scale: f32,
    hover: Option<(Option<usize>, Option<Part>)>,
    dragging_slider: Option<usize>,
    pub opens_down: bool,
    confirming_quit: bool,
    pub capturing: bool,
    capture_acc: Vec<u16>,
    capture_live: Vec<u16>,
    capture_had: bool,
    capture_deadline: Instant,
    toggle_t: HashMap<&'static str, f32>,
    phase: f32,
    pub anchor_y: i32,
    pub closing: bool,
    anim: Option<Tween>,
    bg_raw: Option<Pixmap>,
    bg: Option<Pixmap>,
    bg_print: u64,
    bg_glass: i32,
}

impl Panel {
    pub fn new(win: Layered, kind: Kind) -> Self {
        Self {
            win,
            kind,
            rows: rows(kind),
            scale: 1.0,
            hover: None,
            dragging_slider: None,
            opens_down: false,
            confirming_quit: false,
            capturing: false,
            capture_acc: Vec::new(),
            capture_live: Vec::new(),
            capture_had: false,
            capture_deadline: Instant::now(),
            toggle_t: HashMap::new(),
            phase: 0.0,
            anchor_y: 0,
            closing: false,
            anim: None,
            bg_raw: None,
            bg: None,
            bg_print: 0,
            bg_glass: -1,
        }
    }

    /// Rebuilds the rows after claudeTalk's language changed (other voices,
    /// other languages installed). The texts are translated at paint time.
    pub fn relabel(&mut self, cfg: &Config) {
        self.rows = rows(self.kind);
        for row in &self.rows {
            if let Row::Toggle(_, key) = row {
                self.toggle_t.insert(key, if switch_on(key, cfg) { 1.0 } else { 0.0 });
            }
        }
    }

    fn items(&self) -> (Vec<Item>, f32) {
        let mut y = PAD + TITLE_H;
        let mut out = Vec::new();
        let mut in_group = false;
        for (i, row) in self.rows.iter().enumerate() {
            if let Row::Group(label) = row {
                if in_group {
                    out.push(Item::GroupEnd);
                }
                y += if label.is_empty() { GROUP_GAP } else { GROUP_TITLE_H };
                out.push(Item::GroupTitle(i, y));
                in_group = true;
                continue;
            }
            out.push(Item::Row(i, R::new(PAD, y, PANEL_W - 2.0 * PAD, ROW_H)));
            y += ROW_H;
        }
        out.push(Item::GroupEnd);
        (out, y + PAD)
    }

    pub fn content_height(&self) -> f32 {
        self.items().1
    }

    /// Window size in physical px.
    pub fn phys_size(&self) -> (u32, u32) {
        (((PANEL_W + 2.0 * INSET) * self.scale).ceil() as u32, ((self.content_height() + 2.0 * INSET) * self.scale).ceil() as u32)
    }

    fn rect_of(&self, index: usize) -> Option<R> {
        self.items().0.into_iter().find_map(|it| match it {
            Item::Row(i, r) if i == index => Some(r),
            _ => None,
        })
    }

    pub fn visible(&self) -> bool {
        self.win.visible && !self.closing
    }

    // --- open / close ---------------------------------------------------------

    pub fn open_at(&mut self, x: i32, y: i32, cfg: &Config, m: Material) {
        self.confirming_quit = false;
        self.hover = None;
        self.closing = false;
        self.anchor_y = y;
        self.win.x = x;
        self.win.y = y;
        self.win.set_capturable(cfg.bool("show_in_capture"));
        self.bg_raw = None;
        self.bg = None;
        self.refresh_background(m, cfg.bool("show_in_capture"));
        self.win.opacity = 0.0;
        self.win.y = y + (8.0 * self.scale) as i32;
        self.anim = Some(Tween::new(0.0, 1.0, 180.0, gfx::out_cubic));
        for row in &self.rows {
            if let Row::Toggle(_, key) = row {
                self.toggle_t.insert(key, if switch_on(key, cfg) { 1.0 } else { 0.0 });
            }
        }
    }

    pub fn close(&mut self) -> Vec<Action> {
        let mut acts = Vec::new();
        if !self.win.visible || self.closing {
            return acts;
        }
        if self.capturing {
            self.capturing = false;
            acts.push(Action::CaptureFinished);
        }
        self.closing = true;
        self.anim = Some(Tween::new(self.win.opacity, 0.0, 130.0, gfx::in_cubic));
        acts
    }

    /// One frame. Returns false when the panel finished closing.
    pub fn tick(&mut self, cfg: &Config) -> bool {
        self.phase += 0.12;
        for row in &self.rows {
            if let Row::Toggle(_, key) = row {
                let target = if switch_on(key, cfg) { 1.0 } else { 0.0 };
                let cur = *self.toggle_t.get(key).unwrap_or(&target);
                self.toggle_t.insert(key, gfx::lerp(cur, target, 0.3));
            }
        }
        if let Some(a) = &self.anim {
            let (v, done) = a.value();
            self.win.opacity = v;
            self.win.y = self.anchor_y + (8.0 * (1.0 - v) * self.scale).round() as i32;
            if done {
                self.anim = None;
                if self.closing {
                    self.closing = false;
                    self.win.hide();
                    return false;
                }
            }
        }
        true
    }

    /// True while something moves (open/close, toggles, capture pulse).
    pub fn animating(&self, cfg: &Config) -> bool {
        self.anim.is_some()
            || self.capturing
            || self.rows.iter().any(|r| match r {
                Row::Toggle(_, key) => {
                    let target = if switch_on(key, cfg) { 1.0 } else { 0.0 };
                    (self.toggle_t.get(key).unwrap_or(&target) - target).abs() > 0.004
                }
                _ => false,
            })
    }

    pub fn refresh_background(&mut self, m: Material, capturable: bool) -> bool {
        let (w, h) = self.phys_size();
        if self.bg_raw.is_none() || !(capturable && self.win.visible) {
            self.bg_raw = glass::capture(self.win.x, self.win.y, w, h);
        }
        let Some(raw) = &self.bg_raw else { return false };
        let print = glass::fingerprint(raw);
        let g = (m.glass * 100.0).round() as i32;
        if self.bg.is_some() && print == self.bg_print && g == self.bg_glass {
            return false;
        }
        self.bg_print = print;
        self.bg_glass = g;
        self.bg = Some(glass::glassify(raw, m));
        true
    }

    /// For --render-test: a fixed backdrop instead of a screen capture.
    pub fn test_backdrop(&mut self, raw: &Pixmap, m: Material, cfg: &Config) {
        self.bg = Some(glass::glassify(raw, m));
        for row in &self.rows {
            if let Row::Toggle(_, key) = row {
                self.toggle_t.insert(key, if switch_on(key, cfg) { 1.0 } else { 0.0 });
            }
        }
    }

    pub fn test_confirming(&mut self) {
        self.confirming_quit = true;
    }

    // --- key capture ------------------------------------------------------------

    fn begin_capture(&mut self) -> Action {
        self.capturing = true;
        self.capture_acc.clear();
        self.capture_live.clear();
        self.capture_had = false;
        self.capture_deadline = Instant::now() + CAPTURE_TIMEOUT;
        Action::CaptureStarted
    }

    /// Every 30 ms while capturing: collects the keys held, commits when
    /// all are released, Esc alone cancels, 8 s timeout.
    pub fn capture_tick(&mut self) -> Vec<Action> {
        if !self.capturing {
            return Vec::new();
        }
        let current = hotkey::pressed_keys();
        let mut acts = Vec::new();
        if current == [hotkey::VK_ESCAPE] {
            self.capturing = false;
            acts.push(Action::CaptureFinished);
            return acts;
        }
        if !current.is_empty() {
            for k in &current {
                if !self.capture_acc.contains(k) {
                    self.capture_acc.push(*k);
                }
            }
            self.capture_had = true;
            self.capture_live = hotkey::chord_sorted(&current);
        } else if self.capture_had {
            self.capturing = false;
            if !self.capture_acc.is_empty() {
                let keys = hotkey::chord_sorted(&self.capture_acc);
                acts.push(Action::Set("hotkey".into(), json!(keys)));
            }
            acts.push(Action::CaptureFinished);
            return acts;
        }
        if Instant::now() > self.capture_deadline {
            self.capturing = false;
            acts.push(Action::CaptureFinished);
        }
        acts
    }

    // --- geometry ---------------------------------------------------------------

    fn hotkey_text(&self, cfg: &Config) -> String {
        if self.capturing {
            if self.capture_live.is_empty() {
                tr("Press the keys")
            } else {
                hotkey_label(&self.capture_live)
            }
        } else {
            hotkey_label(&cfg.hotkey())
        }
    }

    fn control_rect(&self, index: usize, rect: R, cfg: &Config) -> R {
        let cy = rect.cy();
        match &self.rows[index] {
            Row::Toggle(..) => R::new(CONTROL_RIGHT - TOGGLE_W, cy - TOGGLE_H / 2.0, TOGGLE_W, TOGGLE_H),
            Row::Segment(_, _, opts) => {
                let w = SEG_MIN_W * opts.len() as f32;
                R::new(CONTROL_RIGHT - w, cy - SEG_H / 2.0, w, SEG_H)
            }
            Row::Slider(..) => R::new(CONTROL_RIGHT - SLIDER_W, cy - 10.0, SLIDER_W, 20.0),
            Row::Hotkey(_) => {
                let w = text_width(&self.hotkey_text(cfg), &TextStyle::new(9.0)) + 24.0;
                R::new(CONTROL_RIGHT - w, cy - 13.0, w, 26.0)
            }
            Row::Text(_, key) => {
                let w = (text_width(&cfg.str(key), &TextStyle::new(9.0)) + 24.0).max(90.0).min(TEXT_MAX_W);
                R::new(CONTROL_RIGHT - w, cy - 13.0, w, 26.0)
            }
            _ => rect,
        }
    }

    fn confirm_rects(rect: R) -> (R, R) {
        let w = 76.0;
        let confirm = R::new(rect.right() - 12.0 - w, rect.cy() - 14.0, w, 28.0);
        let cancel = R::new(confirm.x - 8.0 - w, rect.cy() - 14.0, w, 28.0);
        (confirm, cancel)
    }

    fn hit(&self, x: f32, y: f32, cfg: &Config) -> (Option<usize>, Option<Part>) {
        if R::new(PANEL_W - PAD - 28.0, PAD + 8.0, 28.0, 28.0).contains(x, y) {
            return (None, Some(Part::Close));
        }
        for item in self.items().0 {
            let Item::Row(index, rect) = item else { continue };
            if !rect.contains(x, y) {
                continue;
            }
            let control = self.control_rect(index, rect, cfg);
            return match &self.rows[index] {
                Row::Danger => {
                    if self.confirming_quit {
                        let (confirm, cancel) = Self::confirm_rects(rect);
                        if confirm.contains(x, y) {
                            (Some(index), Some(Part::Confirm))
                        } else if cancel.contains(x, y) {
                            (Some(index), Some(Part::Cancel))
                        } else {
                            (Some(index), None)
                        }
                    } else {
                        (Some(index), Some(Part::Control))
                    }
                }
                Row::Segment(_, _, opts) => {
                    if control.contains(x, y) {
                        let n = ((x - control.x) / (control.w / opts.len() as f32)).floor() as isize;
                        (Some(index), Some(Part::Seg(n.clamp(0, opts.len() as isize - 1) as usize)))
                    } else {
                        (Some(index), None)
                    }
                }
                Row::Slider(..) => {
                    let c = control.adjusted(-8.0, -6.0, 8.0, 6.0);
                    (Some(index), c.contains(x, y).then_some(Part::Control))
                }
                Row::Toggle(..) | Row::Hotkey(_) | Row::Text(..) => {
                    let c = control.adjusted(-4.0, -4.0, 4.0, 4.0);
                    (Some(index), c.contains(x, y).then_some(Part::Control))
                }
                Row::Group(_) => (Some(index), None),
            };
        }
        (None, None)
    }

    fn local(&self, sx: i32, sy: i32) -> (f32, f32) {
        ((sx - self.win.x) as f32 / self.scale - INSET, (sy - self.win.y) as f32 / self.scale - INSET)
    }

    // --- mouse --------------------------------------------------------------------

    /// Returns (actions, pointer cursor?).
    pub fn mouse_move(&mut self, sx: i32, sy: i32, cfg: &Config) -> (Vec<Action>, bool) {
        let (x, y) = self.local(sx, sy);
        if self.dragging_slider.is_some() {
            return (self.apply_slider(x, cfg), true);
        }
        let hit = self.hit(x, y, cfg);
        self.hover = Some(hit);
        (Vec::new(), hit.1.is_some())
    }

    pub fn mouse_leave(&mut self) {
        self.hover = None;
    }

    pub fn mouse_down(&mut self, sx: i32, sy: i32, cfg: &Config) -> Vec<Action> {
        let (x, y) = self.local(sx, sy);
        let (index, part) = self.hit(x, y, cfg);
        if part == Some(Part::Close) {
            return vec![Action::Close];
        }
        let (Some(index), Some(part)) = (index, part) else { return Vec::new() };
        match self.rows[index].clone() {
            Row::Toggle(_, key) if key == AUTO_LANGUAGE => {
                let v = if switch_on(key, cfg) { cfg.str("lang") } else { "auto".into() };
                let v = if v.is_empty() { ct_core::lang::current_code() } else { v };
                vec![Action::Set("language".into(), json!(v))]
            }
            Row::Toggle(_, key) => vec![Action::Set(key.into(), json!(!switch_on(key, cfg)))],
            Row::Segment(_, key, opts) => match part {
                Part::Seg(n) => vec![Action::Set(key.into(), json!(opts[n].0))],
                _ => Vec::new(),
            },
            Row::Slider(..) => {
                self.dragging_slider = Some(index);
                self.apply_slider(x, cfg)
            }
            Row::Hotkey(_) => {
                if self.capturing {
                    Vec::new()
                } else {
                    vec![self.begin_capture()]
                }
            }
            Row::Text(_, key) => {
                let rect = self.rect_of(index).unwrap();
                let chip = self.control_rect(index, rect, cfg);
                let width = chip.w.max(200.0);
                let s = self.scale;
                vec![Action::EditText {
                    key: key.into(),
                    value: cfg.str(key),
                    x: self.win.x + ((INSET + chip.right() - width) * s).round() as i32,
                    y: self.win.y + ((INSET + chip.y) * s).round() as i32,
                    w: (width * s).round() as i32,
                    h: (chip.h * s).round() as i32,
                }]
            }
            Row::Danger => match part {
                Part::Control => {
                    self.confirming_quit = true;
                    Vec::new()
                }
                Part::Cancel => {
                    self.confirming_quit = false;
                    Vec::new()
                }
                Part::Confirm => vec![Action::Quit],
                _ => Vec::new(),
            },
            Row::Group(_) => Vec::new(),
        }
    }

    pub fn mouse_up(&mut self) -> Vec<Action> {
        let dragged = self.dragging_slider.take();
        // The volume slider saves on every step: play the sample once, on release.
        if let Some(i) = dragged {
            if let Row::Slider(_, "tts_volume", _) = self.rows[i] {
                return vec![Action::Preview("volume")];
            }
        }
        Vec::new()
    }

    fn apply_slider(&mut self, x: f32, cfg: &Config) -> Vec<Action> {
        let Some(i) = self.dragging_slider else { return Vec::new() };
        let Row::Slider(_, key, rng) = self.rows[i].clone() else { return Vec::new() };
        let control = self.control_rect(i, self.rect_of(i).unwrap(), cfg);
        let t = ((x - control.x - SLIDER_KNOB_R) / (control.w - 2.0 * SLIDER_KNOB_R)).clamp(0.0, 1.0) as f64;
        let raw = rng.low + t * (rng.high - rng.low);
        let value = ((raw / rng.step).round_ties_even() * rng.step) as i64;
        if cfg.get(key).as_f64() != Some(value as f64) {
            vec![Action::Set(key.into(), json!(value))]
        } else {
            Vec::new()
        }
    }

    // --- painting -------------------------------------------------------------------

    pub fn render(&self, cfg: &Config, m: Material) -> Pixmap {
        let s = self.scale;
        let (w, h) = self.phys_size();
        let mut pm = Pixmap::new(w, h).unwrap();
        let rect = R::new(INSET, INSET, PANEL_W, self.content_height());
        glass::paint_glass(&mut pm, rect, PANEL_RADIUS, self.bg.as_ref(), m, None, s);
        // from here on, relative to the glass
        let ts = Transform::from_scale(s, s).pre_translate(INSET, INSET);
        let (ox, oy) = (INSET * s, INSET * s);
        let title = tr(if self.kind == Kind::Dictation { "Dictation" } else { "Claude's voice" });
        draw_text(&mut pm, &title, R::new(PAD + 4.0, PAD, 200.0, TITLE_H - 8.0), &TextStyle::bold(11.0), text(true), HAlign::Left, VAlign::Center, s, ox, oy);
        self.paint_close(&mut pm, ts);

        let (items, _) = self.items();
        for item in &items {
            if let Item::GroupTitle(index, y) = item {
                if let Row::Group(label) = &self.rows[*index] {
                    if !label.is_empty() {
                        draw_text(
                            &mut pm,
                            &tr(label).to_uppercase(),
                            R::new(PAD + 4.0, y - GROUP_TITLE_H, 200.0, GROUP_TITLE_H - 4.0),
                            &TextStyle::new(8.0),
                            text(false),
                            HAlign::Left,
                            VAlign::Bottom,
                            s,
                            ox,
                            oy,
                        );
                    }
                }
            }
        }
        let mut previous_in_group = false;
        for item in &items {
            match item {
                Item::GroupTitle(..) => previous_in_group = false,
                Item::GroupEnd => {}
                Item::Row(index, r) => {
                    if previous_in_group {
                        gfx::line(&mut pm, r.x + 12.0, r.y, r.right() - 12.0, r.y, white(22), 1.0, false, ts);
                    }
                    previous_in_group = true;
                    self.paint_row(&mut pm, *index, *r, cfg, ts, s, ox, oy);
                }
            }
        }
        pm
    }

    fn paint_close(&self, pm: &mut Pixmap, ts: Transform) {
        let r = R::new(PANEL_W - PAD - 28.0, PAD + 8.0, 28.0, 28.0);
        let hovered = self.hover == Some((None, Some(Part::Close)));
        gfx::fill(pm, &gfx::circle(r.cx(), r.cy(), 14.0), white(if hovered { 40 } else { 16 }), ts);
        let (cx, cy) = (r.cx(), r.cy());
        gfx::line(pm, cx - 4.5, cy - 4.5, cx + 4.5, cy + 4.5, white(200), 1.6, true, ts);
        gfx::line(pm, cx - 4.5, cy + 4.5, cx + 4.5, cy - 4.5, white(200), 1.6, true, ts);
    }

    fn row_hovered(&self, index: usize) -> bool {
        matches!(self.hover, Some((Some(i), Some(_))) if i == index)
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_row(&self, pm: &mut Pixmap, index: usize, rect: R, cfg: &Config, ts: Transform, s: f32, ox: f32, oy: f32) {
        let row = &self.rows[index];
        let st = TextStyle::new(9.5);
        if let Row::Danger = row {
            self.paint_danger(pm, index, rect, ts, s, ox, oy);
            return;
        }
        let label = match row {
            Row::Hotkey(l) | Row::Slider(l, ..) | Row::Segment(l, ..) | Row::Text(l, _) => tr(l),
            Row::Toggle(l, _) if l.starts_with('\u{0}') => tr("Start with \u{201c}{phrase}\u{201d}").replace("{phrase}", &cfg.str("wake_phrase")),
            Row::Toggle(l, _) => tr(l),
            _ => String::new(),
        };
        let control = self.control_rect(index, rect, cfg);
        // Translations run longer than the English: shrink a label that
        // would reach its control (a slider's value sits left of it).
        let value_w = if matches!(row, Row::Slider(_, _, Range { fmt: Some(_), .. })) { SLIDER_VALUE_W + 4.0 } else { 0.0 };
        let room = control.x - value_w - 8.0 - (rect.x + 14.0);
        let mut label_st = TextStyle::new(st.pt);
        let wide = text_width(&label, &label_st);
        if wide > room && room > 0.0 {
            label_st.pt = (st.pt * room / wide).max(7.0);
        }
        draw_text(pm, &label, rect.adjusted(14.0, 0.0, -14.0, 0.0), &label_st, text(true), HAlign::Left, VAlign::Center, s, ox, oy);
        let hovered = self.row_hovered(index);
        match row {
            Row::Toggle(_, key) => {
                let t = *self.toggle_t.get(key).unwrap_or(&if switch_on(key, cfg) { 1.0 } else { 0.0 });
                let off = if hovered { 60.0 } else { 45.0 };
                gfx::fill(pm, &rounded(control, TOGGLE_H / 2.0), white(gfx::lerp(off, 230.0, t) as u8), ts);
                let kr = TOGGLE_H / 2.0 - 3.0;
                let kx = gfx::lerp(control.x + 3.0 + kr, control.right() - 3.0 - kr, t);
                let knob = gfx::circle(kx, control.cy(), kr);
                gfx::fill(pm, &knob, white(255), ts);
                gfx::stroke(pm, &knob, gfx::black(60), 1.0, false, ts);
            }
            Row::Segment(_, key, opts) => {
                gfx::fill(pm, &rounded(control, SEG_H / 2.0), white(18), ts);
                let seg_w = control.w / opts.len() as f32;
                let current = cfg.str(key);
                let hover_n = match self.hover {
                    Some((Some(i), Some(Part::Seg(n)))) if i == index => Some(n),
                    _ => None,
                };
                for (i, (value, label)) in opts.iter().enumerate() {
                    let seg = R::new(control.x + i as f32 * seg_w, control.y, seg_w, control.h);
                    let selected = *value == current;
                    let chip = rounded(seg.adjusted(2.0, 2.0, -2.0, -2.0), SEG_H / 2.0 - 2.0);
                    if selected {
                        gfx::fill(pm, &chip, white(60), ts);
                        gfx::stroke(pm, &chip, white(28), 1.0, false, ts);
                    } else if hover_n == Some(i) {
                        gfx::fill(pm, &chip, white(14), ts);
                    }
                    draw_text(pm, &tr(label), seg, &TextStyle::new(8.5), text(selected), HAlign::Center, VAlign::Center, s, ox, oy);
                }
            }
            Row::Slider(_, key, rng) => {
                let current = cfg.f64(key, rng.low);
                let value = ((current - rng.low) / (rng.high - rng.low)).clamp(0.0, 1.0) as f32;
                if let Some(fmt) = rng.fmt {
                    draw_text(
                        pm,
                        &fmt(current),
                        R::new(control.x - SLIDER_VALUE_W - 4.0, control.y, SLIDER_VALUE_W, control.h),
                        &TextStyle::new(8.5),
                        text(false),
                        HAlign::Right,
                        VAlign::Center,
                        s,
                        ox,
                        oy,
                    );
                }
                let cy = control.cy();
                let (x0, x1) = (control.x + SLIDER_KNOB_R, control.right() - SLIDER_KNOB_R);
                gfx::fill(pm, &rounded(R::new(x0, cy - 2.0, x1 - x0, 4.0), 2.0), white(40), ts);
                let kx = gfx::lerp(x0, x1, value);
                gfx::fill(pm, &rounded(R::new(x0, cy - 2.0, (kx - x0).max(4.0), 4.0), 2.0), white(225), ts);
                let dragging = self.dragging_slider.is_some_and(|d| matches!(&self.rows[d], Row::Slider(_, k, _) if k == key));
                let kr = SLIDER_KNOB_R + if hovered || dragging { 1.5 } else { 0.0 };
                let knob = gfx::circle(kx, cy, kr);
                gfx::fill(pm, &knob, white(255), ts);
                gfx::stroke(pm, &knob, gfx::black(70), 1.0, false, ts);
            }
            Row::Hotkey(_) => {
                let chip = rounded(control, 8.0);
                if self.capturing {
                    let pulse = 0.5 + 0.5 * (self.phase * 2.0).sin();
                    gfx::fill(pm, &chip, white(gfx::lerp(18.0, 48.0, pulse) as u8), ts);
                    gfx::stroke(pm, &chip, white(120), 1.0, false, ts);
                } else {
                    gfx::fill(pm, &chip, white(if hovered { 34 } else { 22 }), ts);
                }
                draw_text(pm, &self.hotkey_text(cfg), control, &TextStyle::new(9.0), text(true), HAlign::Center, VAlign::Center, s, ox, oy);
            }
            Row::Text(_, key) => {
                gfx::fill(pm, &rounded(control, 8.0), white(if hovered { 34 } else { 22 }), ts);
                let st = TextStyle::new(9.0);
                let t = gfx::elide(&cfg.str(key), &st, control.w - 20.0);
                draw_text(pm, &t, control, &st, text(true), HAlign::Center, VAlign::Center, s, ox, oy);
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_danger(&self, pm: &mut Pixmap, index: usize, rect: R, ts: Transform, s: f32, ox: f32, oy: f32) {
        let st = TextStyle::new(9.5);
        if !self.confirming_quit {
            if matches!(self.hover, Some((Some(i), _)) if i == index) {
                gfx::fill(pm, &rounded(rect.adjusted(4.0, 4.0, -4.0, -4.0), 9.0), white(14), ts);
            }
            draw_text(pm, &tr("Turn off dictation"), rect.adjusted(14.0, 0.0, -14.0, 0.0), &st, text(true), HAlign::Left, VAlign::Center, s, ox, oy);
            return;
        }
        let (confirm, cancel) = Self::confirm_rects(rect);
        draw_text(pm, &tr("Turn off?"), R::new(rect.x + 14.0, rect.y, cancel.x - rect.x - 22.0, rect.h), &st, text(true), HAlign::Left, VAlign::Center, s, ox, oy);
        let hover_part = self.hover.and_then(|h| h.1);
        gfx::fill(pm, &rounded(cancel, 14.0), white(if hover_part == Some(Part::Cancel) { 34 } else { 22 }), ts);
        draw_text(pm, &tr("Cancel"), cancel, &TextStyle::new(9.0), text(true), HAlign::Center, VAlign::Center, s, ox, oy);
        gfx::fill(pm, &rounded(confirm, 14.0), white(if hover_part == Some(Part::Confirm) { 255 } else { 225 }), ts);
        draw_text(pm, &tr("Turn off"), confirm, &TextStyle::new(9.0), gfx::black(255), HAlign::Center, VAlign::Center, s, ox, oy);
    }
}

fn text(primary: bool) -> gfx::Color {
    white(if primary { 235 } else { 150 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_label_like_python_g() {
        assert_eq!(silence_fmt(2000.0), "2 s");
        assert_eq!(silence_fmt(4500.0), "4.5 s");
        assert_eq!(silence_fmt(750.0), "0.75 s");
    }
}
