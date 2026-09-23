"""Persistent dictation settings: JSON in %APPDATA%\\claudeTalk.

Stored outside the repo so it survives plugin updates and never pollutes
git. Every set() writes to disk right away (there are only a few values).
Other programs may edit the file too (the talk skill changes Claude's voice
through scripts/voice-toggle.ps1): reload_if_changed() picks that up.
"""

import json
import os
from pathlib import Path

CONFIG_DIR = Path(os.environ.get("APPDATA", str(Path.home()))) / "claudeTalk"
CONFIG_PATH = CONFIG_DIR / "dictation.json"

VK_CONTROL = 0x11
VK_SHIFT = 0x10
VK_SPACE = 0x20

DEFAULTS = {
    "hotkey": [VK_CONTROL, VK_SHIFT, VK_SPACE],  # generic Ctrl/Shift: either side works
    "silence_ms": 2000,  # 500..10000, silence that ends a recording
    "sensitivity": 50,  # 0..100, how easily the mic counts sound as speech
    "sound": True,
    "auto_enter": False,  # press Enter after pasting (sends the message)
    "wake_word": False,  # while talk mode is on, "Oye Claude" starts a dictation (wake.py)
    "theme": "light",  # light | dark
    "glass": 60,  # 0..100, glass effect intensity (blur + transparency)
    "position": "bottom",  # bottom | top
    "show_in_capture": False,  # overlay visible in screen sharing (glass becomes a snapshot)
    "language": "es",  # es | en | auto
    # Claude's voice (talk mode). Read by scripts/talk-common.ps1 too.
    "tts_voice": "es-CO-GonzaloNeural",  # any edge-tts neural voice
    "tts_rate": "+0%",  # edge-tts rate: -15% | +0% | +20% | +40%
}

KEY_NAMES = {
    0x11: "Ctrl",
    0x10: "Shift",
    0x12: "Alt",
    0xA4: "Left Alt",
    0xA5: "Right Alt",
    0xA2: "Left Ctrl",
    0xA3: "Right Ctrl",
    0xA0: "Left Shift",
    0xA1: "Right Shift",
    0x5B: "Left Win",
    0x5C: "Right Win",
    0x20: "Space",
    0x1B: "Esc",
    0x09: "Tab",
    0x0D: "Enter",
    0x08: "Backspace",
    0x14: "Caps Lock",
    0x2D: "Insert",
    0x2E: "Delete",
    0x24: "Home",
    0x23: "End",
    0x21: "Page Up",
    0x22: "Page Down",
    0x25: "Left",
    0x26: "Up",
    0x27: "Right",
    0x28: "Down",
    0x5D: "Menu",
    0xBB: "+",
    0xBD: "-",
    0xBC: ",",
    0xBE: ".",
    0xBF: "/",
    0xC0: "`",
    0xDB: "[",
    0xDC: "\\",
    0xDD: "]",
    0xDE: "'",
    0xBA: ";",
}
KEY_NAMES.update({0x70 + i: f"F{i + 1}" for i in range(12)})
KEY_NAMES.update({0x30 + i: str(i) for i in range(10)})
KEY_NAMES.update({0x41 + i: chr(ord("A") + i) for i in range(26)})
KEY_NAMES.update({0x60 + i: f"Num {i}" for i in range(10)})


def key_name(vk: int) -> str:
    return KEY_NAMES.get(vk, f"Key {vk:#04x}")


def hotkey_label(vks) -> str:
    return " + ".join(key_name(vk) for vk in vks)


class Config:
    def __init__(self, path: Path = CONFIG_PATH):
        self._path = path
        self._data = dict(DEFAULTS)
        self._mtime = None
        self.load()

    def _disk_mtime(self):
        try:
            return self._path.stat().st_mtime_ns
        except OSError:
            return None

    def load(self):
        self._mtime = self._disk_mtime()
        try:
            with open(self._path, "r", encoding="utf-8-sig") as fh:
                stored = json.load(fh)
        except (OSError, ValueError):
            self._mtime = None  # half-written by someone else: retry next time
            return
        for key in DEFAULTS:
            if key in stored:
                self._data[key] = stored[key]
        # sensitivity used to be low | medium | high
        old = {"low": 25, "medium": 50, "high": 75}
        if isinstance(self._data["sensitivity"], str):
            self._data["sensitivity"] = old.get(self._data["sensitivity"], 50)

    def save(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self._path.with_suffix(".tmp")
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(self._data, fh, indent=2, ensure_ascii=False)
        os.replace(tmp, self._path)
        self._mtime = self._disk_mtime()

    def reload_if_changed(self) -> dict:
        """Re-reads the file if something else wrote it; returns the keys
        whose value changed, with their new values."""
        mtime = self._disk_mtime()
        if mtime is None or mtime == self._mtime:
            return {}
        before = dict(self._data)
        self.load()
        return {k: v for k, v in self._data.items() if before.get(k) != v}

    def get(self, key: str):
        return self._data[key]

    def set(self, key: str, value):
        self._data[key] = value
        self.save()

    def as_dict(self) -> dict:
        return dict(self._data)
