//! Per-pixel-alpha popup windows that never take the focus: the pill and
//! the panels. Painted into a tiny-skia pixmap and pushed with
//! UpdateLayeredWindow; opacity is the layered blend alpha.

use tiny_skia::Pixmap;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WDA_NONE: u32 = 0x00;
pub const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct Layered {
    pub hwnd: HWND,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub opacity: f32,
    pub visible: bool,
}

impl Layered {
    /// `class` must be registered with the app's window procedure; `id` goes
    /// in GWLP_USERDATA so the procedure knows which window it is.
    pub fn create(class: &[u16], id: isize) -> Self {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                class.as_ptr(),
                wide("claudeTalk").as_ptr(),
                WS_POPUP,
                0,
                0,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, id) };
        Self { hwnd, x: 0, y: 0, w: 1, h: 1, opacity: 0.0, visible: false }
    }

    /// Invisible to screen capture (so it can grab what's behind it), or
    /// visible in screen sharing when the user asked for it.
    pub fn set_capturable(&self, capturable: bool) {
        unsafe { SetWindowDisplayAffinity(self.hwnd, if capturable { WDA_NONE } else { WDA_EXCLUDEFROMCAPTURE }) };
    }

    pub fn present(&mut self, pm: &Pixmap) {
        let (w, h) = (pm.width(), pm.height());
        self.w = w;
        self.h = h;
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
            if !dib.is_null() {
                // tiny-skia is premultiplied RGBA, GDI wants premultiplied BGRA
                let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (w * h * 4) as usize);
                for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
                    d[0] = s[2];
                    d[1] = s[1];
                    d[2] = s[0];
                    d[3] = s[3];
                }
                let old = SelectObject(mem, dib);
                let pos = POINT { x: self.x, y: self.y };
                let size = SIZE { cx: w as i32, cy: h as i32 };
                let src = POINT { x: 0, y: 0 };
                let blend = BLENDFUNCTION {
                    BlendOp: AC_SRC_OVER as u8,
                    BlendFlags: 0,
                    SourceConstantAlpha: (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
                    AlphaFormat: AC_SRC_ALPHA as u8,
                };
                UpdateLayeredWindow(self.hwnd, screen, &pos, &size, mem, &src, 0, &blend, ULW_ALPHA);
                SelectObject(mem, old);
                DeleteObject(dib);
            }
            DeleteDC(mem);
            ReleaseDC(std::ptr::null_mut(), screen);
        }
    }

    pub fn show(&mut self) {
        if !self.visible {
            unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            unsafe { SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) };
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        if self.visible {
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
            self.visible = false;
        }
    }

}

/// Work area (without the taskbar) and DPI scale of the monitor at a point.
#[derive(Clone, Copy)]
pub struct Monitor {
    pub work: RECT,
    pub scale: f32,
}

pub fn monitor_at(x: i32, y: i32, nearest: bool) -> Option<Monitor> {
    unsafe {
        let flag = if nearest { MONITOR_DEFAULTTONEAREST } else { MONITOR_DEFAULTTONULL };
        let hm = MonitorFromPoint(POINT { x, y }, flag);
        if hm.is_null() {
            return None;
        }
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(hm, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        GetDpiForMonitor(hm, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        Some(Monitor { work: mi.rcWork, scale: dx as f32 / 96.0 })
    }
}

pub fn primary() -> Monitor {
    monitor_at(0, 0, true).unwrap_or(Monitor { work: RECT { left: 0, top: 0, right: 1920, bottom: 1080 }, scale: 1.0 })
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut p) };
    (p.x, p.y)
}
