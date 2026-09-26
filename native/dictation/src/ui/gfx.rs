//! Drawing helpers on tiny-skia: shapes the way QPainterPath builds them,
//! colors, easing curves, and Segoe UI text through ab_glyph (Qt metrics:
//! point sizes at 96 dpi, vertical centering on ascent + descent).

use ab_glyph::{Font, FontArc, GlyphId, PxScale, ScaleFont};
use std::sync::OnceLock;
use tiny_skia::{FillRule, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

pub type Color = tiny_skia::Color;

pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

pub fn white(a: u8) -> Color {
    rgba(255, 255, 255, a)
}

pub fn black(a: u8) -> Color {
    rgba(0, 0, 0, a)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// --- easing (Qt's curves) -------------------------------------------------

pub fn out_cubic(t: f32) -> f32 {
    let u = 1.0 - t;
    1.0 - u * u * u
}

pub fn in_cubic(t: f32) -> f32 {
    t * t * t
}

pub fn in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

pub fn out_back(t: f32) -> f32 {
    let s = 1.70158;
    let u = t - 1.0;
    u * u * ((s + 1.0) * u + s) + 1.0
}

// --- shapes ----------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }
    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }
    pub fn adjusted(&self, l: f32, t: f32, r: f32, b: f32) -> Self {
        Self::new(self.x + l, self.y + t, self.w - l + r, self.h - t + b)
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

/// QPainterPath::addRoundedRect (radii clamped to half the side).
pub fn rounded(r: R, radius: f32) -> Option<Path> {
    if r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    let rad = radius.max(0.0).min(r.w / 2.0).min(r.h / 2.0);
    let mut pb = PathBuilder::new();
    let k = 0.552_284_8 * rad;
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    pb.move_to(x0 + rad, y0);
    pb.line_to(x1 - rad, y0);
    pb.cubic_to(x1 - rad + k, y0, x1, y0 + rad - k, x1, y0 + rad);
    pb.line_to(x1, y1 - rad);
    pb.cubic_to(x1, y1 - rad + k, x1 - rad + k, y1, x1 - rad, y1);
    pb.line_to(x0 + rad, y1);
    pb.cubic_to(x0 + rad - k, y1, x0, y1 - rad + k, x0, y1 - rad);
    pb.line_to(x0, y0 + rad);
    pb.cubic_to(x0, y0 + rad - k, x0 + rad - k, y0, x0 + rad, y0);
    pb.close();
    pb.finish()
}

pub fn circle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    if r <= 0.0 {
        return None;
    }
    PathBuilder::from_circle(cx, cy, r)
}

pub fn paint(color: Color) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color);
    p.anti_alias = true;
    p
}

pub fn fill(pm: &mut Pixmap, path: &Option<Path>, color: Color, ts: Transform) {
    if let Some(p) = path {
        pm.fill_path(p, &paint(color), FillRule::Winding, ts, None);
    }
}

pub fn stroke(pm: &mut Pixmap, path: &Option<Path>, color: Color, width: f32, round: bool, ts: Transform) {
    if let Some(p) = path {
        let mut s = Stroke { width, ..Default::default() };
        if round {
            s.line_cap = tiny_skia::LineCap::Round;
            s.line_join = tiny_skia::LineJoin::Round;
        }
        pm.stroke_path(p, &paint(color), &s, ts, None);
    }
}

pub fn line(pm: &mut Pixmap, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, width: f32, round: bool, ts: Transform) {
    let mut pb = PathBuilder::new();
    pb.move_to(x0, y0);
    pb.line_to(x1, y1);
    stroke(pm, &pb.finish(), color, width, round, ts);
}

// --- text ------------------------------------------------------------------

pub struct Fonts {
    pub regular: FontArc,
    pub semibold: FontArc,
}

pub fn fonts() -> &'static Fonts {
    static F: OnceLock<Fonts> = OnceLock::new();
    F.get_or_init(|| {
        let dir = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into())).join("Fonts");
        let load = |names: &[&str]| -> FontArc {
            for n in names {
                if let Ok(bytes) = std::fs::read(dir.join(n)) {
                    if let Ok(f) = FontArc::try_from_vec(bytes) {
                        return f;
                    }
                }
            }
            panic!("no UI font found in {}", dir.display());
        };
        Fonts { regular: load(&["segoeui.ttf", "arial.ttf"]), semibold: load(&["seguisb.ttf", "segoeui.ttf", "arialbd.ttf"]) }
    })
}

#[derive(Clone, Copy)]
pub enum Weight {
    Regular,
    Semibold,
}

