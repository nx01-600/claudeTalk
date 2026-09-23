"""Text injection via clipboard + simulated Ctrl+V.

The earlier version typed character by character with SendInput+KEYEVENTF_UNICODE
to avoid depending on the clipboard. That avoided the accent/enie problem that
keyboard.write()/pyautogui have, but character-by-character SendInput would
sometimes land on the wrong window if focus changed mid-injection (a focus
bug). Pasting via clipboard is atomic: either the whole text arrives at once,
or none of it does; and since the text is already UTF-16 on the clipboard,
accents/enie/¿¡ are not lost either.

The dictated text is always left on the clipboard when done (whether pasted
or not), as a safety net: if the Ctrl+V didn't reach its target for whatever
reason, the user can paste it manually from what's already copied.
"""

import ctypes
import time
import unicodedata
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

# ctypes assumes restype=c_int (32 bits) if not declared. GlobalAlloc/GlobalLock/
# GetClipboardData return 64-bit pointers/handles: without this, 64-bit Python
# truncates them and either corrupts memory or fails silently.
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
VK_SHIFT = 0x10
VK_MENU = 0x12
VK_LWIN = 0x5B
VK_V = 0x56
VK_RETURN = 0x0D
ENTER_DELAY_S = 0.08

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
TOKEN_QUERY = 0x0008
TokenElevation = 20
ERROR_ACCESS_DENIED = 5

GMEM_MOVEABLE = 0x0002
CF_UNICODETEXT = 13

PASTE_DELAY_BEFORE_S = 0.03
PASTE_DELAY_AFTER_S = 0.15


ULONG_PTR = ctypes.c_size_t


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [
        ("dx", wintypes.LONG),
        ("dy", wintypes.LONG),
        ("mouseData", wintypes.DWORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    ]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [
        ("wVk", wintypes.WORD),
        ("wScan", wintypes.WORD),
        ("dwFlags", wintypes.DWORD),
        ("time", wintypes.DWORD),
        ("dwExtraInfo", ULONG_PTR),
    ]


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = [
        ("uMsg", wintypes.DWORD),
        ("wParamL", wintypes.WORD),
        ("wParamH", wintypes.WORD),
    ]


class _INPUTunion(ctypes.Union):
    # The union has to include MOUSEINPUT even though it's unused: it's the
    # largest member and defines the real size of INPUT (40 bytes on x64).
    # With only KEYBDINPUT it came out to 32, cbSize didn't match, and
    # SendInput silently returned 0 (ERROR_INVALID_PARAMETER): nothing was
    # ever pasted.
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT), ("hi", HARDWAREINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("union", _INPUTunion)]


user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]
user32.SendInput.restype = wintypes.UINT


def get_foreground_window() -> int:
    return user32.GetForegroundWindow()


def _key_event(vk: int, key_up: bool) -> int:
    """Returns how many events SendInput inserted: 1 if it went through, 0 if
    Windows rejected it (malformed INPUT structure, or UIPI if the target
    window runs with higher privileges than this process). That 0 is the
    only signal Windows gives that the paste won't arrive; it used to be
    ignored and treated as if it had pasted."""
    flags = KEYEVENTF_KEYUP if key_up else 0
    inp = INPUT(type=INPUT_KEYBOARD, union=_INPUTunion(ki=KEYBDINPUT(vk, 0, flags, 0, 0)))
    return user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def _send_keys(events: list[tuple[int, bool]]) -> int:
    """Sends several events in ONE call to SendInput, so no physical key
    sneaks in the middle of the combo. Returns how many went through."""
    array_type = INPUT * len(events)
    inputs = array_type()
    for i, (vk, key_up) in enumerate(events):
        flags = KEYEVENTF_KEYUP if key_up else 0
        inputs[i] = INPUT(type=INPUT_KEYBOARD, union=_INPUTunion(ki=KEYBDINPUT(vk, 0, flags, 0, 0)))
    return user32.SendInput(len(events), inputs, ctypes.sizeof(INPUT))


def _send_ctrl_v() -> int:
    """Returns how many of the 4 keyboard events went through (expected: 4)."""
    return _send_keys([(VK_CONTROL, False), (VK_V, False), (VK_V, True), (VK_CONTROL, True)])


def _is_key_down(vk: int) -> bool:
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


def _wait_modifiers_released(timeout_s: float = 1.0):
    """The hotkey is Ctrl+Shift+Space. If the user cuts the recording short
    with a second press, the paste fires ~200ms later and Shift is often
    still physically held down: the app receives Ctrl+Shift+V, which in many
    apps is not "paste" (VS Code opens the Markdown preview, for instance).
    This waits for Shift/Alt/Win to be released; if they aren't, it sends
    synthetic key-ups so Windows treats them as released during the combo."""
    stray = (VK_SHIFT, VK_MENU, VK_LWIN)
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if not any(_is_key_down(vk) for vk in stray):
            return
        time.sleep(0.02)
    _send_keys([(vk, True) for vk in stray if _is_key_down(vk)])


