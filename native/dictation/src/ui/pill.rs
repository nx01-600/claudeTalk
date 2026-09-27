//! The floating pill shown while recording (overlay.py: RecordingOverlay):
//! five level bars, the gear, the "transcribing" wave, and the morph into a
//! glass circle with a green check (pasted) or an amber clipboard (only
//! copied). Dragged anywhere; optionally remembers the spot.

use super::gfx::{self, circle, rounded, white, R};
use super::glass::{self, Material, INSET};
use super::window::{self, Layered};
use std::time::Instant;
use tiny_skia::{PathBuilder, Pixmap, Transform};

pub const WIDTH: f32 = 200.0;
pub const HEIGHT: f32 = 56.0;
const SLIDE_PX: f32 = 14.0;
const EDGE_MARGIN: f32 = 40.0;
const BAR_COUNT: usize = 5;
const BAR_WIDTH: f32 = 6.0;
const BAR_GAP: f32 = 8.0;
const BAR_MIN: f32 = 6.0;
const BAR_MAX: f32 = 30.0;
const BARS_AREA_WIDTH: f32 = WIDTH - 44.0;
const GEAR_CENTER_X: f32 = WIDTH - 24.0;
const GEAR_OUTER_R: f32 = 9.6;
const GEAR_BODY_R: f32 = 6.9;
const GEAR_TOOTH_W: f32 = 3.8;
const GEAR_HOLE_R: f32 = 3.1;
const GEAR_TEETH: usize = 8;
const GEAR_HIT_R: f32 = 14.0;
const SHOW_MS: f32 = 220.0;
const HIDE_MS: f32 = 170.0;
const DRAG_THRESHOLD: i32 = 4;
const MORPH_MS: f32 = 340.0;
const BADGE_R: f32 = 16.0;
const BADGE_IN_MS: f32 = 260.0;
const CHECK_DRAW_MS: f32 = 240.0;
pub const BADGE_HOLD_MS: f32 = 700.0;
const BUSY_WAVE_SPEED: f32 = 7.0;

pub fn badge_total_ms() -> f32 {
    MORPH_MS / 2.0 + BADGE_IN_MS + CHECK_DRAW_MS + BADGE_HOLD_MS
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Phase {
    Record,
    Busy,
    Done,
}

pub struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    ms: f32,
    ease: fn(f32) -> f32,
}

impl Tween {
    pub fn new(from: f32, to: f32, ms: f32, ease: fn(f32) -> f32) -> Self {
        Self { from, to, start: Instant::now(), ms, ease }
    }
    pub fn value(&self) -> (f32, bool) {
        let t = (self.start.elapsed().as_secs_f32() * 1000.0 / self.ms).min(1.0);
        (gfx::lerp(self.from, self.to, (self.ease)(t)), t >= 1.0)
    }
}

pub struct Pill {
    pub win: Layered,
    pub scale: f32,
    levels_target: [f32; BAR_COUNT],
    levels_shown: [f32; BAR_COUNT],
    slide: f32,
    gear_hover: bool,
    gear_hover_t: f32,
    gear_angle: f32,
    pub gear_angle_target: f32,
    pub closing: bool,
    pub hide_pending: bool,
    pub phase: Phase,
    phase_t0: Instant,
    pub phase_token: u64,
    ok: bool,
    anim: Option<Tween>,
    drag_from: Option<((i32, i32), (i32, i32))>,
    pub dragging: bool,
    bg_raw: Option<Pixmap>,
    bg: Option<Pixmap>,
    bg_print: u64,
    bg_glass: i32,
}

pub enum Click {
    None,
    TogglePanel,
}

impl Pill {
    pub fn new(win: Layered) -> Self {
        Self {
            win,
            scale: 1.0,
            levels_target: [0.0; BAR_COUNT],
            levels_shown: [0.0; BAR_COUNT],
            slide: 1.0,
            gear_hover: false,
            gear_hover_t: 0.0,
            gear_angle: 0.0,
            gear_angle_target: 0.0,
            closing: false,
            hide_pending: false,
            phase: Phase::Record,
            phase_t0: Instant::now(),
            phase_token: 0,
            ok: true,
            anim: None,
            drag_from: None,
            dragging: false,
            bg_raw: None,
            bg: None,
            bg_print: 0,
            bg_glass: -1,
        }
    }

    fn logical_size() -> (f32, f32) {
        (WIDTH + 2.0 * INSET, HEIGHT + SLIDE_PX + 2.0 * INSET)
    }

