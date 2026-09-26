//! Getting the text where it belongs (voice-input/inject.py).
//!
//! Clipboard + one simulated Ctrl+V: atomic and Unicode-safe, where typing
//! key by key could land half the text in another window. The text is
//! always left on the clipboard as a safety net.
//!
//! For "Oye Claude" (and dictations started on the desktop) the text goes
//! to the last Claude Code session wherever the focus is: first by writing
//! key events straight into that claude.exe's console input buffer (no
//! focus change at all, works behind a fullscreen game and in any
//! terminal); if that fails, by jumping to the terminal kept fully
//! transparent, finding the tab, pasting, and putting everything back.

use std::sync::Mutex;
use std::thread::sleep;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, OPEN_EXISTING};
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::Memory::*;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;
use windows_sys::Win32::Graphics::Gdi::{RedrawWindow, RDW_ALLCHILDREN, RDW_FRAME, RDW_INVALIDATE};

const VK_CONTROL_: u16 = 0x11;
const VK_SHIFT_: u16 = 0x10;
const VK_MENU_: u16 = 0x12;
const VK_LWIN_: u16 = 0x5B;
const VK_V: u16 = 0x56;
const VK_RETURN_: u16 = 0x0D;
const VK_TAB_: u16 = 0x09;
const ENTER_DELAY: Duration = Duration::from_millis(80);
const PASTE_DELAY_BEFORE: Duration = Duration::from_millis(30);
const PASTE_DELAY_AFTER: Duration = Duration::from_millis(150);
const FOCUS_WAIT: Duration = Duration::from_millis(400);
const TAB_WAIT: Duration = Duration::from_millis(400);
const MAX_TABS: usize = 15;
const CONSOLE_ENTER_DELAY: Duration = Duration::from_millis(250);
const CF_UNICODETEXT_: u32 = 13;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

fn key_down(vk: u16) -> bool {
    crate::hotkey::is_key_down(vk)
}

/// Several key events in ONE SendInput call, so no physical key sneaks in
/// the middle. Returns how many Windows accepted (0 = rejected, e.g. UIPI
/// when the target runs elevated): the only signal a paste won't arrive.
fn send_keys(events: &[(u16, bool)]) -> u32 {
    let inputs: Vec<INPUT> = events
        .iter()
        .map(|&(vk, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 },
            },
        })
        .collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) }
}

fn send_ctrl_v() -> u32 {
    send_keys(&[(VK_CONTROL_, false), (VK_V, false), (VK_V, true), (VK_CONTROL_, true)])
}

/// A second tap of Ctrl+Shift+Space fires the paste while Shift is often
/// still held: the app would get Ctrl+Shift+V (not paste in many apps).
/// Waits for Shift/Alt/Win to be released, else sends synthetic key-ups.
fn wait_modifiers_released() {
    let stray = [VK_SHIFT_, VK_MENU_, VK_LWIN_];
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if !stray.iter().any(|&k| key_down(k)) {
            return;
        }
        sleep(Duration::from_millis(20));
    }
    let ups: Vec<(u16, bool)> = stray.iter().filter(|&&k| key_down(k)).map(|&k| (k, true)).collect();
    send_keys(&ups);
}

pub fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn process_path(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(h);
        if ok == 0 {
            String::new()
        } else {
            String::from_utf16_lossy(&buf[..size as usize])
        }
    }
}

fn describe(hwnd: HWND) -> String {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    let exe = process_path(pid).rsplit('\\').next().unwrap_or("?").to_string();
    let mut elevated = "?".to_string();
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !h.is_null() {
            let mut token: HANDLE = std::ptr::null_mut();
            if windows_sys::Win32::System::Threading::OpenProcessToken(h, TOKEN_QUERY, &mut token) != 0 {
                let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
                let mut ret = 0u32;
                if GetTokenInformation(token, TokenElevation, (&mut e as *mut TOKEN_ELEVATION).cast(), 4, &mut ret) != 0 {
                    elevated = if e.TokenIsElevated != 0 { "yes" } else { "no" }.into();
                }
                CloseHandle(token);
            } else if GetLastError() == ERROR_ACCESS_DENIED {
                elevated = "yes (token access denied: runs with higher privileges than this daemon)".into();
            }
            CloseHandle(h);
        }
    }
    format!("hwnd={hwnd:?} title={:?} exe={exe} elevated={elevated}", window_title(hwnd))
}

