//! Tray icon and its menu (daemon_cli.py): a disabled line with the chord,
//! "Settings", "Turn off dictation" (asks first). The icon is the same
//! drawing as the Start Menu one (voice-input/icon.py).

use super::gfx::{self, rounded, R};
use super::window::wide;
use tiny_skia::{Pixmap, Transform};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const MSG_TRAY: u32 = WM_APP + 10;
pub const CMD_SETTINGS: usize = 2;
pub const CMD_QUIT: usize = 3;

/// White rounded square with five black bars.
pub fn paint_icon(size: u32) -> Pixmap {
    let mut pm = Pixmap::new(size, size).unwrap();
    let s = size as f32;
    let margin = (s / 16.0).round().max(1.0);
    let radius = (s / 4.0).round().max(2.0);
    let shape = rounded(R::new(margin, margin, s - 2.0 * margin, s - 2.0 * margin), radius);
    gfx::fill(&mut pm, &shape, gfx::white(255), Transform::identity());
    gfx::stroke(&mut pm, &shape, gfx::black(60), 1.0, false, Transform::identity());
    let heights: Vec<f32> = [16.0, 26.0, 36.0, 26.0, 16.0].iter().map(|h| s * h / 64.0).collect();
    let (bar_w, gap) = (s * 6.0 / 64.0, s * 5.0 / 64.0);
    let total = heights.len() as f32 * bar_w + (heights.len() - 1) as f32 * gap;
    let mut x = (s - total) / 2.0;
    for h in heights {
        gfx::fill(&mut pm, &rounded(R::new(x, s / 2.0 - h / 2.0, bar_w, h), bar_w / 2.0), gfx::black(255), Transform::identity());
        x += bar_w + gap;
    }
    pm
}

pub fn hicon(size: u32) -> HICON {
    let pm = paint_icon(size);
    unsafe {
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = size as i32;
        bmi.bmiHeader.biHeight = -(size as i32);
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        let dc = GetDC(std::ptr::null_mut());
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        ReleaseDC(std::ptr::null_mut(), dc);
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, (size * size * 4) as usize);
        for (d, s) in dst.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            d[0] = s[2];
            d[1] = s[1];
            d[2] = s[0];
            d[3] = s[3];
        }
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, std::ptr::null());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    pub label: String,
}

impl Tray {
    pub fn new(hwnd: HWND) -> Self {
        let size = unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16) as u32;
        Self { hwnd, icon: hicon(size * 2), label: String::new() }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = MSG_TRAY;
        nid.hIcon = self.icon;
        copy_wide(&mut nid.szTip, &format!("claudeTalk dictation - {}", self.label));
        nid
    }

    pub fn show(&self) {
        let nid = self.data();
        unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = label.to_string();
        let nid = self.data();
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }

    /// Explorer restarted: the icon has to be added again.
    pub fn readd(&self) {
        self.show();
    }

    pub fn remove(&self) {
        let nid = self.data();
        unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }

    /// Shows the menu at the cursor; returns the chosen command (0 = none).
    pub fn menu(&self) -> usize {
        unsafe {
            let m = CreatePopupMenu();
            AppendMenuW(m, MF_STRING | MF_GRAYED, 1, wide(&format!("Dictation: {}", self.label)).as_ptr());
            AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(m, MF_STRING, CMD_SETTINGS, wide("Settings").as_ptr());
            AppendMenuW(m, MF_STRING, CMD_QUIT, wide("Turn off dictation").as_ptr());
            let mut p = POINT { x: 0, y: 0 };
            GetCursorPos(&mut p);
            // Required so the menu closes when clicking elsewhere.
            SetForegroundWindow(self.hwnd);
            let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, p.x, p.y, 0, self.hwnd, std::ptr::null());
            PostMessageW(self.hwnd, WM_NULL, 0, 0);
            DestroyMenu(m);
            cmd as usize
        }
    }
}

/// "Turn off dictation completely?" Yes / No (No by default).
pub fn confirm_quit() -> bool {
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide("Turn off dictation completely?").as_ptr(),
            wide("claudeTalk").as_ptr(),
            MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST,
        ) == IDYES
    }
}