    fn phys_size(&self) -> (u32, u32) {
        let (w, h) = Self::logical_size();
        ((w * self.scale).ceil() as u32, (h * self.scale).ceil() as u32)
    }

    /// Where the pill goes: the remembered drag spot if it still lands on a
    /// screen, else centered at the bottom or top of the primary screen.
    pub fn place(&mut self, top: bool, saved: Option<(i32, i32)>) {
        let (x, y, s) = home(top, saved);
        self.win.x = x;
        self.win.y = y;
        self.scale = s;
    }

    /// Where the visible pill's center is on screen (physical px) and the
    /// scale there: its current spot while shown, else where it would appear.
    pub fn center(&self, top: bool, saved: Option<(i32, i32)>) -> (f32, f32, f32) {
        let (x, y, s) = if self.win.visible && !self.closing {
            (self.win.x, self.win.y, self.scale)
        } else {
            home(top, saved)
        };
        let rest_top = INSET + if top { SLIDE_PX } else { 0.0 };
        (x as f32 + (INSET + WIDTH / 2.0) * s, y as f32 + (rest_top + HEIGHT / 2.0) * s, s)
    }

    /// The window origin that puts the resting pill's center at (cx, cy).
    pub fn origin_for_center(cx: f32, cy: f32, top: bool, s: f32) -> (i32, i32) {
        let rest_top = INSET + if top { SLIDE_PX } else { 0.0 };
        ((cx - (INSET + WIDTH / 2.0) * s).round() as i32, (cy - (rest_top + HEIGHT / 2.0) * s).round() as i32)
    }

    pub fn pill_top(&self, top: bool) -> f32 {
        INSET + (if !top { self.slide } else { 1.0 - self.slide }) * SLIDE_PX
    }

    /// The visible pill in screen (physical) coordinates.
    pub fn pill_rect_on_screen(&self, top: bool) -> (f32, f32, f32, f32) {
        let s = self.scale;
        (
            self.win.x as f32 + INSET * s,
            self.win.y as f32 + self.pill_top(top) * s,
            WIDTH * s,
            HEIGHT * s,
        )
    }

    /// The settings panel opens below the pill when it sits in the upper
    /// half of its screen.
    pub fn opens_down(&self, top: bool) -> bool {
        let (x, y, w, h) = self.pill_rect_on_screen(top);
        let (cx, cy) = ((x + w / 2.0) as i32, (y + h / 2.0) as i32);
        let m = window::monitor_at(cx, cy, true).unwrap();
        (cy as f32) < m.work.top as f32 + (m.work.bottom - m.work.top) as f32 / 2.0
    }

    pub fn set_phase(&mut self, p: Phase) {
        self.phase = p;
        self.phase_t0 = Instant::now();
        self.phase_token += 1;
    }

    fn phase_ms(&self) -> f32 {
        self.phase_t0.elapsed().as_secs_f32() * 1000.0
    }

    fn morph(&self) -> f32 {
        if self.phase != Phase::Done {
            return 0.0;
        }
        gfx::in_out_cubic((self.phase_ms() / MORPH_MS).min(1.0))
    }

    /// Starts showing (or re-arms an already visible pill).
    pub fn fade_in(&mut self, top: bool, saved: Option<(i32, i32)>, capturable: bool) {
        self.set_phase(Phase::Record);
        self.closing = false;
        self.hide_pending = false;
        self.levels_target = [0.0; BAR_COUNT];
        self.levels_shown = [0.0; BAR_COUNT];
        if !self.win.visible {
            self.place(top, saved);
            self.win.set_capturable(capturable);
            self.bg_raw = None;
            self.win.opacity = 0.0;
            self.slide = 1.0;
        }
        self.anim = Some(Tween::new(self.win.opacity, 1.0, SHOW_MS, gfx::out_cubic));
    }

    /// Returns false if it must wait (the panel is open).
    pub fn fade_out(&mut self, panel_open: bool) {
        if !self.win.visible {
            return;
        }
        if panel_open {
            self.hide_pending = true;
            return;
        }
        self.closing = true;
        self.anim = Some(Tween::new(self.win.opacity, 0.0, HIDE_MS, gfx::in_cubic));
    }

    pub fn show_busy(&mut self) {
        if self.win.visible && !self.closing {
            self.set_phase(Phase::Busy);
        }
    }

    /// Returns the phase token to fade out after the badge, if shown.
    pub fn show_result(&mut self, ok: bool) -> Option<u64> {
        if !self.win.visible || self.closing {
            return None;
        }
        self.ok = ok;
        self.set_phase(Phase::Done);
        Some(self.phase_token)
    }

