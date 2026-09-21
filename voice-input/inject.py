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
VK_SHIFT = 0x10
VK_MENU = 0x12
VK_LWIN = 0x5B
VK_V = 0x56

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
TOKEN_QUERY = 0x0008
TokenElevation = 20
ERROR_ACCESS_DENIED = 5

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


def _key_event(vk: int, key_up: bool) -> int:
    """Devuelve cuantos eventos inserto SendInput: 1 si entro, 0 si Windows
    lo bloqueo (UIPI: la ventana destino corre con mas privilegios que este
    proceso). Ese 0 es la unica senal que da Windows de que el paste no va a
    llegar; antes se ignoraba y quedaba como si hubiera pegado."""
    flags = KEYEVENTF_KEYUP if key_up else 0
    inp = INPUT(type=INPUT_KEYBOARD, union=_INPUTunion(ki=KEYBDINPUT(vk, 0, flags, 0, None)))
    return user32.SendInput(1, ctypes.byref(inp), ctypes.sizeof(INPUT))


def _send_keys(events: list[tuple[int, bool]]) -> int:
    """Manda varios eventos en UNA llamada a SendInput, para que ninguna tecla
    fisica se cuele en el medio de la combinacion. Devuelve cuantos entraron."""
    array_type = INPUT * len(events)
    inputs = array_type()
    for i, (vk, key_up) in enumerate(events):
        flags = KEYEVENTF_KEYUP if key_up else 0
        inputs[i] = INPUT(type=INPUT_KEYBOARD, union=_INPUTunion(ki=KEYBDINPUT(vk, 0, flags, 0, None)))
    return user32.SendInput(len(events), inputs, ctypes.sizeof(INPUT))


def _send_ctrl_v() -> int:
    """Devuelve cuantos de los 4 eventos de teclado entraron (esperado: 4)."""
    return _send_keys([(VK_CONTROL, False), (VK_V, False), (VK_V, True), (VK_CONTROL, True)])


def _is_key_down(vk: int) -> bool:
    return bool(user32.GetAsyncKeyState(vk) & 0x8000)


def _wait_modifiers_released(timeout_s: float = 1.0):
    """El hotkey es Ctrl+Shift+Espacio. Si el usuario corta la grabacion con
    una segunda pulsacion, el paste sale ~200ms despues y Shift suele seguir
    fisicamente apretado: la app recibe Ctrl+Shift+V, que en muchas apps no
    es "pegar" (VS Code abre el preview de Markdown, por ejemplo). Se espera
    a que suelte Shift/Alt/Win; si no los suelta, se mandan key-up sinteticos
    para que Windows los considere sueltos durante la combinacion."""
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
                elevated = "si" if elevation.value else "no"
            kernel32.CloseHandle(token)
        elif ctypes.get_last_error() == ERROR_ACCESS_DENIED:
            elevated = "si (acceso denegado al token: corre con mas privilegios que este daemon)"
        kernel32.CloseHandle(process)

    return f"hwnd={hwnd} titulo={title!r} exe={exe} elevado={elevated}"


def _self_is_admin() -> bool:
    return bool(ctypes.windll.shell32.IsUserAnAdmin())


def _open_clipboard(retries: int = 10, delay_s: float = 0.02) -> bool:
    """OpenClipboard falla si otro proceso lo tiene abierto (historial de
    portapapeles, un clipboard manager, etc.). Es tipicamente una fraccion
    de segundo, asi que reintentar unas pocas veces alcanza; sin retry esto
    fallaba en silencio y el texto dictado no llegaba a ningun lado, sin
    ningun error en consola."""
    for _ in range(retries):
        if user32.OpenClipboard(0):
            return True
        time.sleep(delay_s)
    return False


def _get_clipboard_text() -> str | None:
    """Lee CF_UNICODETEXT del portapapeles, o None si no hay texto (u otro formato)."""
    if not _open_clipboard():
        print("[portapapeles] OpenClipboard fallo al leer")
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
        print("[portapapeles] OpenClipboard fallo al escribir, texto perdido")
        return False
    user32.EmptyClipboard()
    user32.SetClipboardData(CF_UNICODETEXT, handle)
    user32.CloseClipboard()
    return True


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

    current = get_foreground_window()
    if current != expected_hwnd:
        print(f"[diag] foco cambio: esperado {_describe_window(expected_hwnd)} / actual {_describe_window(current)}")
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
        f"[diag] destino {_describe_window(expected_hwnd)} | daemon_admin={_self_is_admin()} "
        f"| modificadores al pegar: {mods} | SendInput acepto {sent}/4 eventos"
    )
    if sent < 4:
        print("[diag] Windows bloqueo el Ctrl+V (UIPI): la ventana destino corre elevada y el daemon no")

    return sent == 4
