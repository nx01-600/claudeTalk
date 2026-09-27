//! The "Claude is talking" capsule, shown while Claude's voice plays. Two
//! rows, like a music player:
//! - top: pause/resume, stop (red X: drops only the phrase playing; the
//!   queue goes on), skip (only while more answers wait, with their count;
//!   the count turns blue when some come from another session) and a
//!   speaker icon whose volume slider opens on hover, away from the edge;
//! - bottom: bars that follow the loudness of what is being heard right now
//!   (measured by the speaker, shared through ct_core::voice_link).
//! It sits where the dictation pill goes; while the dictation pill is up it
//! slides to its left, and it follows that pill when it is dragged.

use super::gfx::{self, circle, draw_text, rounded, white, HAlign, TextStyle, VAlign, R};
use super::glass::{self, Material, INSET};
use super::pill::Tween;
use super::window::{self, Layered};
use std::time::{Duration, Instant};
use tiny_skia::{PathBuilder, Pixmap, Transform};

pub const WIDTH: f32 = 156.0;
pub const HEIGHT: f32 = 80.0;
/// Space between this capsule and the dictation pill beside it.
pub const GAP: f32 = 10.0;
const RADIUS: f32 = 22.0;
const ROW_Y: f32 = 25.0;
const BARS_Y: f32 = 57.0;
const BUTTON_HIT_R: f32 = 14.0;
const BAR_COUNT: usize = 11;
const BAR_WIDTH: f32 = 4.0;
const BAR_GAP: f32 = 5.0;
const BAR_MIN: f32 = 3.0;
const BAR_MAX: f32 = 24.0;
/// Frames between two bar samples (~20 per second at 60 fps).
const BAR_EVERY: u32 = 3;
const POP_W: f32 = 40.0;
const POP_H: f32 = 132.0;
const POP_GAP: f32 = 8.0;
const TRACK_TOP: f32 = 16.0;
const TRACK_BOTTOM: f32 = 30.0;
const SHOW_MS: f32 = 180.0;
const HIDE_MS: f32 = 150.0;
/// Between two phrases the player stops for a moment: don't blink.
const LINGER: Duration = Duration::from_millis(700);
/// After the X, ignore the tail of the phrase that was cut.
const MUTE_AFTER_STOP: Duration = Duration::from_millis(900);
/// The volume slider stays open this long after the pointer leaves it.
const POP_LINGER: Duration = Duration::from_millis(350);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Button {
    Pause,
    Stop,
    Skip,
    Volume,
}

/// What a click asks the app to do.
pub enum Act {
    TogglePause,
    Stop,
    Skip,
    /// Slider moved: apply live (volume 0-100).
    Volume(i32),
    /// Slider released: save it.
    VolumeDone(i32),
    /// The capsule is being dragged; its center is now here (screen px).
    Dragged(f32, f32),
    /// The drag ended.
    DragEnd,
    /// Start or end a drag (mouse capture).
    Capture(bool),
}

pub struct TalkPill {
    pub win: Layered,
    pub scale: f32,
    target: Option<(i32, i32)>,
    pos: (f32, f32),
    closing: bool,
    anim: Option<Tween>,
    last_heard: Instant,
    muted_until: Instant,
    /// The slider opens upward (capsule in the lower half of its screen).
    up: bool,
    pub paused: bool,
    /// Answers waiting after the one playing.
    pub queued: usize,
    /// Some of them come from another Claude Code session.
    pub other_session: bool,
    pub volume: i32,
    hover: Option<Button>,
    hover_t: [f32; 4],
    pop_open: bool,
    pop_t: f32,
    pop_left_at: Option<Instant>,
    dragging: bool,
    /// Pressed on the body: (cursor, window origin) at the press.
    drag_from: Option<((i32, i32), (i32, i32))>,
    /// Moved past the threshold: the capsule follows the pointer.
    pub moving: bool,
    levels: [f32; BAR_COUNT],
    shown_levels: [f32; BAR_COUNT],
    frame_n: u32,
    bg_raw: Option<Pixmap>,
    bg: Option<Pixmap>,
    bg_print: u64,
    bg_glass: i32,
}