    pub fn set_level(&mut self, v: f32) {
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        self.levels_target.rotate_left(1);
        self.levels_target[BAR_COUNT - 1] = v;
    }

    /// One animation frame. Returns false once the pill finished hiding.
    pub fn tick(&mut self) -> bool {
        if self.phase == Phase::Busy {
            let t = Instant::now().duration_since(*START).as_secs_f32() * BUSY_WAVE_SPEED;
            for i in 0..BAR_COUNT {
                self.levels_target[i] = 0.08 + 0.3 * (t - i as f32 * 0.9).sin().max(0.0).powi(2);
            }
        }
        for i in 0..BAR_COUNT {
            self.levels_shown[i] = gfx::lerp(self.levels_shown[i], self.levels_target[i], 0.35);
        }
        self.gear_hover_t = gfx::lerp(self.gear_hover_t, if self.gear_hover { 1.0 } else { 0.0 }, 0.25);
        self.gear_angle = gfx::lerp(self.gear_angle, self.gear_angle_target, 0.18);
        if let Some(a) = &self.anim {
            let (v, done) = a.value();
            self.win.opacity = v;
            self.slide = 1.0 - v;
            if done {
                self.anim = None;
                if self.closing {
                    self.win.hide();
                    return false;
                }
            }
        }
        true
    }

    /// Takes (or re-blurs) the backdrop. Live mode re-captures every call;
    /// with "show in screen share" the snapshot taken before showing stays.
    pub fn refresh_background(&mut self, m: Material, capturable: bool) -> bool {
        let (w, h) = self.phys_size();
        if self.bg_raw.is_none() || !(capturable && self.win.visible) {
            self.bg_raw = glass::capture(self.win.x, self.win.y, w, h);
        }
        let Some(raw) = &self.bg_raw else { return false };
        let print = glass::fingerprint(raw);
        let g = (m.glass * 100.0).round() as i32;
        if self.bg.is_some() && print == self.bg_print && g == self.bg_glass {
            return false; // nothing behind changed: skip the blur
        }
        self.bg_print = print;
        self.bg_glass = g;
        self.bg = Some(glass::glassify(raw, m));
        true
    }

    fn gear_center(&self, top: bool) -> (f32, f32) {
        (INSET + GEAR_CENTER_X, self.pill_top(top) + HEIGHT / 2.0)
    }

    fn over_gear(&self, lx: f32, ly: f32, top: bool) -> bool {
        if self.phase == Phase::Done {
            return false;
        }
        let (cx, cy) = self.gear_center(top);
        (lx - cx).hypot(ly - cy) <= GEAR_HIT_R
    }

    fn local(&self, sx: i32, sy: i32) -> (f32, f32) {
        ((sx - self.win.x) as f32 / self.scale, (sy - self.win.y) as f32 / self.scale)
    }

    /// Mouse moved (screen coords). Returns the cursor to show and whether
    /// a drag just started (the panel closes then).
    pub fn mouse_move(&mut self, sx: i32, sy: i32, top: bool, panel_open: bool) -> (bool, bool) {
        if let Some((start, origin)) = self.drag_from {
            let (dx, dy) = (sx - start.0, sy - start.1);
            let mut started = false;
            if !self.dragging && dx.abs() + dy.abs() >= DRAG_THRESHOLD {
                self.dragging = true;
                started = true;
            }
            if self.dragging {
                self.win.x = origin.0 + dx;
                self.win.y = origin.1 + dy;
            }
            return (false, started);
        }
        let (lx, ly) = self.local(sx, sy);
        let hover = self.over_gear(lx, ly, top);
        if hover != self.gear_hover {
            self.gear_hover = hover;
            let base = if panel_open { 90.0 } else { 0.0 };
            self.gear_angle_target = base + if hover { 30.0 } else { 0.0 };
        }
        (hover, false)
    }

    pub fn mouse_leave(&mut self, panel_open: bool) {
        if self.drag_from.is_none() && self.gear_hover {
            self.gear_hover = false;
            self.gear_angle_target = if panel_open { 90.0 } else { 0.0 };
        }
    }

    pub fn mouse_down(&mut self, sx: i32, sy: i32, top: bool) -> Click {
        let (lx, ly) = self.local(sx, sy);
        if self.over_gear(lx, ly, top) {
            return Click::TogglePanel;
        }
        self.drag_from = Some(((sx, sy), (self.win.x, self.win.y)));
        self.dragging = false;
        Click::None
    }

