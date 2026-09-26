//! A one-line field over a text row's chip (overlay.py: _TextEditor). The
//! panels never take the focus, so they can't be typed into: this small
//! window can. Enter or clicking away saves, Esc drops the change.

use super::window::wide;
use crate::inject;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const BG: u32 = 0x0031_2d2b; // #2b2d31 as COLORREF (0x00BBGGRR)
const BORDER: u32 = 0x0093_8d8a; // #8a8d93
const EM_LIMITTEXT: u32 = 0x00C5;
const EM_SETSEL: u32 = 0x00B1;
const EN_KILLFOCUS: u32 = 0x0200;

pub struct Editor {
    pub host: HWND,
    pub edit: HWND,
    pub key: Option<String>,
    brush: HBRUSH,
    font: HFONT,
}

/// Messages the edit control sends back to the app's window procedure.
pub const MSG_COMMIT: u32 = WM_APP + 20;
pub const MSG_CANCEL: u32 = WM_APP + 21;

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, host: usize) -> LRESULT {
    if msg == WM_KEYDOWN && (wp as u16 == 0x0D || wp as u16 == 0x1B) {
        PostMessageW(host as HWND, if wp as u16 == 0x0D { MSG_COMMIT } else { MSG_CANCEL }, 0, 0);
        return 0;
    }
    if msg == WM_CHAR && (wp == 0x0D || wp == 0x1B) {
        return 0; // no beep
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

impl Editor {
    /// `host` is a popup of the app's window class (id in GWLP_USERDATA).
    pub fn create(class: &[u16], id: isize) -> Self {
        unsafe {
            let host = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class.as_ptr(),
                wide("claudeTalk").as_ptr(),
                WS_POPUP,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            SetWindowLongPtrW(host, GWLP_USERDATA, id);
            let edit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | ES_AUTOHSCROLL as u32,
                0,
                0,
                10,
                10,
                host,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            SendMessageW(edit, EM_LIMITTEXT, 40, 0);
            SetWindowSubclass(edit, Some(edit_proc), 1, host as usize);
            Self { host, edit, key: None, brush: CreateSolidBrush(BG), font: std::ptr::null_mut() }
        }
    }

    pub fn open(&mut self, key: &str, value: &str, x: i32, y: i32, w: i32, h: i32, scale: f32) {
        self.key = Some(key.to_string());
        unsafe {
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            // Segoe UI 9 pt
            self.font = CreateFontW(-((9.0 * 96.0 / 72.0 * scale).round() as i32), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, wide("Segoe UI").as_ptr());
            SendMessageW(self.edit, WM_SETFONT, self.font as WPARAM, 1);
            let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, (16.0 * scale) as i32, (16.0 * scale) as i32);
            SetWindowRgn(self.host, rgn, 1);
            let pad = (8.0 * scale) as i32;
            let text_h = (16.0 * scale) as i32;
            SetWindowPos(self.host, HWND_TOPMOST, x, y, w, h, SWP_SHOWWINDOW);
            MoveWindow(self.edit, pad, (h - text_h) / 2, w - 2 * pad, text_h, 1);
            SetWindowTextW(self.edit, wide(value).as_ptr());
            SendMessageW(self.edit, EM_SETSEL, 0, -1);
            // Windows only hands the focus to the process in front; take it
            // the way the paste does.
            inject::focus(self.host);
            SetFocus(self.edit);
        }
    }

    pub fn text(&self) -> String {
        unsafe {
            let len = GetWindowTextLengthW(self.edit);
            let mut buf = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(self.edit, buf.as_mut_ptr(), buf.len() as i32);
            String::from_utf16_lossy(&buf[..n.max(0) as usize])
        }
    }

    /// Closes the field; returns (key, text) when the change is kept.
    pub fn finish(&mut self, commit: bool) -> Option<(String, String)> {
        let key = self.key.take()?;
        let text = self.text();
        unsafe { ShowWindow(self.host, SW_HIDE) };
        commit.then_some((key, text))
    }

    /// Host window messages; Some(result) when handled.
    pub fn handle(&mut self, msg: u32, wp: WPARAM, _lp: LPARAM) -> Option<LRESULT> {
        unsafe {
            match msg {
                WM_CTLCOLOREDIT => {
                    let dc = wp as HDC;
                    SetTextColor(dc, 0x00FF_FFFF);
                    SetBkColor(dc, BG);
                    Some(self.brush as LRESULT)
                }
                WM_ERASEBKGND => Some(1),
                WM_PAINT => {
                    let mut ps: PAINTSTRUCT = std::mem::zeroed();
                    let dc = BeginPaint(self.host, &mut ps);
                    let mut rc: RECT = std::mem::zeroed();
                    GetClientRect(self.host, &mut rc);
                    let pen = CreatePen(PS_SOLID, 1, BORDER);
                    let old_pen = SelectObject(dc, pen);
                    let old_brush = SelectObject(dc, self.brush);
                    RoundRect(dc, 0, 0, rc.right, rc.bottom, 16, 16);
                    SelectObject(dc, old_pen);
                    SelectObject(dc, old_brush);
                    DeleteObject(pen);
                    EndPaint(self.host, &ps);
                    Some(0)
                }
                WM_COMMAND if (wp >> 16) as u32 == EN_KILLFOCUS => {
                    PostMessageW(self.host, MSG_COMMIT, 0, 0);
                    Some(0)
                }
                _ => None,
            }
        }
    }
}