fn index(b: Button) -> usize {
    b as usize
}

impl TalkPill {
    pub fn new(win: Layered) -> Self {
        let now = Instant::now();
        Self {
            win,
            scale: 1.0,
            target: None,
            pos: (0.0, 0.0),
            closing: false,
            anim: None,
            last_heard: now - LINGER * 2,
            muted_until: now,
            up: true,
            paused: false,
            queued: 0,
            other_session: false,
            volume: 100,
            hover: None,
            hover_t: [0.0; 4],
            pop_open: false,
            pop_t: 0.0,
            pop_left_at: None,
            dragging: false,
            drag_from: None,
            moving: false,
            levels: [0.0; BAR_COUNT],
            shown_levels: [0.0; BAR_COUNT],
            frame_n: 0,
            bg_raw: None,
            bg: None,
            bg_print: 0,
            bg_glass: -1,
        }
    }

    fn logical_size() -> (f32, f32) {
        (WIDTH + 2.0 * INSET, HEIGHT + POP_GAP + POP_H + 2.0 * INSET)
    }

    pub fn phys_size(&self) -> (u32, u32) {
        let (w, h) = Self::logical_size();
        ((w * self.scale).ceil() as u32, (h * self.scale).ceil() as u32)
    }

    /// Top of the capsule inside the window (the slider takes the rest).
    fn cap_top(&self) -> f32 {
        if self.up {
            INSET + POP_H + POP_GAP
        } else {
            INSET
        }
    }

    /// The capsule's vertical center inside the window (logical px).
    pub fn cap_center_y(&self) -> f32 {
        self.cap_top() + HEIGHT / 2.0
    }

    fn cap_rect(&self) -> R {
        R::new(INSET, self.cap_top(), WIDTH, HEIGHT)
    }

    pub fn shown(&self) -> bool {
        self.win.visible && !self.closing
    }

    /// Fed every poll with whether Claude's voice is playing (or queued).
    /// Returns true when the capsule should appear now.
    pub fn heard(&mut self, talking: bool) -> bool {
        let now = Instant::now();
        if talking && now >= self.muted_until {
            self.last_heard = now;
            return !self.shown();
        }
        false
    }

    /// True once Claude has been quiet long enough to hide.
    pub fn quiet(&self) -> bool {
        self.last_heard.elapsed() >= LINGER || Instant::now() < self.muted_until
    }

    /// Puts the capsule's center at (cx, cy), screen px; glides there while
    /// visible. The slider opens toward the middle of the screen.
    pub fn aim(&mut self, cx: f32, cy: f32, scale: f32) {
        if self.moving {
            return; // the pointer places it
        }
        self.scale = scale;
        if !self.win.visible {
            let mid = window::monitor_at(cx as i32, cy as i32, true).map(|m| (m.work.top + m.work.bottom) as f32 / 2.0);
            self.up = mid.is_none_or(|mid| cy > mid);
        }
        let x = (cx - (INSET + WIDTH / 2.0) * scale).round() as i32;
        let y = (cy - (self.cap_top() + HEIGHT / 2.0) * scale).round() as i32;
        self.target = Some((x, y));
        if !self.win.visible {
            self.pos = (x as f32, y as f32);
            self.win.x = x;
            self.win.y = y;
        }
    }

    pub fn fade_in(&mut self, capturable: bool) {
        self.closing = false;
        if !self.win.visible {
            self.win.set_capturable(capturable);
            self.bg_raw = None;
            self.bg = None;
            self.win.opacity = 0.0;
            self.levels = [0.0; BAR_COUNT];
            self.shown_levels = [0.0; BAR_COUNT];
            self.pop_open = false;
            self.pop_t = 0.0;
        }
        self.anim = Some(Tween::new(self.win.opacity, 1.0, SHOW_MS, gfx::out_cubic));
    }