    /// Returns true when a drag just ended.
    pub fn mouse_up(&mut self) -> bool {
        if self.drag_from.is_none() {
            return false;
        }
        let dragged = self.dragging;
        self.drag_from = None;
        self.dragging = false;
        dragged
    }

    /// For --render-test: a fixed backdrop instead of a screen capture.
    pub fn test_backdrop(&mut self, raw: &Pixmap, m: Material) {
        self.bg = Some(glass::glassify(raw, m));
    }

    /// For --render-test: jump to a moment of the animation.
    pub fn test_state(&mut self, phase: Phase, ms_into_phase: f32, levels: [f32; BAR_COUNT], ok: bool) {
        self.phase = phase;
        self.phase_t0 = Instant::now() - std::time::Duration::from_secs_f32(ms_into_phase / 1000.0);
        self.levels_shown = levels;
        self.ok = ok;
        self.slide = 0.0;
        self.win.opacity = 1.0;
    }

    pub fn render(&self, m: Material, top: bool) -> Pixmap {
        let s = self.scale;
        let (w, h) = self.phys_size();
        let mut pm = Pixmap::new(w, h).unwrap();
        let ts = Transform::from_scale(s, s);
        let top_y = self.pill_top(top);
        let radius = HEIGHT / 2.0;
        let morph = self.morph();
        let width = gfx::lerp(WIDTH, HEIGHT, morph);
        let rect = R::new(INSET + (WIDTH - width) / 2.0, top_y, width, HEIGHT);
        // veil: the pill's tint
        glass::paint_glass(&mut pm, rect, radius, self.bg.as_ref(), m, Some(gfx::black(m.tint_alpha())), s);
        if self.phase == Phase::Done {
            if morph < 1.0 {
                self.paint_bars(&mut pm, top_y, morph, ts);
            }
            self.paint_badge(&mut pm, top_y, ts);
        } else {
            self.paint_bars(&mut pm, top_y, 0.0, ts);
        }
        let gear_alpha = (1.0 - morph * 2.5).max(0.0);
        if gear_alpha > 0.0 {
            self.paint_gear(&mut pm, top, gear_alpha, ts);
        }
        pm
    }

    fn paint_bars(&self, pm: &mut Pixmap, top: f32, fold: f32, ts: Transform) {
        let total_w = BAR_COUNT as f32 * BAR_WIDTH + (BAR_COUNT - 1) as f32 * BAR_GAP;
        let start_x = INSET + (BARS_AREA_WIDTH - total_w) / 2.0;
        let center_x = INSET + WIDTH / 2.0;
        let center_y = top + HEIGHT / 2.0;
        for i in 0..BAR_COUNT {
            let level = self.levels_shown[i] * (1.0 - fold);
            let bar_h = BAR_MIN + level * (BAR_MAX - BAR_MIN);
            let x = gfx::lerp(start_x + i as f32 * (BAR_WIDTH + BAR_GAP), center_x - BAR_WIDTH / 2.0, fold);
            let y = center_y - bar_h / 2.0;
            let alpha = ((170.0 + 85.0 * (level * 1.6).min(1.0)) * (1.0 - fold)) as u8;
            gfx::fill(pm, &rounded(R::new(x, y, BAR_WIDTH, bar_h), BAR_WIDTH / 2.0), white(alpha), ts);
        }
    }

