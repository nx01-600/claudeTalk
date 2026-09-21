"""Persistent dictation settings: JSON in %APPDATA%\\claudeTalk.

Stored outside the repo so it survives plugin updates and never pollutes
git. Every set() writes to disk right away (there are only a few values).
"""

import json
import os
from pathlib import Path

CONFIG_DIR = Path(os.environ.get("APPDATA", str(Path.home()))) / "claudeTalk"
CONFIG_PATH = CONFIG_DIR / "dictation.json"

VK_LMENU = 0xA4
VK_RCONTROL = 0xA3

DEFAULTS = {
    "hotkey": [VK_LMENU, VK_RCONTROL],
    "silence_ms": 2000,
    "sound": True,
    "theme": "light",  # light | dark
    "glass": 60,  # 0..100, glass effect intensity (blur + transparency)
    "position": "bottom",  # bottom | top
    "language": "es",  # es | en | auto
}

KEY_NAMES = {
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
        self.load()

    def load(self):
        try:
            with open(self._path, "r", encoding="utf-8") as fh:
                stored = json.load(fh)
        except (OSError, ValueError):
            return
        for key in DEFAULTS:
            if key in stored:
                self._data[key] = stored[key]

    def save(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self._path.with_suffix(".tmp")
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(self._data, fh, indent=2, ensure_ascii=False)
        os.replace(tmp, self._path)

    def get(self, key: str):
        return self._data[key]

    def set(self, key: str, value):
        self._data[key] = value
        self.save()

    def as_dict(self) -> dict:
        return dict(self._data)
