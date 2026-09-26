//! The "liquid glass" material (overlay.py: _glassify, _paint_shadow,
//! _paint_glass). The window is excluded from screen capture, so it can
//! grab what is behind itself, blur it with color kept and saturation
//! boosted, tint it, and add edge lensing, rims and a specular line. The
//! capture and blur run in native code now: well under a millisecond for
//! the pill, and skipped entirely when the backdrop didn't change.

use super::gfx::{self, black, rounded, white, Color, R};
use tiny_skia::{FillRule, Mask, PathBuilder, Pixmap, PixmapPaint, Transform};
use windows_sys::Win32::Graphics::Gdi::*;

pub const INSET: f32 = 16.0;
const SHADOW_OFFSET_Y: f32 = 4.0;
const SHADOW_SPREAD: i32 = 12;
const LENS_BAND_PX: f32 = 9.0;
const LENS_SCALE: f32 = 1.07;

/// "Glass" slider 0..100 -> the material's parameters.
#[derive(Clone, Copy)]
pub struct Material {
    pub glass: f32, // 0..1
}

impl Material {
    pub fn blur_px(&self) -> i32 {
        gfx::lerp(10.0, 36.0, self.glass).round() as i32
    }
    pub fn tint_alpha(&self) -> u8 {
        gfx::lerp(90.0, 0.0, self.glass).round() as u8
    }
    pub fn saturation(&self) -> f32 {
        gfx::lerp(1.15, 1.5, self.glass)
    }
}

/// Screen pixels under a physical rect, as an opaque RGBA pixmap. Only shows
/// what is behind our window because it's excluded from capture (or not yet
/// on screen).
pub fn capture(x: i32, y: i32, w: u32, h: u32) -> Option<Pixmap> {
    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w as i32;
        bmi.bmiHeader.biHeight = -(h as i32);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        let mut out = None;
        if !dib.is_null() {
            let old = SelectObject(mem, dib);
            if BitBlt(mem, 0, 0, w as i32, h as i32, screen, x, y, SRCCOPY | CAPTUREBLT) != 0 {
                let src = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
                if let Some(mut pm) = Pixmap::new(w, h) {
                    for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                        d[0] = s[2];
                        d[1] = s[1];
                        d[2] = s[0];
                        d[3] = 255;
                    }
                    out = Some(pm);
                }
            }
            SelectObject(mem, old);
            DeleteObject(dib);
        }
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
        out
    }
}

/// Separable box blur, edge-replicated, window of 2r samples
/// (numpy version in overlay.py:_box_blur).
fn box_blur(buf: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        return;
    }
    let mut tmp = vec![0.0f32; buf.len()];
    let n = 2 * r;
    let pass = |src: &[f32], dst: &mut [f32], len: usize, stride: usize, count: usize, step: usize| {
        for line in 0..count {
            let base = line * step;
            for c in 0..3 {
                let at = |i: isize| src[base + (i.clamp(0, len as isize - 1) as usize) * stride + c];
                let mut sum: f32 = (1..=n as isize).map(|k| at(k - r as isize)).sum();
                for j in 0..len {
                    dst[base + j * stride + c] = sum / n as f32;
                    // slide: add j+1+r, drop j+1-r
                    sum += at(j as isize + 1 + r as isize) - at(j as isize + 1 - r as isize);
                }
            }
        }
    };
    pass(buf, &mut tmp, h, w * 3, w, 3); // vertical: along columns
    pass(&tmp, buf, w, 3, h, w * 3); // horizontal: along rows
}

/// Capture -> glass backdrop: 1/3 resolution, three box passes (close to a
/// gaussian), saturation boost, darkened to 80 %, scaled back up.
pub fn glassify(raw: &Pixmap, m: Material) -> Pixmap {
    let (w, h) = (raw.width() as usize, raw.height() as usize);
    let (sw, sh) = ((w / 3).max(1), (h / 3).max(1));
    let src = raw.data();
    let mut small = vec![0.0f32; sw * sh * 3];
    for y in 0..sh {
        for x in 0..sw {
            let (mut acc, mut n) = ([0.0f32; 3], 0.0f32);
            for yy in y * h / sh..((y + 1) * h / sh).max(y * h / sh + 1) {
                for xx in x * w / sw..((x + 1) * w / sw).max(x * w / sw + 1) {
                    let i = (yy.min(h - 1) * w + xx.min(w - 1)) * 4;
                    for c in 0..3 {
                        acc[c] += src[i + c] as f32;
                    }
                    n += 1.0;
                }
            }
            for c in 0..3 {
                small[(y * sw + x) * 3 + c] = acc[c] / n;
            }
        }
    }
    let r = (m.blur_px() / 3).max(1) as usize;
    for _ in 0..3 {
        box_blur(&mut small, sw, sh, r);
    }
    let sat = m.saturation();
    for px in small.chunks_exact_mut(3) {
        let gray = 0.299 * px[0] + 0.587 * px[1] + 0.114 * px[2];
        for c in px.iter_mut() {
            *c = ((gray + (*c - gray) * sat) * 0.8).clamp(0.0, 255.0).trunc();
        }
    }
    // bilinear back up to full size
    let mut out = Pixmap::new(w as u32, h as u32).unwrap();
    let d = out.data_mut();
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy.floor() as usize, fy - fy.floor());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx.floor() as usize, fx - fx.floor());
            let x1 = (x0 + 1).min(sw - 1);
            let i = (y * w + x) * 4;
            for c in 0..3 {
                let p = |yy: usize, xx: usize| small[(yy * sw + xx) * 3 + c];
                let top = p(y0, x0) + (p(y0, x1) - p(y0, x0)) * tx;
                let bot = p(y1, x0) + (p(y1, x1) - p(y1, x0)) * tx;
                d[i + c] = (top + (bot - top) * ty).round() as u8;
            }
            d[i + 3] = 255;
        }
    }
    out
}