    pub fn fade_out(&mut self) {
        if !self.win.visible || self.closing || self.dragging || self.moving {
            return;
        }
        self.closing = true;
        self.hover = None;
        self.pop_open = false;
        self.anim = Some(Tween::new(self.win.opacity, 0.0, HIDE_MS, gfx::in_cubic));
    }

    /// The X was pressed: hide now and don't come back for the rest of the
    /// phrase being cut.
    pub fn silenced(&mut self) {
        self.muted_until = Instant::now() + MUTE_AFTER_STOP;
        self.fade_out();
    }

    /// One frame; `level` is the loudness playing now. Returns (still
    /// visible, moved).
    pub fn tick(&mut self, level: f32) -> (bool, bool) {
        self.frame_n = self.frame_n.wrapping_add(1);
        if self.frame_n % BAR_EVERY == 0 {
            self.levels.rotate_left(1);
            self.levels[BAR_COUNT - 1] = if self.paused { 0.0 } else { level };
        }
        for i in 0..BAR_COUNT {
            self.shown_levels[i] = gfx::lerp(self.shown_levels[i], self.levels[i], 0.35);
        }
        for b in [Button::Pause, Button::Stop, Button::Skip, Button::Volume] {
            let on = self.hover == Some(b) || (b == Button::Volume && self.pop_open);
            self.hover_t[index(b)] = gfx::lerp(self.hover_t[index(b)], if on { 1.0 } else { 0.0 }, 0.25);
        }
        if self.pop_open && !self.dragging && self.hover != Some(Button::Volume) {
            if self.pop_left_at.is_some_and(|t| t.elapsed() >= POP_LINGER) {
                self.pop_open = false;
            }
        }
        self.pop_t = gfx::lerp(self.pop_t, if self.pop_open { 1.0 } else { 0.0 }, 0.3);
        let mut moved = false;
        if let Some((tx, ty)) = self.target {
            let (x, y) = (gfx::lerp(self.pos.0, tx as f32, 0.22), gfx::lerp(self.pos.1, ty as f32, 0.22));
            self.pos = if (x - tx as f32).abs() < 0.5 && (y - ty as f32).abs() < 0.5 { (tx as f32, ty as f32) } else { (x, y) };
            let (nx, ny) = (self.pos.0.round() as i32, self.pos.1.round() as i32);
            if (nx, ny) != (self.win.x, self.win.y) {
                self.win.x = nx;
                self.win.y = ny;
                moved = true;
            }
        }
        if let Some(a) = &self.anim {
            let (v, done) = a.value();
            self.win.opacity = v;
            if done {
                self.anim = None;
                if self.closing {
                    self.closing = false;
                    self.win.hide();
                    return (false, moved);
                }
            }
        }
        (true, moved)
    }

    /// The window is tall to leave room for the volume slider; while that
    /// is closed only the capsule's band is captured and blurred (less than
    /// half the pixels).
    fn glass_band(&self) -> (u32, u32) {
        let s = self.scale;
        let (_, h) = self.phys_size();
        if self.pop_open || self.pop_t > 0.02 {
            return (0, h);
        }
        let top = ((self.cap_top() - INSET) * s).floor().max(0.0) as u32;
        let bottom = (((self.cap_top() + HEIGHT + INSET) * s).ceil() as u32).min(h);
        (top, bottom - top)
    }

