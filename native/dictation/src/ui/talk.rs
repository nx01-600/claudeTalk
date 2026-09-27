//! The "Claude is talking" pill: a small glass pill with moving bars while
//! Claude's voice plays, and a red X on its left that silences Claude right
//! away (talk mode and dictation stay on). It sits where the dictation pill
//! goes; while the dictation pill is up it slides to its left, and it
//! follows the dictation pill when that one is dragged.

use super::gfx::{self, circle, rounded, R};
use super::glass::{self, Material, INSET};
use super::pill::Tween;
use super::window::Layered;
use std::time::{Duration, Instant};
use tiny_skia::{Pixmap, Transform};

pub const WIDTH: f32 = 116.0;
pub const HEIGHT: f32 = 44.0;
/// Space between this pill and the dictation pill beside it.
pub const GAP: f32 = 10.0;
const STOP_CX: f32 = 22.0;
const STOP_R: f32 = 13.0;
const STOP_HIT_R: f32 = 17.0;
const BAR_COUNT: usize = 4;
const BAR_WIDTH: f32 = 5.0;
const BAR_GAP: f32 = 6.0;
const BAR_MIN: f32 = 5.0;
const BAR_MAX: f32 = 22.0;
const SHOW_MS: f32 = 180.0;
const HIDE_MS: f32 = 150.0;
/// Between two phrases the player stops for a moment: don't blink.
const LINGER: Duration = Duration::from_millis(700);
/// After the X, ignore the tail of the phrase that was cut.
const MUTE_AFTER_STOP: Duration = Duration::from_millis(900);
const WAVE_SPEED: f32 = 6.0;

