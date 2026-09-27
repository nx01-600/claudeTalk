//! Dictation settings: %APPDATA%\claudeTalk\dictation.json, shared with the
//! plugin (`claudetalk toggle set ...` edits it too). Every set() writes to
//! disk right away; reload_if_changed() picks up edits made by others.
//! Unlike v0.5 it keeps keys it doesn't know, so nothing another tool
//! stored is lost when the daemon saves.

use ct_core::fsutil::write_json;
use ct_core::paths;
use serde_json::{json, Map, Value};
use std::time::SystemTime;

/// Defaults; the language-dependent ones come from claudeTalk's language.
pub fn defaults() -> Map<String, Value> {
    let pack = ct_core::lang::current();
    let v = json!({
        "hotkey": [0x11, 0x10, 0x20],
        "silence_ms": 2000,
        "sensitivity": 50,
        "sound": true,
        "auto_enter": false,
        "wake_word": false,
        "wake_phrase": pack.wake_phrase,
        "speak_only_spoken": false,
        "glass": 60,
        "position": "bottom",
        "remember_drag": false,
        "drag_pos": null,
        "show_in_capture": false,
        "lang": pack.code,
        "language": pack.code,
        "tts_voice": pack.default_voice(),
        "tts_rate": "+0%",
        "tts_volume": 100,
        "edge_version": ""
    });
    v.as_object().unwrap().clone()
}

pub struct Config {
    data: Map<String, Value>,
    mtime: Option<SystemTime>,
}

fn disk_mtime() -> Option<SystemTime> {
    std::fs::metadata(paths::settings_file()).and_then(|m| m.modified()).ok()
}

impl Config {
    pub fn load() -> Self {
        let mut c = Self { data: defaults(), mtime: None };
        c.reload();
        c
    }

    fn reload(&mut self) {
        self.mtime = disk_mtime();
        let Ok(bytes) = std::fs::read(paths::settings_file()) else {
            self.mtime = None;
            return;
        };
        let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
        let Ok(Value::Object(stored)) = serde_json::from_slice::<Value>(body) else {
            self.mtime = None; // half-written by someone else: retry next time
            return;
        };
        let mut data = defaults();
        for (k, v) in stored {
            data.insert(k, v);
        }
        // sensitivity used to be low | medium | high
        if let Some(Value::String(s)) = data.get("sensitivity") {
            let n = match s.as_str() {
                "low" => 25,
                "high" => 75,
                _ => 50,
            };
            data.insert("sensitivity".into(), json!(n));
        }
        self.data = data;
    }

    pub fn save(&mut self) {
        if let Err(e) = write_json(&paths::settings_file(), &Value::Object(self.data.clone())) {
            eprintln!("[settings] save failed: {e}");
        }
        self.mtime = disk_mtime();
    }

    /// Re-reads the file if something else wrote it; returns the keys whose
    /// value changed.
    pub fn reload_if_changed(&mut self) -> Vec<String> {
        let m = disk_mtime();
        if m.is_none() || m == self.mtime {
            return Vec::new();
        }
        let before = self.data.clone();
        self.reload();
        self.data.iter().filter(|(k, v)| before.get(*k) != Some(*v)).map(|(k, _)| k.clone()).collect()
    }

    pub fn get(&self, key: &str) -> &Value {
        self.data.get(key).unwrap_or(&Value::Null)
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.data.insert(key.to_string(), value);
        self.save();
    }

    pub fn bool(&self, key: &str) -> bool {
        ct_core::settings::truthy(self.get(key))
    }

    pub fn f64(&self, key: &str, default: f64) -> f64 {
        self.get(key).as_f64().unwrap_or(default)
    }

    pub fn str(&self, key: &str) -> String {
        match self.get(key) {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        }
    }

    pub fn hotkey(&self) -> Vec<u16> {
        self.get("hotkey")
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_u64()).map(|v| v as u16).collect())
            .unwrap_or_default()
    }
}

pub fn key_name(vk: u16) -> String {
    let fixed = match vk {
        0x11 => "Ctrl",
        0x10 => "Shift",
        0x12 => "Alt",
        0xA4 => "Left Alt",
        0xA5 => "Right Alt",
        0xA2 => "Left Ctrl",
        0xA3 => "Right Ctrl",
        0xA0 => "Left Shift",
        0xA1 => "Right Shift",
        0x5B => "Left Win",
        0x5C => "Right Win",
        0x20 => "Space",
        0x1B => "Esc",
        0x09 => "Tab",
        0x0D => "Enter",
        0x08 => "Backspace",
        0x14 => "Caps Lock",
        0x2D => "Insert",
        0x2E => "Delete",
        0x24 => "Home",
        0x23 => "End",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x5D => "Menu",
        0xBB => "+",
        0xBD => "-",
        0xBC => ",",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        0xBA => ";",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match vk {
        0x70..=0x7B => format!("F{}", vk - 0x70 + 1),
        0x30..=0x39 => format!("{}", vk - 0x30),
        0x41..=0x5A => ((b'A' + (vk - 0x41) as u8) as char).to_string(),
        0x60..=0x69 => format!("Num {}", vk - 0x60),
        _ => format!("Key {vk:#04x}"),
    }
}

pub fn hotkey_label(vks: &[u16]) -> String {
    vks.iter().map(|&v| key_name(v)).collect::<Vec<_>>().join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(hotkey_label(&[0x11, 0x10, 0x20]), "Ctrl + Shift + Space");
        assert_eq!(key_name(0x71), "F2");
        assert_eq!(key_name(0x4B), "K");
        assert_eq!(key_name(0x07), "Key 0x07");
    }
}