    pub fn refresh_background(&mut self, m: Material, capturable: bool) -> bool {
        let (w, h) = self.phys_size();
        let (band_y, band_h) = self.glass_band();
        let band_changed = self.bg_raw.as_ref().is_some_and(|r| r.height() != band_h);
        if self.bg_raw.is_none() || band_changed || !(capturable && self.win.visible) {
            self.bg_raw = glass::capture(self.win.x, self.win.y + band_y as i32, w, band_h);
        }
        let Some(raw) = &self.bg_raw else { return false };
        let print = glass::fingerprint(raw);
        let g = (m.glass * 100.0).round() as i32;
        if self.bg.is_some() && !band_changed && print == self.bg_print && g == self.bg_glass {
            return false;
        }
        self.bg_print = print;
        self.bg_glass = g;
        let blurred = glass::glassify(raw, m);
        let mut full = Pixmap::new(w, h).unwrap();
        full.draw_pixmap(0, band_y as i32, blurred.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
        self.bg = Some(full);
        true
    }

    /// True when the glass must be taken now (the slider just opened).
    pub fn needs_glass(&self) -> bool {
        self.win.visible && self.bg_raw.is_none()
    }

    // --- layout ----------------------------------------------------------------------

    fn buttons(&self) -> Vec<Button> {
        let mut v = vec![Button::Pause, Button::Stop];
        if self.queued > 0 {
            v.push(Button::Skip);
        }
        v.push(Button::Volume);
        v
    }

    /// Button centers, spread evenly over the top row.
    fn button_centers(&self) -> Vec<(Button, f32, f32)> {
        let list = self.buttons();
        let margin = 26.0;
        let span = WIDTH - 2.0 * margin;
        let step = span / (list.len() - 1) as f32;
        let y = self.cap_top() + ROW_Y;
        list.into_iter().enumerate().map(|(i, b)| (b, INSET + margin + i as f32 * step, y)).collect()
    }

    fn pop_rect(&self) -> R {
        let vx = self.button_centers().iter().find(|(b, ..)| *b == Button::Volume).map(|c| c.1).unwrap_or(INSET + WIDTH - 26.0);
        let y = if self.up { self.cap_top() - POP_GAP - POP_H } else { self.cap_top() + HEIGHT + POP_GAP };
        R::new(vx - POP_W / 2.0, y, POP_W, POP_H)
    }

    fn track(&self) -> (f32, f32, f32) {
        let r = self.pop_rect();
        (r.x + POP_W / 2.0, r.y + TRACK_TOP, r.y + POP_H - TRACK_BOTTOM)
    }

    fn local(&self, sx: i32, sy: i32) -> (f32, f32) {
        ((sx - self.win.x) as f32 / self.scale, (sy - self.win.y) as f32 / self.scale)
    }

    fn button_at(&self, lx: f32, ly: f32) -> Option<Button> {
        self.button_centers().into_iter().find(|(_, cx, cy)| (lx - cx).hypot(ly - cy) <= BUTTON_HIT_R).map(|(b, ..)| b)
    }

    fn in_pop(&self, lx: f32, ly: f32) -> bool {
        self.pop_open && self.pop_rect().contains(lx, ly)
    }

    fn volume_at(&self, ly: f32) -> i32 {
        let (_, top, bottom) = self.track();
        (((bottom - ly) / (bottom - top)).clamp(0.0, 1.0) * 100.0).round() as i32
    }

    // --- mouse -----------------------------------------------------------------------

    /// The capsule's center on screen (physical px).
    fn center_on_screen(&self) -> (f32, f32) {
        let s = self.scale;
        (self.win.x as f32 + (INSET + WIDTH / 2.0) * s, self.win.y as f32 + (self.cap_top() + HEIGHT / 2.0) * s)
    }

    /// Returns (hand cursor, actions). The hand shows over the buttons and
    /// the slider; the rest of the capsule is a drag handle.
    pub fn mouse_move(&mut self, sx: i32, sy: i32) -> (bool, Vec<Act>) {
        if let Some((start, origin)) = self.drag_from {
            let (dx, dy) = (sx - start.0, sy - start.1);
            if !self.moving && dx.abs() + dy.abs() >= 4 {
                self.moving = true;
                self.pop_open = false;
            }
            if self.moving {
                self.win.x = origin.0 + dx;
                self.win.y = origin.1 + dy;
                self.pos = (self.win.x as f32, self.win.y as f32);
                self.target = Some((self.win.x, self.win.y));
                let (cx, cy) = self.center_on_screen();
                return (false, vec![Act::Dragged(cx, cy)]);
            }
            return (false, Vec::new());
        }
        let (lx, ly) = self.local(sx, sy);
        if self.dragging {
            self.volume = self.volume_at(ly);
            return (true, vec![Act::Volume(self.volume)]);
        }
        let b = if self.closing { None } else { self.button_at(lx, ly) };
        let over_pop = self.in_pop(lx, ly);
        self.hover = if over_pop { Some(Button::Volume) } else { b };
        if b == Some(Button::Volume) && !self.pop_open {
            self.pop_open = true;
            // the slider's area needs its glass now
            self.bg_raw = None;
        }
        self.pop_left_at = if self.hover == Some(Button::Volume) { None } else { self.pop_left_at.or(Some(Instant::now())) };
        (b.is_some() || over_pop, Vec::new())
    }

    pub fn mouse_leave(&mut self) {
        if !self.dragging && self.drag_from.is_none() {
            self.hover = None;
            self.pop_left_at.get_or_insert(Instant::now());
        }
    }

    pub fn mouse_down(&mut self, sx: i32, sy: i32) -> Vec<Act> {
        if self.closing {
            return Vec::new();
        }
        let (lx, ly) = self.local(sx, sy);
        if self.in_pop(lx, ly) {
            self.dragging = true;
            self.volume = self.volume_at(ly);
            return vec![Act::Capture(true), Act::Volume(self.volume)];
        }
        match self.button_at(lx, ly) {
            Some(Button::Pause) => vec![Act::TogglePause],
            Some(Button::Stop) => vec![Act::Stop],
            Some(Button::Skip) => vec![Act::Skip],
            Some(Button::Volume) => {
                self.pop_open = !self.pop_open || self.pop_t < 0.5;
                Vec::new()
            }
            None if self.cap_rect().contains(lx, ly) => {
                self.drag_from = Some(((sx, sy), (self.win.x, self.win.y)));
                vec![Act::Capture(true)]
            }
            None => Vec::new(),
        }
    }

    pub fn mouse_up(&mut self) -> Vec<Act> {
        if self.drag_from.take().is_some() {
            let moved = std::mem::take(&mut self.moving);
            let mut acts = vec![Act::Capture(false)];
            if moved {
                acts.push(Act::DragEnd);
            }
            return acts;
        }
        if !self.dragging {
            return Vec::new();
        }
        self.dragging = false;
        self.pop_left_at = Some(Instant::now());
        vec![Act::Capture(false), Act::VolumeDone(self.volume)]
    }

    // --- drawing ---------------------------------------------------------------------

    /// For --render-test.
    pub fn test_state(&mut self, raw: &Pixmap, m: Material, paused: bool, queued: usize, pop: bool, other: bool) {
        self.other_session = other;
        self.bg = Some(glass::glassify(raw, m));
        self.win.opacity = 1.0;
        self.paused = paused;
        self.queued = queued;
        self.volume = 70;
        self.pop_open = pop;
        self.pop_t = if pop { 1.0 } else { 0.0 };
        self.hover_t = [0.0, if pop { 0.0 } else { 1.0 }, 0.0, if pop { 1.0 } else { 0.0 }];
        let wave = [0.2, 0.5, 0.8, 0.6, 0.9, 0.7, 0.4, 0.65, 0.85, 0.5, 0.3];
        self.shown_levels = if paused { [0.0; BAR_COUNT] } else { wave };
    }

    pub fn render(&self, m: Material) -> Pixmap {
        let s = self.scale;
        let (w, h) = self.phys_size();
        let mut pm = Pixmap::new(w, h).unwrap();
        let ts = Transform::from_scale(s, s);
        let cap = self.cap_rect();
        glass::paint_glass(&mut pm, cap, RADIUS, self.bg.as_ref(), m, Some(gfx::black(m.tint_alpha())), s);
        // hairline between the controls and the bars
        let sep_y = cap.y + (ROW_Y + BARS_Y) / 2.0 - 2.0;
        gfx::line(&mut pm, cap.x + 18.0, sep_y, cap.right() - 18.0, sep_y, white(22), 1.0, false, ts);
        for (b, cx, cy) in self.button_centers() {
            let t = self.hover_t[index(b)];
            if b != Button::Stop && t > 0.01 {
                gfx::fill(&mut pm, &circle(cx, cy, 13.0), white((34.0 * t) as u8), ts);
            }
            match b {
                Button::Pause => self.paint_pause(&mut pm, cx, cy, t, ts),
                Button::Stop => paint_stop(&mut pm, cx, cy, t, ts),
                Button::Skip => self.paint_skip(&mut pm, cx, cy, t, ts),
                Button::Volume => self.paint_speaker(&mut pm, cx, cy, t, ts),
            }
        }
        self.paint_bars(&mut pm, ts);
        if self.pop_t > 0.02 {
            self.paint_pop(&mut pm, m, ts);
        }
        pm
    }

    fn paint_pause(&self, pm: &mut Pixmap, cx: f32, cy: f32, t: f32, ts: Transform) {
        let a = (215.0 + 40.0 * t) as u8;
        if self.paused {
            // play: a rounded triangle
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 4.0, cy - 7.0);
            pb.line_to(cx + 7.0, cy);
            pb.line_to(cx - 4.0, cy + 7.0);
            pb.close();
            let p = pb.finish();
            gfx::fill(pm, &p, white(a), ts);
            gfx::stroke(pm, &p, white(a), 2.0, true, ts);
        } else {
            gfx::fill(pm, &rounded(R::new(cx - 6.0, cy - 7.5, 4.2, 15.0), 1.6), white(a), ts);
            gfx::fill(pm, &rounded(R::new(cx + 1.8, cy - 7.5, 4.2, 15.0), 1.6), white(a), ts);
        }
    }