fn self_is_admin() -> bool {
    #[link(name = "shell32")]
    extern "system" {
        fn IsUserAnAdmin() -> i32;
    }
    unsafe { IsUserAnAdmin() != 0 }
}

/// OpenClipboard fails while another process holds it (clipboard history,
/// managers); it's usually a moment, so retry.
fn open_clipboard() -> bool {
    for _ in 0..10 {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            return true;
        }
        sleep(Duration::from_millis(20));
    }
    false
}

pub fn set_clipboard(text: &str) -> bool {
    let data: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if h.is_null() {
            return false;
        }
        let p = GlobalLock(h) as *mut u16;
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        GlobalUnlock(h);
        if !open_clipboard() {
            println!("[clipboard] OpenClipboard failed while writing, text lost");
            GlobalFree(h);
            return false;
        }
        EmptyClipboard();
        SetClipboardData(CF_UNICODETEXT_, h as HANDLE);
        CloseClipboard();
    }
    true
}

/// Pastes only if the window focused when recording started is still in
/// front. The text is left on the clipboard either way.
pub fn paste_if_focus_unchanged(text: &str, expected: HWND, press_enter: bool) -> bool {
    set_clipboard(text);
    let current = foreground();
    if current != expected {
        println!("[diag] focus changed: expected {} / current {}", describe(expected), describe(current));
        return false;
    }
    wait_modifiers_released();
    sleep(PASTE_DELAY_BEFORE);
    let mods = format!(
        "shift={} ctrl={} alt={} win={}",
        key_down(VK_SHIFT_),
        key_down(VK_CONTROL_),
        key_down(VK_MENU_),
        key_down(VK_LWIN_)
    );
    let sent = send_ctrl_v();
    sleep(PASTE_DELAY_AFTER);
    println!(
        "[diag] target {} | daemon_admin={} | modifiers at paste: {mods} | SendInput accepted {sent}/4 events",
        describe(expected),
        self_is_admin()
    );
    if sent < 4 {
        println!("[diag] SendInput rejected events (GetLastError={})", unsafe { GetLastError() });
    }
    if sent == 4 && press_enter {
        sleep(ENTER_DELAY);
        send_keys(&[(VK_RETURN_, false), (VK_RETURN_, true)]);
    }
    sent == 4
}

// --- Claude Code windows ------------------------------------------------------

/// Claude Code titles its terminal "<glyph> <topic>" (✳ idle, animated
/// symbols while working): Unicode categories So, Sm, Po, like v0.5.
fn is_title_glyph(c: char) -> bool {
    use unicode_general_category::{get_general_category, GeneralCategory::*};
    matches!(get_general_category(c), OtherSymbol | MathSymbol | OtherPunctuation)
}

pub fn topic_from_title(title: &str) -> Option<String> {
    let t = title.trim();
    let mut chars = t.chars();
    let first = chars.next()?;
    let second = chars.next();
    if t.chars().count() > 2 && second == Some(' ') && is_title_glyph(first) {
        return Some(chars.as_str().trim().to_string());
    }
    if t.contains("Claude Code") {
        return Some(t.to_string());
    }
    None
}

pub fn claude_topic(hwnd: HWND) -> Option<String> {
    if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
        return None;
    }
    topic_from_title(&window_title(hwnd))
}

pub fn is_claude_window(hwnd: HWND) -> bool {
    claude_topic(hwnd).is_some()
}