/// A font at a Qt point size, in logical pixels (96 dpi).
pub struct TextStyle {
    pub pt: f32,
    pub weight: Weight,
}

impl TextStyle {
    pub fn new(pt: f32) -> Self {
        Self { pt, weight: Weight::Regular }
    }
    pub fn bold(pt: f32) -> Self {
        Self { pt, weight: Weight::Semibold }
    }
    fn font(&self) -> &'static FontArc {
        match self.weight {
            Weight::Regular => &fonts().regular,
            Weight::Semibold => &fonts().semibold,
        }
    }
    /// Qt: pixel size = point size * 96 / 72.
    fn px(&self) -> f32 {
        self.pt * 96.0 / 72.0
    }
}

fn scaled(style: &TextStyle, scale: f32) -> ab_glyph::PxScaleFont<&'static FontArc> {
    let f = style.font();
    // ab_glyph's PxScale is the height of ascent - descent; Qt's pixel size
    // is the em size. Convert em -> height.
    let em = style.px() * scale;
    let units = f.units_per_em().unwrap_or(2048.0);
    let height_units = f.ascent_unscaled() - f.descent_unscaled();
    f.as_scaled(PxScale::from(em * height_units / units))
}

/// Horizontal advance of `text`, in logical px.
pub fn text_width(text: &str, style: &TextStyle) -> f32 {
    let sf = scaled(style, 1.0);
    let mut w = 0.0;
    let mut prev: Option<GlyphId> = None;
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id);
        prev = Some(id);
    }
    w
}

/// QFontMetricsF::elidedText(..., ElideRight, width).
pub fn elide(text: &str, style: &TextStyle, max_w: f32) -> String {
    if text_width(text, style) <= max_w {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for n in (0..chars.len()).rev() {
        let s: String = chars[..n].iter().collect::<String>() + "\u{2026}";
        if text_width(&s, style) <= max_w {
            return s;
        }
    }
    "\u{2026}".into()
}

#[derive(Clone, Copy, PartialEq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, PartialEq)]
pub enum VAlign {
    Center,
    Bottom,
}

/// Draws `text` inside `r` (logical px) at `scale`, offset by (ox, oy) in
/// physical px. Glyph coverage is blended straight into the pixmap.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(pm: &mut Pixmap, text: &str, r: R, style: &TextStyle, color: Color, h: HAlign, v: VAlign, scale: f32, ox: f32, oy: f32) {
    let sf = scaled(style, scale);
    let width = text_width(text, style) * scale;
    let (ascent, descent) = (sf.ascent(), sf.descent()); // descent < 0
    let x0 = match h {
        HAlign::Left => r.x * scale,
        HAlign::Center => r.cx() * scale - width / 2.0,
        HAlign::Right => r.right() * scale - width,
    } + ox;
    let baseline = match v {
        VAlign::Center => r.y * scale + (r.h * scale - (ascent - descent)) / 2.0 + ascent,
        VAlign::Bottom => r.bottom() * scale + descent,
    } + oy;
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let c = color.premultiply().to_color_u8();
    let data = pm.data_mut();
    let mut x = x0;
    let mut prev: Option<GlyphId> = None;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(sf.scale, ab_glyph::point(x, baseline));
        x += sf.h_advance(id);
        prev = Some(id);
        let Some(outline) = sf.outline_glyph(glyph) else { continue };
        let b = outline.px_bounds();
        outline.draw(|gx, gy, cov| {
            let px = b.min.x as i32 + gx as i32;
            let py = b.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= pw || py >= ph || cov <= 0.0 {
                return;
            }
            let i = (py as usize * pw as usize + px as usize) * 4;
            let a = cov.min(1.0);
            let src = [c.red() as f32 * a, c.green() as f32 * a, c.blue() as f32 * a, c.alpha() as f32 * a];
            let inv = 1.0 - src[3] / 255.0;
            for k in 0..4 {
                data[i + k] = (src[k] + data[i + k] as f32 * inv).round().clamp(0.0, 255.0) as u8;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_end_at_one() {
        for f in [out_cubic, in_cubic, in_out_cubic, out_back] {
            assert!((f(1.0) - 1.0).abs() < 1e-6);
            assert!(f(0.0).abs() < 1e-6);
        }
        assert!(out_back(0.7) > 1.0); // overshoots
    }

    #[test]
    fn text_measures() {
        let w = text_width("Dictation", &TextStyle::bold(11.0));
        assert!(w > 50.0 && w < 90.0, "{w}");
        assert!(elide("una frase bastante larga para el chip", &TextStyle::new(9.0), 80.0).ends_with('\u{2026}'));
    }
}
