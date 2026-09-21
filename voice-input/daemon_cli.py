"""Daemon de dictado: hotkey -> graba (con overlay) -> transcribe -> pega.

Ctrl+Shift+Espacio: un toque arranca a grabar. Corta solo cuando hay 2s de
silencio sostenido o cuando se vuelve a tocar el hotkey (toggle, no hay que
mantener apretado). Mientras graba se ve la pildora flotante (overlay.py).
Esc durante la grabacion: cancela y descarta.

Hilos: el principal corre el loop de Qt (overlay); el hotkey tiene su propio
message loop Win32; cada grabacion corre en un hilo de trabajo. Todo lo que
toca el overlay pasa por senales Qt (thread-safe).
"""

import ctypes
import sys
import threading
import time

# La consola de Windows arranca en cp1252: imprimir un titulo de ventana con
# emoji o el texto dictado con tildes rompia con UnicodeEncodeError o salia
# como "�". Se pasa la consola y stdout a UTF-8.
ctypes.windll.kernel32.SetConsoleOutputCP(65001)
sys.stdout.reconfigure(encoding="utf-8", errors="replace")

from PySide6.QtCore import QTimer

import audio
import hotkey
import inject
import overlay
import sounds
from stt import ResidentTranscriber

HOTKEY = "ctrl+shift+space"
INITIAL_PROMPT = (
    "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, "
    "branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress."
)

state_lock = threading.Lock()
state = "idle"
force_stop = threading.Event()
cancel_flag = threading.Event()

transcriber = ResidentTranscriber(
    initial_prompt=INITIAL_PROMPT,
    on_state_change=lambda s: print(f"[modelo] {s}"),
)

threading.Thread(target=transcriber.warm_up, daemon=True).start()

app, overlay_bridge = overlay.create_app_and_overlay()
overlay_bridge.settings_clicked.connect(lambda: print("[ajustes] todavia no hay panel de ajustes"))


def _should_cancel():
    """Polling desde audio.record_until_silence (cada BLOCK_MS).

    Corte manual = segunda pulsacion del hotkey (ver _on_press), que setea
    force_stop. Esc se detecta aca con GetAsyncKeyState.
    """
    if force_stop.is_set() or cancel_flag.is_set():
        return True
    if hotkey.is_key_down(hotkey.VK_ESCAPE):
        cancel_flag.set()
        return True
    return False


def _worker():
    global state
    hwnd = inject.get_foreground_window()
    overlay_bridge.recording_started.emit()
    sounds.chime_start()
    print("[grabando] habla ahora...")
    try:
        pcm = audio.record_until_silence(
            should_cancel=_should_cancel,
            on_level=overlay_bridge.level_changed.emit,
        )
    except audio.RecordingCancelled:
        pcm = None
    overlay_bridge.recording_stopped.emit()

    if cancel_flag.is_set() or pcm is None or len(pcm) == 0:
        print("[cancelado] descartado")
        with state_lock:
            state = "idle"
        cancel_flag.clear()
        force_stop.clear()
        return

    print(f"[transcribiendo] {len(pcm) / audio.SAMPLE_RATE:.1f}s de audio")
    t0 = time.time()
    text = transcriber.transcribe(pcm)
    t1 = time.time()
    print(f"[texto] {text!r} ({t1 - t0:.2f}s)")

    if not text:
        print("[vacio] nada que inyectar")
    else:
        ok = inject.paste_text_if_focus_unchanged(text, hwnd)
        if ok:
            print("[inyectado]")
        else:
            print("[no pegado] texto queda en el portapapeles (ver [diag] arriba)")

    force_stop.clear()
    with state_lock:
        state = "idle"


def _on_press():
    """Toggle: primera pulsacion arranca, segunda corta manualmente
    (mientras se sigue grabando/transcribiendo, cualquier pulsacion extra
    solo confirma el corte, no pasa nada raro)."""
    global state
    with state_lock:
        if state == "idle":
            state = "recording"
            threading.Thread(target=_worker, daemon=True).start()
        else:
            force_stop.set()


global_hotkey = hotkey.GlobalHotkey(on_press=_on_press)
try:
    global_hotkey.start()
except OSError as exc:
    print(f"[error] no se pudo registrar {HOTKEY}: {exc}")
    print("[error] casi seguro ya hay otra instancia de daemon_cli.py corriendo; cerrala y volve a lanzar")
    sys.exit(1)

print(f"Escuchando {HOTKEY}. Ctrl+C para salir.")
app.aboutToQuit.connect(global_hotkey.stop)

# El loop nativo de Qt no le da chance al interprete de Python de atender
# senales (Ctrl+C) mientras no hay eventos de ventana; este timer inocuo lo
# despierta cada 200ms para que SIGINT no quede colgado.
_signal_pump = QTimer()
_signal_pump.timeout.connect(lambda: None)
_signal_pump.start(200)

sys.exit(app.exec())