/// Desktop and taskbars: focus lands there and a Ctrl+V "succeeds" into
/// nothing.
pub fn is_shell_surface(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return true;
    }
    matches!(class_name(hwnd).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Front-most visible window showing Claude Code (EnumWindows walks in
/// z-order), as (hwnd, topic).
pub fn find_claude_window() -> (HWND, Option<String>) {
    unsafe extern "system" fn visit(hwnd: HWND, lp: LPARAM) -> windows_sys::core::BOOL {
        let out = &mut *(lp as *mut (HWND, Option<String>));
        if IsWindowVisible(hwnd) != 0 {
            if let Some(t) = claude_topic(hwnd) {
                *out = (hwnd, Some(t));
                return 0;
            }
        }
        1
    }
    let mut found: (HWND, Option<String>) = (std::ptr::null_mut(), None);
    unsafe { EnumWindows(Some(visit), &mut found as *mut _ as LPARAM) };
    found
}

/// Brings `hwnd` to the front: borrows the foreground thread's input queue,
/// and if the foreground lock still refuses, an Alt tap lifts it.
pub fn focus(hwnd: HWND) -> bool {
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        let current = foreground();
        let this_thread = GetCurrentThreadId();
        let fg_thread = if current.is_null() { 0 } else { GetWindowThreadProcessId(current, std::ptr::null_mut()) };
        let attached = fg_thread != 0 && fg_thread != this_thread && AttachThreadInput(this_thread, fg_thread, 1) != 0;
        BringWindowToTop(hwnd);
        if SetForegroundWindow(hwnd) == 0 {
            send_keys(&[(VK_MENU_, false), (VK_MENU_, true)]);
            SetForegroundWindow(hwnd);
        }
        if attached {
            AttachThreadInput(this_thread, fg_thread, 0);
        }
    }
    let deadline = Instant::now() + FOCUS_WAIT;
    while Instant::now() < deadline {
        if foreground() == hwnd {
            return true;
        }
        sleep(Duration::from_millis(20));
    }
    false
}

/// Ctrl+Tab / Ctrl+Shift+Tab, then waits for the title to follow.
fn switch_tab(hwnd: HWND, backwards: bool) {
    let before = window_title(hwnd);
    let mut keys = vec![(VK_CONTROL_, false)];
    if backwards {
        keys.push((VK_SHIFT_, false));
    }
    keys.extend([(VK_TAB_, false), (VK_TAB_, true)]);
    if backwards {
        keys.push((VK_SHIFT_, true));
    }
    keys.push((VK_CONTROL_, true));
    send_keys(&keys);
    let deadline = Instant::now() + TAB_WAIT;
    while Instant::now() < deadline && window_title(hwnd) == before {
        sleep(Duration::from_millis(20));
    }
}

fn find_tab(hwnd: HWND, topic: &str) -> Option<usize> {
    let start = window_title(hwnd);
    for steps in 1..=MAX_TABS {
        switch_tab(hwnd, false);
        if foreground() != hwnd {
            return None; // the user moved elsewhere; stop sending keys
        }
        if claude_topic(hwnd).as_deref() == Some(topic) {
            return Some(steps);
        }
        if window_title(hwnd) == start {
            return None;
        }
    }
    None
}

/// Keeps `hwnd` fully transparent while it holds focus for a paste; the
/// original style and opacity always come back.
struct Invisible {
    hwnd: HWND,
    style: isize,
    was_layered: bool,
    alpha: u8,
    flags: u32,
}

impl Invisible {
    fn new(hwnd: HWND) -> Self {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let was_layered = style & WS_EX_LAYERED as isize != 0;
            let (mut alpha, mut flags) = (255u8, 0u32);
            if was_layered {
                GetLayeredWindowAttributes(hwnd, std::ptr::null_mut(), &mut alpha, &mut flags);
            }
            let mut hidden = false;
            if was_layered || SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as isize) != 0 {
                hidden = SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA) != 0;
            }
            if !hidden {
                println!("[diag] could not make {} transparent; it will flash", describe(hwnd));
            }
            Self { hwnd, style, was_layered, alpha, flags }
        }
    }
}