    fn paint_skip(&self, pm: &mut Pixmap, cx: f32, cy: f32, t: f32, ts: Transform) {
        let a = (215.0 + 40.0 * t) as u8;
        let mut pb = PathBuilder::new();
        pb.move_to(cx - 7.0, cy - 6.5);
        pb.line_to(cx + 2.5, cy);
        pb.line_to(cx - 7.0, cy + 6.5);
        pb.close();
        let p = pb.finish();
        gfx::fill(pm, &p, white(a), ts);
        gfx::stroke(pm, &p, white(a), 1.6, true, ts);
        gfx::fill(pm, &rounded(R::new(cx + 3.6, cy - 7.0, 3.0, 14.0), 1.2), white(a), ts);
        // how many answers wait: a small badge on the upper right
        let label = if self.queued > 9 { "9+".to_string() } else { self.queued.to_string() };
        let (bx, by) = (cx + 9.0, cy - 8.0);
        // warm: this session's answers; blue: another session's are waiting
        let badge = if self.other_session { gfx::rgba(110, 175, 255, 255) } else { gfx::rgba(255, 178, 140, 255) };
        gfx::fill(pm, &circle(bx, by, 6.5), badge, ts);
        draw_text(pm, &label, R::new(bx - 7.0, by - 7.0, 14.0, 14.0), &TextStyle::bold(6.5), gfx::black(220), HAlign::Center, VAlign::Center, self.scale, 0.0, 0.0);
    }

