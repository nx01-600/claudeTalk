"""Daemon de dictado: hotkey -> graba (con overlay) -> transcribe -> pega.

Un toque del acorde (por defecto Alt izquierdo + Ctrl derecho) arranca a
grabar. Corta solo cuando hay silencio sostenido (configurable) o cuando se
vuelve a tocar el acorde (toggle, no hay que mantener apretado). Mientras
graba se ve la pildora flotante (overlay.py); su tuerca abre los ajustes.
Esc durante la grabacion: cancela y descarta.

Modos de arranque:
  daemon_cli.py          manual (la "app"): queda hasta que se apague.
  daemon_cli.py --auto   lo lanza el hook SessionStart del plugin; se cierra
                         solo cuando no queda ningun Claude Code abierto.

Hilos: el principal corre el loop de Qt (overlay, bandeja); el hotkey hace
polling en su propio hilo; cada grabacion corre en un hilo de trabajo. Todo
lo que toca la GUI pasa por senales Qt (thread-safe).
"""

import argparse
import ctypes
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

LOG_PATH = Path(os.environ.get("TEMP", ".")) / "claudetalk-dictado.log"


def _setup_console(log_to_file: bool):
    """Sin consola (pythonw) o con --log: todo va a un archivo en %TEMP%.
    Con consola: se pasa a UTF-8 (arranca en cp1252 y un titulo de ventana
    con emoji o el texto dictado con tildes rompia el print)."""
    if sys.stdout is None or log_to_file:
        stream = open(LOG_PATH, "a", encoding="utf-8", buffering=1)
        sys.stdout = stream
        sys.stderr = stream
        return
    ctypes.windll.kernel32.SetConsoleOutputCP(65001)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


parser = argparse.ArgumentParser(description="claudeTalk - dictado por voz para Claude Code")
parser.add_argument("--auto", action="store_true", help="lanzado por Claude Code; se cierra cuando no queda ninguno abierto")
parser.add_argument("--log", action="store_true", help="escribir la salida en %TEMP%\\claudetalk-dictado.log")
args = parser.parse_args()
_setup_console(args.log)

# Una sola instancia: si hubiera dos, las dos reaccionarian al mismo acorde.
ERROR_ALREADY_EXISTS = 183
_kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
_kernel32.CreateMutexW.restype = ctypes.c_void_p
_kernel32.CreateMutexW.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p]
_instance_mutex = _kernel32.CreateMutexW(None, False, "Local\\claudeTalk-dictado")
if ctypes.get_last_error() == ERROR_ALREADY_EXISTS:
    print("[info] ya hay una instancia del dictado corriendo; esta se cierra")
    sys.exit(0)

from PySide6.QtCore import QTimer
from PySide6.QtGui import QAction, QColor, QIcon, QPainter, QPainterPath, QPixmap
from PySide6.QtWidgets import QMenu, QMessageBox, QSystemTrayIcon

import audio
import config as cfg
import hotkey
import inject
import overlay
import sounds
from stt import ResidentTranscriber

INITIAL_PROMPT = (
    "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, "
    "branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress."
)
CLAUDE_CHECK_S = 5

config = cfg.Config()

state_lock = threading.Lock()
state = "idle"
force_stop = threading.Event()
cancel_flag = threading.Event()

transcriber = ResidentTranscriber(
    initial_prompt=INITIAL_PROMPT,
    on_state_change=lambda s: print(f"[modelo] {s}"),
)
threading.Thread(target=transcriber.warm_up, daemon=True).start()

app, bridge = overlay.create_app_and_overlay(config)


def _should_cancel():
    """Polling desde audio.record_until_silence (cada BLOCK_MS).

    Corte manual = segunda pulsacion del acorde (ver _on_press), que setea
    force_stop. Esc se detecta aca con GetAsyncKeyState.
    """
    if force_stop.is_set() or cancel_flag.is_set():
        return True
    if hotkey.is_key_down(hotkey.VK_ESCAPE):
        cancel_flag.set()
        return True
    return False


def _language():
    value = config.get("language")
    return None if value == "auto" else value


