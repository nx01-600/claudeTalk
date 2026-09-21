"""Prueba manual de inyeccion por portapapeles: abre Notepad, pega texto con
tildes/enie/signos, y verifica que quedo tanto en Notepad como en el
portapapeles (el texto dictado siempre se deja copiado, como red de seguridad).

Reemplaza a los viejos test_inject.py (SendInput char-by-char + EnumWindows)
y test_inject2.py (foreground actual), fusionados en uno solo ahora que
inject.py pega por Ctrl+V en vez de escribir caracter por caracter.
"""

import ctypes
import subprocess
import time

import inject

user32 = ctypes.windll.user32

SAMPLE_TEXT = "Prueba ñ á é í ó ú ¿cómo va?"

subprocess.Popen(["notepad.exe"])
time.sleep(2.5)

EnumWindowsProc = ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_int, ctypes.c_int)
target_hwnd = [0]


def _enum(hwnd, lparam):
    if not user32.IsWindowVisible(hwnd):
        return True
    length = user32.GetWindowTextLengthW(hwnd)
    if length == 0:
        return True
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buf, length + 1)
    title = buf.value
    if "notepad" in title.lower() or "bloc de notas" in title.lower():
        target_hwnd[0] = hwnd
        return False
    return True


user32.EnumWindows(EnumWindowsProc(_enum), 0)
notepad_hwnd = target_hwnd[0]

if not notepad_hwnd:
    print("FALLO: no se encontro ninguna ventana de notepad")
    raise SystemExit(1)

user32.SetForegroundWindow(notepad_hwnd)
time.sleep(0.5)
fg = user32.GetForegroundWindow()
print(f"foreground tras SetForegroundWindow: {fg} (coincide: {fg == notepad_hwnd})")

ok = inject.paste_text_if_focus_unchanged(SAMPLE_TEXT, notepad_hwnd)
print(f"paste_text_if_focus_unchanged devolvio: {ok}")
time.sleep(0.3)

edit_hwnd = user32.FindWindowExW(notepad_hwnd, 0, "Edit", None)
if not edit_hwnd:
    edit_hwnd = user32.FindWindowExW(notepad_hwnd, 0, "RichEditD2DPT", None)

if edit_hwnd:
    length = user32.GetWindowTextLengthW(edit_hwnd)
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.SendMessageW(edit_hwnd, 0x000D, length + 1, buf)
    pasted = buf.value
    print(f"contenido leido del control: {pasted!r}")
    print("OK: texto pegado correctamente" if SAMPLE_TEXT in pasted else "FALLO: texto no coincide")
else:
    print("FALLO: no se encontro el control de texto")

clipboard_now = inject._get_clipboard_text()
print(f"portapapeles despues de pegar: {clipboard_now!r}")
print("OK: texto dictado quedo en el portapapeles" if clipboard_now == SAMPLE_TEXT else "FALLO: no quedo en el portapapeles")
