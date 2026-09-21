"""Inyeccion de texto via portapapeles + Ctrl+V simulado.

Version anterior escribia caracter por caracter con SendInput+KEYEVENTF_UNICODE
para no depender del portapapeles. Eso evitaba el problema de tildes/enie que
tienen keyboard.write()/pyautogui, pero character-by-character SendInput a
veces llegaba a la ventana equivocada si el foco cambiaba a mitad de la
inyeccion (bug de foco). Pegar por portapapeles es atomico: o el texto entero
llega de una, o no llega nada; y como el texto ya esta en UTF-16 en el
portapapeles, tildes/enie/¿¡ tampoco se pierden.

El texto dictado siempre queda en el portapapeles al terminar (se pegue o
no), como red de seguridad: si el Ctrl+V no llego a destino por lo que sea,
el usuario lo pega a mano con lo que ya tiene copiado.
"""

import ctypes
import time
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

# ctypes asume restype=c_int (32 bits) si no se declara. GlobalAlloc/GlobalLock/
# GetClipboardData devuelven punteros/handles de 64 bits: sin esto, Python de
# 64 bits los trunca y corrompe la memoria o falla silenciosamente.
kernel32.GlobalAlloc.restype = wintypes.HGLOBAL
kernel32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [wintypes.HGLOBAL]
kernel32.GlobalUnlock.argtypes = [wintypes.HGLOBAL]
user32.GetClipboardData.restype = wintypes.HANDLE
user32.GetClipboardData.argtypes = [wintypes.UINT]
user32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
user32.GetForegroundWindow.restype = wintypes.HWND

INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x0002
VK_CONTROL = 0x11
VK_V = 0x56

GMEM_MOVEABLE = 0x0002
CF_UNICODETEXT = 13

PASTE_DELAY_BEFORE_S = 0.03
PASTE_DELAY_AFTER_S = 0.15


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [
        ("wVk", wintypes.WORD),
        ("wScan", wintypes.WORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ctypes.POINTER(wintypes.ULONG)),
    ]


class _INPUTunion(ctypes.Union):
    _fields_ = [("ki", KEYBDINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("union", _INPUTunion)]


def get_foreground_window() -> int:
    return user32.GetForegroundWindow()


def _key_event(vk: int, key_up: bool):
    flags = KEYEVENTF_KEYUP if key_up else 0
    inp = INPUT(type=INPUT_KEYBOARD, union=_INPUTunion(ki=KEYBDINPUT(vk, 0, flags, 0, None)))
    user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def _send_ctrl_v():
    _key_event(VK_CONTROL, key_up=False)
    _key_event(VK_V, key_up=False)
    _key_event(VK_V, key_up=True)
    _key_event(VK_CONTROL, key_up=True)


def _get_clipboard_text() -> str | None:
    """Lee CF_UNICODETEXT del portapapeles, o None si no hay texto (u otro formato)."""
    if not user32.OpenClipboard(0):
        return None
    try:
        if not user32.IsClipboardFormatAvailable(CF_UNICODETEXT):
            return None
        handle = user32.GetClipboardData(CF_UNICODETEXT)
        if not handle:
            return None
        ptr = kernel32.GlobalLock(handle)
        if not ptr:
            return None
        try:
            return ctypes.wstring_at(ptr)
        finally:
            kernel32.GlobalUnlock(handle)
    finally:
        user32.CloseClipboard()


def _set_clipboard_text(text: str):
    data = text.encode("utf-16-le") + b"\x00\x00"
    handle = kernel32.GlobalAlloc(GMEM_MOVEABLE, len(data))
    ptr = kernel32.GlobalLock(handle)
    ctypes.memmove(ptr, data, len(data))
    kernel32.GlobalUnlock(handle)

    user32.OpenClipboard(0)
    user32.EmptyClipboard()
    user32.SetClipboardData(CF_UNICODETEXT, handle)
    user32.CloseClipboard()


def copy_to_clipboard(text: str):
    """Deja el texto en el portapapeles sin pegarlo (fallback si cambio el foco)."""
    _set_clipboard_text(text)


def paste_text_if_focus_unchanged(text: str, expected_hwnd: int) -> bool:
    """Pega `text` solo si la ventana enfocada al empezar a grabar sigue siendo
    la misma ahora. `text` queda en el portapapeles en cualquier caso.

    Devuelve True si pego, False si el foco cambio y solo quedo en el
    portapapeles.
    """
    _set_clipboard_text(text)

    if get_foreground_window() != expected_hwnd:
        return False

    time.sleep(PASTE_DELAY_BEFORE_S)
    _send_ctrl_v()
    time.sleep(PASTE_DELAY_AFTER_S)

    return True