def _worker():
    global state
    hwnd = inject.get_foreground_window()
    bridge.recording_started.emit()
    if config.get("sound"):
        sounds.chime_start()
    print("[grabando] habla ahora...")
    try:
        pcm = audio.record_until_silence(
            should_cancel=_should_cancel,
            on_level=bridge.level_changed.emit,
            silence_hold_ms=int(config.get("silence_ms")),
        )
    except audio.RecordingCancelled:
        pcm = None
    bridge.recording_stopped.emit()

    if cancel_flag.is_set() or pcm is None or len(pcm) == 0:
        print("[cancelado] descartado")
        with state_lock:
            state = "idle"
        cancel_flag.clear()
        force_stop.clear()
        return

    print(f"[transcribiendo] {len(pcm) / audio.SAMPLE_RATE:.1f}s de audio")
    t0 = time.time()
    text = transcriber.transcribe(pcm, language=_language())
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


chord = hotkey.ChordHotkey(on_press=_on_press, keys=config.get("hotkey"))
chord.start()


def _on_settings_changed(key, value):
    if key == "hotkey":
        chord.set_keys(value)
        _refresh_tray_label()
    print(f"[ajustes] {key} = {value!r}")


bridge.settings_changed.connect(_on_settings_changed)
bridge.capture_started.connect(chord.pause)
bridge.capture_finished.connect(chord.resume)
bridge.quit_requested.connect(app.quit)


# --- bandeja ------------------------------------------------------------------


def _tray_icon() -> QIcon:
    size = 64
    pixmap = QPixmap(size, size)
    pixmap.fill(QColor(0, 0, 0, 0))
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    shape = QPainterPath()
    shape.addRoundedRect(4, 4, size - 8, size - 8, 16, 16)
    painter.fillPath(shape, QColor(255, 255, 255))
    painter.setPen(QColor(0, 0, 0, 60))
    painter.drawPath(shape)
    painter.setPen(QColor(0, 0, 0, 0))
    heights = [16, 26, 36, 26, 16]
    bar_w, gap = 6, 5
    total = len(heights) * bar_w + (len(heights) - 1) * gap
    x = (size - total) / 2
    for h in heights:
        bar = QPainterPath()
        bar.addRoundedRect(x, size / 2 - h / 2, bar_w, h, bar_w / 2, bar_w / 2)
        painter.fillPath(bar, QColor(0, 0, 0))
        x += bar_w + gap
    painter.end()
    return QIcon(pixmap)


tray = QSystemTrayIcon(_tray_icon(), app)
tray_menu = QMenu()
tray_label = QAction("", tray_menu)
tray_label.setEnabled(False)
tray_menu.addAction(tray_label)
tray_menu.addSeparator()
tray_settings = QAction("Ajustes", tray_menu)
tray_settings.triggered.connect(bridge.panel.open_standalone)
tray_menu.addAction(tray_settings)
tray_quit = QAction("Apagar dictado", tray_menu)


def _quit_from_tray():
    answer = QMessageBox.question(
        None,
        "claudeTalk",
        "¿Apagar el dictado por completo?",
        QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        QMessageBox.StandardButton.No,
    )
    if answer == QMessageBox.StandardButton.Yes:
        app.quit()


tray_quit.triggered.connect(_quit_from_tray)
tray_menu.addAction(tray_quit)
tray.setContextMenu(tray_menu)


def _refresh_tray_label():
    label = cfg.hotkey_label(config.get("hotkey"))
    tray_label.setText(f"Dictado: {label}")
    tray.setToolTip(f"claudeTalk dictado - {label}")


_refresh_tray_label()
tray.show()


# --- modo automatico: vive mientras haya un Claude Code abierto ----------------


def _claude_running() -> bool:
    result = subprocess.run(
        ["tasklist", "/FI", "IMAGENAME eq claude.exe", "/NH"],
        capture_output=True,
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    return b"claude.exe" in result.stdout.lower()


def _auto_watchdog():
    if not _claude_running():
        print("[auto] no queda ningun Claude Code abierto; el dictado se cierra")
        app.quit()


if args.auto:
    watchdog = QTimer()
    watchdog.timeout.connect(_auto_watchdog)
    watchdog.start(CLAUDE_CHECK_S * 1000)

app.aboutToQuit.connect(chord.stop)

# El loop nativo de Qt no le da chance al interprete de Python de atender
# senales (Ctrl+C) mientras no hay eventos de ventana; este timer inocuo lo
# despierta cada 200ms para que SIGINT no quede colgado.
_signal_pump = QTimer()
_signal_pump.timeout.connect(lambda: None)
_signal_pump.start(200)

print(f"Dictado listo: {cfg.hotkey_label(config.get('hotkey'))}. {'Modo auto (atado a Claude Code).' if args.auto else 'Ctrl+C o Apagar para salir.'}")
sys.exit(app.exec())
