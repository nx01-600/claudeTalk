"""Configuracion persistente del dictado: JSON en %APPDATA%\\claudeTalk.

Se guarda fuera del repo para que sobreviva a actualizaciones del plugin y
no ensucie git. Cada set() escribe a disco enseguida (son pocos valores).
"""

import json
import os
from pathlib import Path

CONFIG_DIR = Path(os.environ.get("APPDATA", str(Path.home()))) / "claudeTalk"
CONFIG_PATH = CONFIG_DIR / "dictado.json"

VK_LMENU = 0xA4
VK_RCONTROL = 0xA3

DEFAULTS = {
    "hotkey": [VK_LMENU, VK_RCONTROL],
    "silence_ms": 2000,
    "sound": True,
    "theme": "light",  # light | dark
    "glass": 60,  # 0..100, intensidad del efecto vidrio (blur + transparencia)
    "position": "bottom",  # bottom | top
    "language": "es",  # es | en | auto
}

KEY_NAMES = {
    0xA4: "Alt izq",
    0xA5: "Alt der",
    0xA2: "Ctrl izq",
    0xA3: "Ctrl der",
    0xA0: "Shift izq",
    0xA1: "Shift der",
    0x5B: "Win izq",
    0x5C: "Win der",
    0x20: "Espacio",
    0x1B: "Esc",
    0x09: "Tab",
    0x0D: "Enter",
    0x08: "Retroceso",
    0x14: "Bloq Mayús",
    0x2D: "Insert",
    0x2E: "Supr",
    0x24: "Inicio",
    0x23: "Fin",
    0x21: "Re Pág",
    0x22: "Av Pág",
    0x25: "Izquierda",
    0x26: "Arriba",
    0x27: "Derecha",
    0x28: "Abajo",
    0x5D: "Menú",
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
    return KEY_NAMES.get(vk, f"Tecla {vk:#04x}")


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
