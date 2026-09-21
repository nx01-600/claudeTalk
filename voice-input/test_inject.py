"""Manual test of clipboard injection: opens Notepad, pastes text with
accents/enie/punctuation, and verifies it landed both in Notepad and on the
clipboard (dictated text is always left copied, as a safety net).

Replaces the old test_inject.py (character-by-character SendInput +
EnumWindows) and test_inject2.py (current foreground), now merged into one
now that inject.py pastes via Ctrl+V instead of typing character by
character.
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
    print("FAIL: no Notepad window found")
    raise SystemExit(1)

user32.SetForegroundWindow(notepad_hwnd)
time.sleep(0.5)
fg = user32.GetForegroundWindow()
print(f"foreground after SetForegroundWindow: {fg} (matches: {fg == notepad_hwnd})")

ok = inject.paste_text_if_focus_unchanged(SAMPLE_TEXT, notepad_hwnd)
print(f"paste_text_if_focus_unchanged returned: {ok}")
time.sleep(0.3)

edit_hwnd = user32.FindWindowExW(notepad_hwnd, 0, "Edit", None)
if not edit_hwnd:
    edit_hwnd = user32.FindWindowExW(notepad_hwnd, 0, "RichEditD2DPT", None)

if edit_hwnd:
    length = user32.GetWindowTextLengthW(edit_hwnd)
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.SendMessageW(edit_hwnd, 0x000D, length + 1, buf)
    pasted = buf.value
    print(f"content read from control: {pasted!r}")
    print("OK: text pasted correctly" if SAMPLE_TEXT in pasted else "FAIL: text does not match")
else:
    print("FAIL: text control not found")

clipboard_now = inject._get_clipboard_text()
print(f"clipboard after pasting: {clipboard_now!r}")
print("OK: dictated text left in the clipboard" if clipboard_now == SAMPLE_TEXT else "FAIL: not left in the clipboard")