def _describe_window(hwnd: int) -> str:
    length = user32.GetWindowTextLengthW(hwnd)
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buf, length + 1)
    title = buf.value

    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    exe = "?"
    elevated = "?"
    process = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid.value)
    if process:
        size = wintypes.DWORD(1024)
        path_buf = ctypes.create_unicode_buffer(size.value)
        if kernel32.QueryFullProcessImageNameW(process, 0, path_buf, ctypes.byref(size)):
            exe = path_buf.value.rsplit("\\", 1)[-1]
        advapi32 = ctypes.WinDLL("advapi32", use_last_error=True)
        token = wintypes.HANDLE()
        if advapi32.OpenProcessToken(process, TOKEN_QUERY, ctypes.byref(token)):
            elevation = wintypes.DWORD()
            returned = wintypes.DWORD()
            if advapi32.GetTokenInformation(
                token, TokenElevation, ctypes.byref(elevation), ctypes.sizeof(elevation), ctypes.byref(returned)
            ):
                elevated = "yes" if elevation.value else "no"
            kernel32.CloseHandle(token)
        elif ctypes.get_last_error() == ERROR_ACCESS_DENIED:
            elevated = "yes (token access denied: runs with higher privileges than this daemon)"
        kernel32.CloseHandle(process)

    return f"hwnd={hwnd} title={title!r} exe={exe} elevated={elevated}"


def _self_is_admin() -> bool:
    return bool(ctypes.windll.shell32.IsUserAnAdmin())


def _open_clipboard(retries: int = 10, delay_s: float = 0.02) -> bool:
    """OpenClipboard fails if another process has it open (clipboard history,
    a clipboard manager, etc.). It's typically a fraction of a second, so a
    few retries is enough; without retrying this used to fail silently and
    the dictated text would go nowhere, with no error on the console."""
    for _ in range(retries):
        if user32.OpenClipboard(0):
            return True
        time.sleep(delay_s)
    return False


def _get_clipboard_text() -> str | None:
    """Reads CF_UNICODETEXT from the clipboard, or None if there's no text (or a different format)."""
    if not _open_clipboard():
        print("[clipboard] OpenClipboard failed while reading")
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


def _set_clipboard_text(text: str) -> bool:
    data = text.encode("utf-16-le") + b"\x00\x00"
    handle = kernel32.GlobalAlloc(GMEM_MOVEABLE, len(data))
    ptr = kernel32.GlobalLock(handle)
    ctypes.memmove(ptr, data, len(data))
    kernel32.GlobalUnlock(handle)

    if not _open_clipboard():
        print("[clipboard] OpenClipboard failed while writing, text lost")
        return False
    user32.EmptyClipboard()
    user32.SetClipboardData(CF_UNICODETEXT, handle)
    user32.CloseClipboard()
    return True


def copy_to_clipboard(text: str):
    """Leaves the text on the clipboard without pasting it (fallback if focus changed)."""
    _set_clipboard_text(text)


def paste_text_if_focus_unchanged(text: str, expected_hwnd: int, press_enter: bool = False) -> bool:
    """Pastes `text` only if the window focused when recording started is
    still the current one. `text` is left on the clipboard either way.

    Returns True if it pasted, False if focus changed and it was only left
    on the clipboard.
    """
    _set_clipboard_text(text)

    current = get_foreground_window()
    if current != expected_hwnd:
        print(f"[diag] focus changed: expected {_describe_window(expected_hwnd)} / current {_describe_window(current)}")
        return False

    _wait_modifiers_released()
    time.sleep(PASTE_DELAY_BEFORE_S)
    mods = (
        f"shift={_is_key_down(VK_SHIFT)} ctrl={_is_key_down(VK_CONTROL)} "
        f"alt={_is_key_down(VK_MENU)} win={_is_key_down(VK_LWIN)}"
    )
    sent = _send_ctrl_v()
    time.sleep(PASTE_DELAY_AFTER_S)
    print(
        f"[diag] target {_describe_window(expected_hwnd)} | daemon_admin={_self_is_admin()} "
        f"| modifiers at paste: {mods} | SendInput accepted {sent}/4 events"
    )
    if sent < 4:
        print(f"[diag] SendInput rejected events (GetLastError={ctypes.get_last_error()})")

    if sent == 4 and press_enter:
        # Give the app time to take the pasted text before submitting it.
        time.sleep(ENTER_DELAY_S)
        _send_keys([(VK_RETURN, False), (VK_RETURN, True)])
    return sent == 4


# --- wake word: paste into the last Claude Code window, wherever focus is ---

SW_RESTORE = 9
FOCUS_WAIT_S = 0.4
VK_TAB = 0x09
TAB_WAIT_S = 0.4  # time for the terminal to retitle itself after a tab switch
MAX_TABS = 15
# Claude Code titles its terminal "<glyph> <topic>": ✳ when idle, an
# animated symbol (◐ ◑ ✢ ✶ braille dots...) while it works. A plain shell's
# title starts with a letter or a path instead.
SYMBOL_CATEGORIES = ("So", "Sm", "Po")