impl Drop for Invisible {
    fn drop(&mut self) {
        unsafe {
            if self.was_layered {
                SetLayeredWindowAttributes(self.hwnd, 0, self.alpha, if self.flags != 0 { self.flags } else { LWA_ALPHA });
            } else {
                SetLayeredWindowAttributes(self.hwnd, 0, 255, LWA_ALPHA);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, self.style);
            }
            RedrawWindow(self.hwnd, std::ptr::null(), std::ptr::null_mut(), RDW_INVALIDATE | RDW_ALLCHILDREN | RDW_FRAME);
        }
    }
}

// --- typing straight into a Claude Code console ---------------------------------

static CONSOLE_LOCK: Mutex<()> = Mutex::new(());

/// Every claude.exe except the Claude desktop app (under WindowsApps).
fn claude_code_pids() -> Vec<u32> {
    let snap = ct_core::procs::Snapshot::take();
    snap.0
        .iter()
        .filter(|(_, p)| p.exe == "claude.exe")
        .map(|(&pid, _)| pid)
        .filter(|&pid| !process_path(pid).to_lowercase().contains("\\windowsapps\\"))
        .collect()
}

/// Attaches this process (it has no console of its own) to `pid`'s console.
/// Ctrl+C there is ignored meanwhile, so it can't take the daemon down.
struct Attached(bool);

impl Attached {
    fn new(pid: u32) -> Self {
        unsafe {
            FreeConsole();
            if AttachConsole(pid) == 0 {
                return Self(false);
            }
            SetConsoleCtrlHandler(None, 1);
        }
        Self(true)
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                FreeConsole();
                SetConsoleCtrlHandler(None, 0);
            }
        }
    }
}

fn console_title() -> String {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetConsoleTitleW(buf.as_mut_ptr(), 1024) };
    String::from_utf16_lossy(&buf[..n as usize])
}

/// (pid, topic) of every Claude Code session, read from its console title
/// (the tab's title, even for a tab in the background).
pub fn claude_consoles() -> Vec<(u32, String)> {
    let _g = CONSOLE_LOCK.lock().unwrap();
    let mut found = Vec::new();
    for pid in claude_code_pids() {
        let a = Attached::new(pid);
        let topic = if a.0 { topic_from_title(&console_title()) } else { None };
        drop(a);
        if let Some(t) = topic {
            found.push((pid, t));
        }
    }
    found
}

pub fn console_pid(topic: Option<&str>) -> Option<u32> {
    let topic = topic?;
    claude_consoles().into_iter().find(|(_, t)| t == topic).map(|(p, _)| p)
}

fn key_records(units: &[u16], vk: u16, scan: u16) -> Vec<INPUT_RECORD> {
    let mut v = Vec::with_capacity(units.len() * 2);
    for &u in units {
        for down in [1, 0] {
            v.push(INPUT_RECORD {
                EventType: KEY_EVENT as u16,
                Event: INPUT_RECORD_0 {
                    KeyEvent: KEY_EVENT_RECORD {
                        bKeyDown: down,
                        wRepeatCount: 1,
                        wVirtualKeyCode: vk,
                        wVirtualScanCode: scan,
                        uChar: KEY_EVENT_RECORD_0 { UnicodeChar: u },
                        dwControlKeyState: 0,
                    },
                },
            });
        }
    }
    v
}

fn write_input(h: HANDLE, recs: &[INPUT_RECORD]) -> bool {
    let mut written = 0u32;
    unsafe { WriteConsoleInputW(h, recs.as_ptr(), recs.len() as u32, &mut written) != 0 && written as usize == recs.len() }
}