pub struct TalkPill {
    pub win: Layered,
    pub scale: f32,
    /// Where the window is heading (physical px); it glides there.
    target: Option<(i32, i32)>,
    pos: (f32, f32),
    closing: bool,
    anim: Option<Tween>,
    last_heard: Instant,
    muted_until: Instant,
    hover: bool,
    hover_t: f32,
    bg_raw: Option<Pixmap>,
    bg: Option<Pixmap>,
    bg_print: u64,
    bg_glass: i32,
    t0: Instant,
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
            hover: false,
            hover_t: 0.0,
            bg_raw: None,
            bg: None,
            bg_print: 0,
            bg_glass: -1,
            t0: now,
        }
    }

    pub fn phys_size(&self) -> (u32, u32) {
        (((WIDTH + 2.0 * INSET) * self.scale).ceil() as u32, ((HEIGHT + 2.0 * INSET) * self.scale).ceil() as u32)
    }

    /// Window origin that puts the pill's center at (cx, cy), screen px.
    pub fn origin_for_center(cx: f32, cy: f32, s: f32) -> (i32, i32) {
        ((cx - (WIDTH / 2.0 + INSET) * s).round() as i32, (cy - (HEIGHT / 2.0 + INSET) * s).round() as i32)
    }

    pub fn shown(&self) -> bool {
        self.win.visible && !self.closing
    }

    /// Fed every poll with whether Claude's voice is playing (or queued).
    /// Returns true when the pill should appear now.
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

    /// Moves toward `origin` (gliding while visible, jumping otherwise).
    pub fn aim(&mut self, origin: (i32, i32), scale: f32) {
        self.scale = scale;
        self.target = Some(origin);
        if !self.win.visible {
            self.pos = (origin.0 as f32, origin.1 as f32);
            self.win.x = origin.0;
            self.win.y = origin.1;
        }
    }

    pub fn fade_in(&mut self, capturable: bool) {
        self.closing = false;
        if !self.win.visible {
            self.win.set_capturable(capturable);
            self.bg_raw = None;
            self.bg = None;
            self.win.opacity = 0.0;
        }
        self.anim = Some(Tween::new(self.win.opacity, 1.0, SHOW_MS, gfx::out_cubic));
    }

    pub fn fade_out(&mut self) {
        if !self.win.visible || self.closing {
            return;
        }
        self.closing = true;
        self.hover = false;
        self.anim = Some(Tween::new(self.win.opacity, 0.0, HIDE_MS, gfx::in_cubic));
    }

    /// The X was pressed: hide now and don't come back for the rest of the
    /// phrase that is being cut.
    pub fn silenced(&mut self) {
        self.muted_until = Instant::now() + MUTE_AFTER_STOP;
        self.fade_out();
    }

    /// One frame. Returns (still visible, moved).
    pub fn tick(&mut self) -> (bool, bool) {
        self.hover_t = gfx::lerp(self.hover_t, if self.hover { 1.0 } else { 0.0 }, 0.25);
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

    fn over_stop(&self, sx: i32, sy: i32) -> bool {
        let (lx, ly) = ((sx - self.win.x) as f32 / self.scale, (sy - self.win.y) as f32 / self.scale);
        (lx - (INSET + STOP_CX)).hypot(ly - (INSET + HEIGHT / 2.0)) <= STOP_HIT_R
    }

    /// Returns whether the pointer is over the X (hand cursor).
    pub fn mouse_move(&mut self, sx: i32, sy: i32) -> bool {
        self.hover = !self.closing && self.over_stop(sx, sy);
        self.hover
    }

    pub fn mouse_leave(&mut self) {
        self.hover = false;
    }

    /// Returns true when the click lands on the X.
    pub fn mouse_down(&mut self, sx: i32, sy: i32) -> bool {
        !self.closing && self.over_stop(sx, sy)
    }

    /// For --render-test.
    pub fn test_backdrop(&mut self, raw: &Pixmap, m: Material, hover: bool) {
        self.bg = Some(glass::glassify(raw, m));
        self.win.opacity = 1.0;
        self.hover_t = if hover { 1.0 } else { 0.0 };
    }

    pub fn render(&self, m: Material) -> Pixmap {
        let s = self.scale;
        let (w, h) = self.phys_size();
        let mut pm = Pixmap::new(w, h).unwrap();
        let ts = Transform::from_scale(s, s);
        let rect = R::new(INSET, INSET, WIDTH, HEIGHT);
        glass::paint_glass(&mut pm, rect, HEIGHT / 2.0, self.bg.as_ref(), m, Some(gfx::black(m.tint_alpha())), s);
        // the X: red, a little bigger and brighter under the pointer
        let (cx, cy) = (INSET + STOP_CX, INSET + HEIGHT / 2.0);
        let r = STOP_R * (1.0 + 0.1 * self.hover_t);
        let red = gfx::rgba(255, (69.0 + 20.0 * self.hover_t) as u8, (58.0 + 18.0 * self.hover_t) as u8, 255);
        gfx::fill(&mut pm, &circle(cx, cy, r), red, ts);
        let k = 4.3 * (1.0 + 0.1 * self.hover_t);
        gfx::line(&mut pm, cx - k, cy - k, cx + k, cy + k, gfx::white(255), 2.3, true, ts);
        gfx::line(&mut pm, cx - k, cy + k, cx + k, cy - k, gfx::white(255), 2.3, true, ts);
        // Claude's voice: warm bars on a slow wave (no level is measured)
        let t = self.t0.elapsed().as_secs_f32() * WAVE_SPEED;
        let total = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
        let area_x = INSET + STOP_CX + STOP_R + 6.0;
        let area_w = INSET + WIDTH - 12.0 - area_x;
        let x0 = area_x + (area_w - total) / 2.0;
        for i in 0..BAR_COUNT {
            let level = 0.25 + 0.75 * ((t - i as f32 * 1.1).sin() * 0.5 + 0.5).powf(1.6);
            let bh = BAR_MIN + level * (BAR_MAX - BAR_MIN);
            let x = x0 + i as f32 * (BAR_WIDTH + BAR_GAP);
            gfx::fill(&mut pm, &rounded(R::new(x, cy - bh / 2.0, BAR_WIDTH, bh), BAR_WIDTH / 2.0), gfx::rgba(255, 178, 140, 235), ts);
        }
        pm
    }
}
