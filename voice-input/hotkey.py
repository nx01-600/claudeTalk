"""Hotkey global via RegisterHotKey (nativo de Windows), no un hook de bajo nivel.

`keyboard` (pip) instala un WH_KEYBOARD_LL que intercepta TODAS las teclas del
sistema; ahi salian el auto-repeat storm y la necesidad de debounce manual.
RegisterHotKey es la API que usa el propio Windows para atajos globales: el OS
entrega un solo mensaje WM_HOTKEY por combinacion presionada, y MOD_NOREPEAT
le pide al OS que no reenvie mensajes mientras la tecla siga apretada (repeat
de autorepeticion), asi que el filtro de repeticion ya no hace falta.

RegisterHotKey no tiene evento de "soltar", asi que el release se detecta con
polling de GetAsyncKeyState desde el hilo que graba (ver `is_key_down`).
"""

import ctypes
import threading
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)

MOD_ALT = 0x0001
MOD_CONTROL = 0x0002
MOD_SHIFT = 0x0004
MOD_NOREPEAT = 0x4000
WM_HOTKEY = 0x0312
VK_SPACE = 0x20
VK_ESCAPE = 0x1B

HOTKEY_ID = 1


def is_key_down(vk: int) -> bool:
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


class GlobalHotkey:
    """Registra un hotkey global y corre un message loop en su propio hilo.

    `on_press` se llama (desde el hilo del hotkey) cada vez que llega WM_HOTKEY.
    No hay callback de "release": quien reciba on_press debe hacer polling de
    `is_key_down(VK_SPACE)` si necesita saber cuando se suelta.
    """

    def __init__(self, on_press, modifiers=MOD_CONTROL | MOD_SHIFT, vk=VK_SPACE):
        self._on_press = on_press
        self._modifiers = modifiers | MOD_NOREPEAT
        self._vk = vk
        self._thread_id = None
        self._ready = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self._thread.start()
        self._ready.wait()

    def _run(self):
        self._thread_id = ctypes.windll.kernel32.GetCurrentThreadId()

        if not user32.RegisterHotKey(None, HOTKEY_ID, self._modifiers, self._vk):
            raise ctypes.WinError(ctypes.get_last_error())

        self._ready.set()

        msg = wintypes.MSG()
        try:
            while user32.GetMessageW(ctypes.byref(msg), None, 0, 0) != 0:
                if msg.message == WM_HOTKEY and msg.wParam == HOTKEY_ID:
                    self._on_press()
        finally:
            user32.UnregisterHotKey(None, HOTKEY_ID)

    def stop(self):
        if self._thread_id is not None:
            user32.PostThreadMessageW(self._thread_id, 0x0012, 0, 0)  # WM_QUIT