    fn paint_speaker(&self, pm: &mut Pixmap, cx: f32, cy: f32, t: f32, ts: Transform) {
        let a = (215.0 + 40.0 * t) as u8;
        // the glyph with its arcs is wider to the right: center it
        let cx = cx - 2.5;
        let mut pb = PathBuilder::new();
        pb.move_to(cx - 8.0, cy - 3.2);
        pb.line_to(cx - 4.6, cy - 3.2);
        pb.line_to(cx - 0.4, cy - 7.2);
        pb.line_to(cx - 0.4, cy + 7.2);
        pb.line_to(cx - 4.6, cy + 3.2);
        pb.line_to(cx - 8.0, cy + 3.2);
        pb.close();
        let p = pb.finish();
        gfx::fill(pm, &p, white(a), ts);
        gfx::stroke(pm, &p, white(a), 1.2, true, ts);
        // one arc per third of the volume
        let arcs = if self.volume == 0 { 0 } else { 1 + (self.volume.min(99) / 34) as usize };
        for i in 0..arcs {
            let r = 4.0 + i as f32 * 3.4;
            let mut pb = PathBuilder::new();
            let (a0, a1) = (-0.9f32, 0.9f32);
            pb.move_to(cx + 1.5 + r * a0.cos(), cy + r * a0.sin());
            let steps = 8;
            for k in 1..=steps {
                let ang = a0 + (a1 - a0) * k as f32 / steps as f32;
                pb.line_to(cx + 1.5 + r * ang.cos(), cy + r * ang.sin());
            }
            gfx::stroke(pm, &pb.finish(), white(a), 1.6, true, ts);
        }
        if self.volume == 0 {
            gfx::line(pm, cx + 3.0, cy - 3.5, cx + 8.0, cy + 3.5, white(a), 1.6, true, ts);
            gfx::line(pm, cx + 3.0, cy + 3.5, cx + 8.0, cy - 3.5, white(a), 1.6, true, ts);
        }
    }