user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.IsWindow.argtypes = [wintypes.HWND]
user32.IsIconic.argtypes = [wintypes.HWND]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetWindowTextLengthW.argtypes = [wintypes.HWND]
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]


def window_title(hwnd: int) -> str:
    length = user32.GetWindowTextLengthW(hwnd)
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buf, length + 1)
    return buf.value


def claude_topic(hwnd: int) -> str | None:
    """The session topic if the window currently shows Claude Code (by the
    title Claude Code gives its terminal), else None. The glyph is dropped:
    it animates, the topic doesn't. A terminal with several tabs only shows
    the active tab's title."""
    if not hwnd or not user32.IsWindow(hwnd):
        return None
    title = window_title(hwnd).strip()
    if len(title) > 2 and title[1] == " " and unicodedata.category(title[0]) in SYMBOL_CATEGORIES:
        return title[2:].strip()
    if "Claude Code" in title:
        return title
    return None


def is_claude_window(hwnd: int) -> bool:
    return claude_topic(hwnd) is not None


def _focus(hwnd: int) -> bool:
    """Brings `hwnd` to the front. Windows only lets the foreground process do
    that, so this borrows the foreground thread's input queue for a moment."""
    if user32.IsIconic(hwnd):
        user32.ShowWindow(hwnd, SW_RESTORE)
    current = get_foreground_window()
    this_thread = kernel32.GetCurrentThreadId()
    fg_thread = user32.GetWindowThreadProcessId(current, None) if current else 0
    attached = bool(fg_thread and fg_thread != this_thread and user32.AttachThreadInput(this_thread, fg_thread, True))
    try:
        user32.BringWindowToTop(hwnd)
        user32.SetForegroundWindow(hwnd)
    finally:
        if attached:
            user32.AttachThreadInput(this_thread, fg_thread, False)
    deadline = time.monotonic() + FOCUS_WAIT_S
    while time.monotonic() < deadline:
        if get_foreground_window() == hwnd:
            return True
        time.sleep(0.02)
    return False


def _switch_tab(hwnd: int, backwards: bool = False):
    """Ctrl+Tab / Ctrl+Shift+Tab: next / previous tab in Warp, Windows
    Terminal and most tabbed terminals. Waits for the title to follow."""
    before = window_title(hwnd)
    keys = [(VK_CONTROL, False)] + ([(VK_SHIFT, False)] if backwards else [])
    keys += [(VK_TAB, False), (VK_TAB, True)]
    keys += ([(VK_SHIFT, True)] if backwards else []) + [(VK_CONTROL, True)]
    _send_keys(keys)
    deadline = time.monotonic() + TAB_WAIT_S
    while time.monotonic() < deadline and window_title(hwnd) == before:
        time.sleep(0.02)


def _find_tab(hwnd: int, topic: str) -> int | None:
    """Cycles the window's tabs until the one titled `topic` is in front.
    Returns how many tabs it moved forward, or None (back where it started)."""
    start = window_title(hwnd)
    for steps in range(1, MAX_TABS + 1):
        _switch_tab(hwnd)
        if get_foreground_window() != hwnd:
            return None  # the user moved elsewhere; stop sending keys
        if claude_topic(hwnd) == topic:
            return steps
        if window_title(hwnd) == start:
            return None
    return None


def paste_into_window(text: str, hwnd: int, topic: str | None, press_enter: bool = False) -> bool:
    """Pastes `text` into the Claude Code session `topic` living in `hwnd`,
    even if the user is in another window or another tab of that terminal:
    jumps there (cycling tabs if needed), pastes (and presses Enter), then
    puts the tab and the focus back. `text` is left on the clipboard either
    way, and nothing is typed anywhere unless that session is found."""
    _set_clipboard_text(text)
    if not hwnd or not topic or not user32.IsWindow(hwnd):
        print("[diag] wake: no Claude Code window seen yet; text left on the clipboard")
        return False
    previous = get_foreground_window()
    if previous != hwnd and not _focus(hwnd):
        print(f"[diag] could not bring {_describe_window(hwnd)} to the front")
        return False
    _wait_modifiers_released()
    moved = 0
    if claude_topic(hwnd) != topic:
        moved = _find_tab(hwnd, topic)
        if moved is None:
            print(f"[diag] wake: no tab titled {topic!r} in {_describe_window(hwnd)}; text left on the clipboard")
            return False
    ok = paste_text_if_focus_unchanged(text, hwnd, press_enter=press_enter)
    time.sleep(ENTER_DELAY_S)
    for _ in range(moved):
        if get_foreground_window() != hwnd:
            break
        _switch_tab(hwnd, backwards=True)
    if previous and previous != hwnd and user32.IsWindow(previous):
        _focus(previous)
    return ok