/// Types into a Claude Code session with the focus untouched: `pid` if it's
/// still open (Claude Code retitles sessions), else the one titled `topic`,
/// else only when exactly one session is open.
pub fn type_into_console(text: &str, topic: Option<&str>, press_enter: bool, pid: Option<u32>) -> bool {
    let sessions = claude_consoles();
    let matches: Vec<u32> = if pid.is_some_and(|p| sessions.iter().any(|(s, _)| *s == p)) {
        vec![pid.unwrap()]
    } else if let Some(t) = topic {
        sessions.iter().filter(|(_, s)| s == t).map(|(p, _)| *p).collect()
    } else {
        sessions.iter().map(|(p, _)| *p).collect()
    };
    if matches.is_empty() || (topic.is_none() && matches.len() > 1) {
        let titles: Vec<&String> = sessions.iter().map(|(_, t)| t).collect();
        println!("[diag] console: {} session(s) for {topic:?} among {titles:?}", matches.len());
        return false;
    }
    let pid = matches[0];
    // WriteConsoleInputW takes UTF-16 units: an emoji goes as its two halves.
    let units: Vec<u16> = text.encode_utf16().collect();
    let _g = CONSOLE_LOCK.lock().unwrap();
    let a = Attached::new(pid);
    if !a.0 {
        println!("[diag] console: could not attach to claude.exe pid={pid}");
        return false;
    }
    let h = unsafe {
        CreateFileW(
            wide("CONIN$").as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            3,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        println!("[diag] console: no input buffer for pid={pid}");
        return false;
    }
    let ok = write_input(h, &key_records(&units, 0, 0));
    if !ok {
        println!("[diag] console: WriteConsoleInput failed (GetLastError={})", unsafe { GetLastError() });
    } else if press_enter {
        sleep(CONSOLE_ENTER_DELAY);
        write_input(h, &key_records(&[b'\r' as u16], VK_RETURN_, 0x1C));
    }
    unsafe { CloseHandle(h) };
    drop(a);
    if ok {
        println!("[console] typed into claude.exe pid={pid} ({topic:?})");
    }
    ok
}

/// Sends `text` to the Claude Code session `topic` in `hwnd`, wherever the
/// user is: console first, then the visible route (invisible terminal,
/// find the tab, paste, put everything back).
pub fn paste_into_window(text: &str, hwnd: HWND, topic: Option<&str>, press_enter: bool, pid: Option<u32>) -> bool {
    set_clipboard(text);
    let previous = foreground();
    let in_front = !hwnd.is_null() && previous == hwnd && claude_topic(hwnd).as_deref() == topic;
    if !in_front && type_into_console(text, topic, press_enter, pid) {
        return true;
    }
    let Some(topic) = topic else {
        println!("[diag] wake: no Claude Code window seen yet; text left on the clipboard");
        return false;
    };
    if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
        println!("[diag] wake: no Claude Code window seen yet; text left on the clipboard");
        return false;
    }
    if previous == hwnd {
        return paste_into(text, hwnd, topic, previous, press_enter);
    }
    let _hidden = Invisible::new(hwnd);
    paste_into(text, hwnd, topic, previous, press_enter)
}

fn paste_into(text: &str, hwnd: HWND, topic: &str, previous: HWND, press_enter: bool) -> bool {
    if previous != hwnd && !focus(hwnd) {
        println!("[diag] could not bring {} to the front", describe(hwnd));
        return false;
    }
    wait_modifiers_released();
    let mut moved = 0;
    if claude_topic(hwnd).as_deref() != Some(topic) {
        match find_tab(hwnd, topic) {
            Some(n) => moved = n,
            None => {
                println!("[diag] wake: no tab titled {topic:?} in {}; text left on the clipboard", describe(hwnd));
                return false;
            }
        }
    }
    let ok = paste_if_focus_unchanged(text, hwnd, press_enter);
    sleep(ENTER_DELAY);
    for _ in 0..moved {
        if foreground() != hwnd {
            break;
        }
        switch_tab(hwnd, true);
    }
    if !previous.is_null() && previous != hwnd && unsafe { IsWindow(previous) } != 0 {
        focus(previous);
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics() {
        assert_eq!(topic_from_title("\u{2733} Optimizar Claude Talk").as_deref(), Some("Optimizar Claude Talk"));
        assert_eq!(topic_from_title("\u{2810} trabajando").as_deref(), Some("trabajando"));
        assert_eq!(topic_from_title("· algo").as_deref(), Some("algo"));
        assert_eq!(topic_from_title("Claude Code").as_deref(), Some("Claude Code"));
        assert_eq!(topic_from_title("C:\\Users\\x - PowerShell"), None);
        assert_eq!(topic_from_title("(x) algo"), None);
        assert_eq!(topic_from_title("a b"), None);
    }
}