    fn paint_bars(&self, pm: &mut Pixmap, ts: Transform) {
        let cap = self.cap_rect();
        let total = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
        let x0 = cap.x + (WIDTH - total) / 2.0;
        let cy = cap.y + BARS_Y;
        let alpha = if self.paused { 120 } else { 235 };
        for i in 0..BAR_COUNT {
            let level = self.shown_levels[i];
            let bh = BAR_MIN + level * (BAR_MAX - BAR_MIN);
            let x = x0 + i as f32 * (BAR_WIDTH + BAR_GAP);
            gfx::fill(pm, &rounded(R::new(x, cy - bh / 2.0, BAR_WIDTH, bh), BAR_WIDTH / 2.0), gfx::rgba(255, 178, 140, alpha), ts);
        }
    }

    fn paint_pop(&self, pm: &mut Pixmap, m: Material, ts: Transform) {
        let r = self.pop_rect();
        // grows out of the speaker icon
        let k = gfx::out_cubic(self.pop_t.clamp(0.0, 1.0));
        let h = r.h * k;
        let rect = if self.up { R::new(r.x, r.bottom() - h, r.w, h) } else { R::new(r.x, r.y, r.w, h) };
        if h < 8.0 {
            return;
        }
        glass::paint_glass(pm, rect, POP_W / 2.0, self.bg.as_ref(), m, Some(gfx::black(m.tint_alpha())), self.scale);
        if k < 0.9 {
            return;
        }
        let (tx, top, bottom) = self.track();
        let v = self.volume as f32 / 100.0;
        let ky = bottom - v * (bottom - top);
        gfx::fill(pm, &rounded(R::new(tx - 2.0, top, 4.0, bottom - top), 2.0), white(40), ts);
        gfx::fill(pm, &rounded(R::new(tx - 2.0, ky, 4.0, (bottom - ky).max(4.0)), 2.0), white(225), ts);
        gfx::fill(pm, &circle(tx, ky, 7.0), white(255), ts);
        gfx::stroke(pm, &circle(tx, ky, 7.0), gfx::black(60), 1.0, false, ts);
        draw_text(pm, &self.volume.to_string(), R::new(r.x, r.bottom() - 24.0, r.w, 18.0), &TextStyle::new(8.0), white(220), HAlign::Center, VAlign::Center, self.scale, 0.0, 0.0);
    }
}

fn paint_stop(pm: &mut Pixmap, cx: f32, cy: f32, t: f32, ts: Transform) {
    let r = 11.0 * (1.0 + 0.1 * t);
    let red = gfx::rgba(255, (69.0 + 20.0 * t) as u8, (58.0 + 18.0 * t) as u8, 255);
    gfx::fill(pm, &circle(cx, cy, r), red, ts);
    let k = 3.8 * (1.0 + 0.1 * t);
    gfx::line(pm, cx - k, cy - k, cx + k, cy + k, white(255), 2.1, true, ts);
    gfx::line(pm, cx - k, cy + k, cx + k, cy - k, white(255), 2.1, true, ts);
}