    fn paint_badge(&self, pm: &mut Pixmap, top: f32, ts: Transform) {
        let ms = self.phase_ms() - MORPH_MS / 2.0;
        if ms <= 0.0 {
            return;
        }
        let grow = gfx::out_back((ms / BADGE_IN_MS).min(1.0));
        let (cx, cy) = (INSET + WIDTH / 2.0, top + HEIGHT / 2.0);
        let color = if self.ok { gfx::rgba(52, 199, 89, 255) } else { gfx::rgba(255, 159, 10, 255) };
        gfx::fill(pm, &circle(cx, cy, BADGE_R * grow), color, ts);
        let draw = gfx::out_cubic(((ms - BADGE_IN_MS * 0.55) / CHECK_DRAW_MS).clamp(0.0, 1.0));
        if draw <= 0.0 {
            return;
        }
        if self.ok {
            // the check, drawn stroke by stroke
            let pts = [(-5.8f32, 0.4f32), (-1.9, 4.3), (6.0, -4.4)];
            let lens: Vec<f32> = pts.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).collect();
            let mut left = draw * lens.iter().sum::<f32>();
            let mut pb = PathBuilder::new();
            pb.move_to(cx + pts[0].0, cy + pts[0].1);
            for (i, seg) in lens.iter().enumerate() {
                let (a, b) = (pts[i], pts[i + 1]);
                let t = (left / seg).min(1.0);
                pb.line_to(cx + a.0 + (b.0 - a.0) * t, cy + a.1 + (b.1 - a.1) * t);
                left -= seg;
                if left <= 0.0 {
                    break;
                }
            }
            gfx::stroke(pm, &pb.finish(), white(255), 2.6, true, ts);
        } else {
            // clipboard: the text is waiting there for a manual Ctrl+V
            let k = 0.6 + 0.4 * draw;
            let t2 = ts.pre_translate(cx, cy).pre_scale(k, k);
            let a = (255.0 * draw) as u8;
            gfx::stroke(pm, &rounded(R::new(-6.0, -6.0, 12.0, 14.5), 2.2), white(a), 2.0, true, t2);
            gfx::fill(pm, &rounded(R::new(-3.4, -8.4, 6.8, 4.4), 1.4), white(a), t2);
            gfx::line(pm, -2.8, 0.2, 2.8, 0.2, white(a), 1.7, true, t2);
            gfx::line(pm, -2.8, 3.8, 1.2, 3.8, white(a), 1.7, true, t2);
        }
    }

    fn paint_gear(&self, pm: &mut Pixmap, top: bool, opacity: f32, ts: Transform) {
        let (cx, cy) = self.gear_center(top);
        let alpha = ((150.0 + 105.0 * self.gear_hover_t) * opacity) as u8;
        let scale = 1.0 + 0.12 * self.gear_hover_t;
        let t = ts.pre_translate(cx, cy).pre_rotate(self.gear_angle).pre_scale(scale, scale);
        gfx::fill(pm, &GEAR, white(alpha), t);
    }

}

/// The pill window's origin and scale for `place` (see there).
fn home(top: bool, saved: Option<(i32, i32)>) -> (i32, i32, f32) {
    let (lw, lh) = Pill::logical_size();
    let size = |s: f32| ((lw * s).ceil() as i32, (lh * s).ceil() as i32);
    if let Some((x, y)) = saved {
        let m = window::monitor_at(x, y, true).unwrap();
        let (w, h) = size(m.scale);
        if window::monitor_at(x + w / 2, y + h / 2, false).is_some() {
            return (x, y, m.scale);
        }
    }
    let m = window::primary();
    let s = m.scale;
    let (w, h) = size(s);
    let work = m.work;
    let x = work.left + ((work.right - work.left) - w) / 2;
    let y = if top {
        work.top + ((EDGE_MARGIN - SLIDE_PX - INSET) * s).round() as i32
    } else {
        work.bottom - h - ((EDGE_MARGIN - INSET) * s).round() as i32
    };
    (x, y, s)
}

static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// Circular body + rounded teeth minus the central hole (overlay.py:
/// _gear_path). Nonzero winding: body and teeth run clockwise, the hole
/// counter-clockwise, so the hole cancels out and overlaps don't.
static GEAR: std::sync::LazyLock<Option<tiny_skia::Path>> = std::sync::LazyLock::new(|| {
    let mut pb = PathBuilder::new();
    push_circle(&mut pb, GEAR_BODY_R, true);
    let tooth_len = GEAR_OUTER_R - GEAR_BODY_R + 2.6;
    let tooth = rounded(R::new(-GEAR_TOOTH_W / 2.0, -GEAR_OUTER_R, GEAR_TOOTH_W, tooth_len), 1.5)?;
    for k in 0..GEAR_TEETH {
        if let Some(p) = tooth.clone().transform(Transform::from_rotate(k as f32 * 360.0 / GEAR_TEETH as f32)) {
            pb.push_path(&p);
        }
    }
    push_circle(&mut pb, GEAR_HOLE_R, false);
    pb.finish()
});

fn push_circle(pb: &mut PathBuilder, r: f32, clockwise: bool) {
    let k = 0.552_284_8 * r;
    let d = if clockwise { 1.0 } else { -1.0 };
    pb.move_to(r, 0.0);
    pb.cubic_to(r, d * k, k, d * r, 0.0, d * r);
    pb.cubic_to(-k, d * r, -r, d * k, -r, 0.0);
    pb.cubic_to(-r, -d * k, -k, -d * r, 0.0, -d * r);
    pb.cubic_to(k, -d * r, r, -d * k, r, 0.0);
    pb.close();
}