/// A cheap fingerprint of a capture, to skip re-blurring an unchanged one.
pub fn fingerprint(pm: &Pixmap) -> u64 {
    let d = pm.data();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in d.chunks(4).step_by(7) {
        h ^= u32::from_le_bytes([chunk[0], chunk[1], chunk[2], 0]) as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

fn shadow(pm: &mut Pixmap, rect: R, radius: f32, ts: Transform) {
    let layers = SHADOW_SPREAD;
    for k in (0..=layers).rev() {
        let t = (layers - k) as f32 / layers as f32;
        let alpha = (60.0 * t * t / 3.0) as u8 + 1;
        let kf = k as f32;
        let r = rect.adjusted(-kf, -kf + SHADOW_OFFSET_Y, kf, kf + SHADOW_OFFSET_Y);
        gfx::fill(pm, &rounded(r, radius + kf), black(alpha), ts);
    }
}

fn mask_of(pm: &Pixmap, paths: &[&Option<tiny_skia::Path>], rule: FillRule, ts: Transform) -> Option<Mask> {
    let mut mask = Mask::new(pm.width(), pm.height())?;
    let mut pb = PathBuilder::new();
    for p in paths.iter().filter_map(|p| p.as_ref()) {
        pb.push_path(p);
    }
    let path = pb.finish()?;
    mask.fill_path(&path, rule, true, ts);
    Some(mask)
}

/// The whole material for a rounded shape at `rect` (logical px, scaled by
/// `s`). `bg` is the glassified backdrop, full window size in physical px.
pub fn paint_glass(pm: &mut Pixmap, rect: R, radius: f32, bg: Option<&Pixmap>, m: Material, veil: Option<Color>, s: f32) {
    let ts = Transform::from_scale(s, s);
    shadow(pm, rect, radius, ts);
    let shape = rounded(rect, radius);
    if let Some(bg) = bg {
        let clip = mask_of(pm, &[&shape], FillRule::Winding, ts);
        pm.draw_pixmap(0, 0, bg.as_ref(), &PixmapPaint::default(), Transform::identity(), clip.as_ref());
        // edge lensing: the backdrop slightly magnified in the outer band
        let inner = rounded(
            rect.adjusted(LENS_BAND_PX, LENS_BAND_PX, -LENS_BAND_PX, -LENS_BAND_PX),
            radius - LENS_BAND_PX,
        );
        let ring = mask_of(pm, &[&shape, &inner], FillRule::EvenOdd, ts);
        let (cx, cy) = (rect.cx() * s, rect.cy() * s);
        let lens = Transform::from_translate(cx, cy).pre_scale(LENS_SCALE, LENS_SCALE).pre_translate(-cx, -cy);
        let paint = PixmapPaint { opacity: 0.7, quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        pm.draw_pixmap(0, 0, bg.as_ref(), &paint, lens, ring.as_ref());
        // veil after the band, so the band is tinted like the rest
        gfx::fill(pm, &shape, veil.unwrap_or(black(m.tint_alpha())), ts);
    } else {
        gfx::fill(pm, &shape, black(235), ts);
    }
    gfx::stroke(pm, &shape, white(120), 1.0, false, ts);
    let inner_line = rounded(rect.adjusted(1.0, 1.0, -1.0, -1.0), radius - 1.0);
    gfx::stroke(pm, &inner_line, black(70), 1.0, false, ts);
    // specular highlight along the top third
    if let Some(top) = mask_of(
        pm,
        &[&tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h * 0.34).map(PathBuilder::from_rect)],
        FillRule::Winding,
        ts,
    ) {
        if let Some(p) = &inner_line {
            let stroke = tiny_skia::Stroke { width: 1.4, ..Default::default() };
            pm.stroke_path(p, &gfx::paint(white(150)), &stroke, ts, Some(&top));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_blur_is_two_dimensional() {
        let (w, h) = (31usize, 21usize);
        let mut buf = vec![0.0f32; w * h * 3];
        buf[(10 * w + 15) * 3] = 1000.0;
        for _ in 0..3 {
            box_blur(&mut buf, w, h, 2);
        }
        let at = |x: usize, y: usize| buf[(y * w + x) * 3];
        assert!((at(15, 13) - at(18, 10)).abs() < 1e-3, "{} vs {}", at(15, 13), at(18, 10));
        assert!(at(15, 13) > 0.0);
        let total: f32 = buf.chunks(3).map(|p| p[0]).sum();
        assert!((total - 1000.0).abs() < 1.0);
    }

    #[test]
    fn box_blur_matches_numpy_window() {
        // one channel spread over 3: a single bright pixel in a row of 7
        let (w, h, r) = (7usize, 1usize, 1usize);
        let mut buf = vec![0.0f32; w * h * 3];
        buf[3 * 3] = 90.0;
        let mut row = buf.clone();
        box_blur(&mut row, w, h, r);
        // window of 2r = 2 samples: offsets (j-r+1 .. j+r) = (j, j+1)
        let got: Vec<f32> = row.chunks(3).map(|p| p[0]).collect();
        assert_eq!(got, vec![0.0, 0.0, 45.0, 45.0, 0.0, 0.0, 0.0]);
    }
}
