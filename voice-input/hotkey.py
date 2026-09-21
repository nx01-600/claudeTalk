"""Global hotkey as a key chord (default: Ctrl + Shift + Space).

RegisterHotKey does not work here: its MOD_ALT/MOD_CONTROL flags cannot tell
left from right, and it rejects a chord made only of modifiers. A low-level
hook (WH_KEYBOARD_LL, what the `keyboard` library uses) allows it, but it
already gave us auto-repeat storms and desyncs. Instead we poll
GetAsyncKeyState every 15 ms from a dedicated thread: when every key of the
chord becomes pressed it fires once, and it does not fire again until one
of them is released. No hooks, no repeats, negligible cost.

`pressed_keys()` is used to capture a new combination from the settings panel.
"""

import ctypes
import threading
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)

VK_ESCAPE = 0x1B
POLL_S = 0.015

# Codes that do not count as a "key" while capturing a chord: mouse buttons
# and the generic modifiers (Windows reports them pressed together with the
# left/right variant, which is the one we care about).
_CAPTURE_IGNORE = {0x01, 0x02, 0x04, 0x05, 0x06, 0x10, 0x11, 0x12, 0x90, 0x91}


def is_key_down(vk: int) -> bool:
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


def pressed_keys() -> list[int]:
    """Keys physically held right now (no mouse buttons, no generic
    modifiers), sorted by virtual-key code."""
    return [vk for vk in range(0x08, 0xFF) if vk not in _CAPTURE_IGNORE and is_key_down(vk)]


class ChordHotkey:
    """Calls `on_press` (from its own thread) every time the chord goes from
    released to pressed. `set_keys` swaps the chord live; `pause` silences it
    while the settings panel captures a new combination."""

    def __init__(self, on_press, keys, poll_s: float = POLL_S):
        self._on_press = on_press
        self._keys = tuple(keys)
        self._poll_s = poll_s
        self._paused = threading.Event()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self._thread.start()

    def stop(self):
        self._stop.set()

    def set_keys(self, keys):
        self._keys = tuple(keys)

    def pause(self):
        self._paused.set()

    def resume(self):
        self._paused.clear()

    def _run(self):
        armed = True
        while not self._stop.is_set():
            keys = self._keys
            all_down = bool(keys) and all(is_key_down(vk) for vk in keys)
            if self._paused.is_set():
                armed = False
            elif all_down and armed:
                armed = False
                self._on_press()
            elif not all_down:
                armed = True
            time.sleep(self._poll_s)
