//! Global key chord (default Ctrl + Shift + Space), voice-input/hotkey.py.
//!
//! RegisterHotKey can't tell left from right modifiers and rejects a chord
//! made only of modifiers; a low-level hook gave auto-repeat storms. v0.5
//! polled GetAsyncKeyState every 15 ms. Now Raw Input (RIDEV_INPUTSINK)
//! wakes us only when a key actually changes, and GetAsyncKeyState still
//! decides whether the whole chord is down: same edge logic, no polling.
//! Fires once when every key of the chord becomes pressed, and not again
//! until one of them is released (auto-repeat never re-fires it).

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::Input::{RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_INPUTSINK};

pub const VK_ESCAPE: u16 = 0x1B;

/// Mouse buttons and the generic modifiers (Windows reports them pressed
/// together with the left/right one, which is the one we keep).
const CAPTURE_IGNORE: [u16; 10] = [0x01, 0x02, 0x04, 0x05, 0x06, 0x10, 0x11, 0x12, 0x90, 0x91];

pub fn is_key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

/// Keys physically held right now (no mouse, no generic modifiers).
pub fn pressed_keys() -> Vec<u16> {
    (0x08u16..0xFF).filter(|vk| !CAPTURE_IGNORE.contains(vk) && is_key_down(*vk)).collect()
}

/// Keyboard raw input for `hwnd`, even when another app has the focus.
pub fn register(hwnd: HWND) -> bool {
    let dev = RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x06, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd };
    unsafe { RegisterRawInputDevices(&dev, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) != 0 }
}

/// Escape taken from the whole system while a recording runs: it cancels
/// the dictation and never reaches the window in front, where it would
/// interrupt Claude. Registered on the calling thread; released on drop.
pub struct EscapeGrab(bool);

const ESCAPE_HOTKEY_ID: i32 = 0xC7E5;

impl EscapeGrab {
    pub fn new() -> Self {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_NOREPEAT};
        Self(unsafe { RegisterHotKey(std::ptr::null_mut(), ESCAPE_HOTKEY_ID, MOD_NOREPEAT, VK_ESCAPE as u32) } != 0)
    }

    /// True once Escape was pressed; call on the thread that created it.
    pub fn pressed(&self) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};
        if !self.0 {
            return false;
        }
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), WM_HOTKEY, WM_HOTKEY, PM_REMOVE) != 0 }
    }
}

impl Drop for EscapeGrab {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::UnregisterHotKey(std::ptr::null_mut(), ESCAPE_HOTKEY_ID) };
        }
    }
}

pub struct Chord {
    pub keys: Vec<u16>,
    armed: bool,
    pub paused: bool,
}

impl Chord {
    pub fn new(keys: Vec<u16>) -> Self {
        Self { keys, armed: true, paused: false }
    }

    /// Called on every keyboard event; true when the chord was just pressed.
    pub fn update(&mut self) -> bool {
        let all_down = !self.keys.is_empty() && self.keys.iter().all(|&k| is_key_down(k));
        if self.paused {
            self.armed = false;
            false
        } else if all_down && self.armed {
            self.armed = false;
            true
        } else {
            if !all_down {
                self.armed = true;
            }
            false
        }
    }
}

/// Panel capture order: Ctrl, Alt, Shift, Win, then the rest by code.
pub fn chord_sorted(vks: &[u16]) -> Vec<u16> {
    let rank = |vk: u16| match vk {
        0xA2 | 0xA3 => 0,
        0xA4 | 0xA5 => 1,
        0xA0 | 0xA1 => 2,
        0x5B | 0x5C => 3,
        _ => 4,
    };
    let mut v: Vec<u16> = vks.to_vec();
    v.sort_by_key(|&vk| (rank(vk), vk));
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    /// Sends a real Escape: run by hand (`--ignored`). It is swallowed only
    /// when the grab works, so it never reaches the window in front then.
    #[test]
    #[ignore]
    fn escape_grab_swallows() {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
        let grab = super::EscapeGrab::new();
        assert!(grab.0, "RegisterHotKey(Escape) failed");
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: super::VK_ESCAPE, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [key(0), key(KEYEVENTF_KEYUP)];
        unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(grab.pressed());
        assert!(!grab.pressed());
    }

    #[test]
    fn sorted() {
        assert_eq!(super::chord_sorted(&[0x20, 0xA0, 0xA2, 0x20]), vec![0xA2, 0xA0, 0x20]);
    }
}
