"""Hotkey global por acorde de teclas (por defecto Alt izquierdo + Ctrl derecho).

RegisterHotKey no sirve aca: sus flags MOD_ALT/MOD_CONTROL no distinguen
lado, y no acepta un acorde hecho solo de modificadores. Un hook de bajo
nivel (WH_KEYBOARD_LL, lo que usa la libreria `keyboard`) si lo permite,
pero ya nos trajo storms de auto-repeat y desincronizaciones. En cambio se
hace polling de GetAsyncKeyState cada 15ms desde un hilo propio: cuando
todas las teclas del acorde pasan a estar apretadas se dispara una vez, y
no se vuelve a disparar hasta que alguna se suelte. Sin hooks, sin
repeticion, y el costo es despreciable.

`pressed_keys()` sirve para capturar una combinacion nueva desde ajustes.
"""

import ctypes
import threading
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)

VK_ESCAPE = 0x1B
POLL_S = 0.015

# Codigos que no cuentan como "tecla" al capturar un acorde: botones del
# mouse y los modificadores genericos (Windows los marca apretados a la vez
# que la version izquierda/derecha, que es la que interesa).
_CAPTURE_IGNORE = {0x01, 0x02, 0x04, 0x05, 0x06, 0x10, 0x11, 0x12, 0x90, 0x91}


def is_key_down(vk: int) -> bool:
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


def pressed_keys() -> list[int]:
    """Teclas fisicamente apretadas ahora mismo (sin mouse ni modificadores
    genericos), ordenadas por codigo."""
    return [vk for vk in range(0x08, 0xFF) if vk not in _CAPTURE_IGNORE and is_key_down(vk)]


class ChordHotkey:
    """Llama a `on_press` (desde su propio hilo) cada vez que el acorde pasa
    de suelto a apretado. `set_keys` cambia el acorde en caliente; `pause`
    lo silencia mientras ajustes captura una combinacion nueva."""

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
