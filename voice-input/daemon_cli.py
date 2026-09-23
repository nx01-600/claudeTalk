"""Dictation daemon: hotkey -> records (with overlay) -> transcribes -> pastes.

A single tap of the chord (Ctrl + Shift + Space by default) starts
recording. It cuts either on sustained silence (configurable) or when the
chord is tapped again (toggle, no need to hold it down). While recording,
the floating pill (overlay.py) is shown; its gear opens settings.
Esc during recording: cancels and discards.
While talk mode is on and the "Oye Claude" toggle is set, saying the wake
phrase starts a recording too (wake.py). That text always goes to the last
window that showed Claude Code, even if the user is elsewhere by then.

Start modes:
  daemon_cli.py          manual (the "app"): stays until turned off.
  daemon_cli.py --auto   launched by the plugin's SessionStart hook; closes
                         on its own once no Claude Code window is left.

Threads: the main thread runs the Qt loop (overlay, tray); the hotkey polls
on its own thread; each recording runs on a worker thread. Everything that
touches the GUI goes through Qt signals (thread-safe).
"""

import argparse
import ctypes
import os
import sys
import threading
import time
from pathlib import Path

LOG_PATH = Path(os.environ.get("TEMP", ".")) / "claudetalk-dictation.log"


def _setup_console(log_to_file: bool):
    """No console (pythonw) or with --log: everything goes to a file in
    %TEMP%. With a console: switch to UTF-8 (it starts in cp1252 and a
    window title with an emoji, or dictated text with accents, would break
    print)."""
    if sys.stdout is None or log_to_file:
        stream = open(LOG_PATH, "a", encoding="utf-8", buffering=1)
        sys.stdout = stream
        sys.stderr = stream
        return
    ctypes.windll.kernel32.SetConsoleOutputCP(65001)
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")


parser = argparse.ArgumentParser(description="claudeTalk - voice dictation for Claude Code")
parser.add_argument("--auto", action="store_true", help="launched by Claude Code; closes once none are left open")
parser.add_argument("--log", action="store_true", help="write output to %TEMP%\\claudetalk-dictation.log")
args = parser.parse_args()
_setup_console(args.log)

# Single instance: if there were two, both would react to the same chord.
ERROR_ALREADY_EXISTS = 183
_kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
_kernel32.CreateMutexW.restype = ctypes.c_void_p
_kernel32.CreateMutexW.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_wchar_p]
_instance_mutex = _kernel32.CreateMutexW(None, False, "Local\\claudeTalk-dictation")
if ctypes.get_last_error() == ERROR_ALREADY_EXISTS:
    print("[info] another dictation instance is already running; this one exits")
    sys.exit(0)

from PySide6.QtCore import QTimer
from PySide6.QtGui import QAction, QIcon
from PySide6.QtWidgets import QMenu, QMessageBox, QSystemTrayIcon

import audio
import config as cfg
import hotkey
import icon
import duck
import inject
import overlay
import sounds
import wake
from stt import ResidentTranscriber

INITIAL_PROMPTS = {
    "es": (
        "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, "
        "branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress."
    ),
    "en": (
        "Dictation in English for Claude Code: commit, repository, hook, pull request, "
        "branch, terminal, script, TypeScript, Docker, WordPress."
    ),
}
CLAUDE_CHECK_S = 5
WAKE_START_TIMEOUT_MS = 6000  # after "Oye Claude", give up if nothing is said
# Mic sensitivity (0..100 slider) -> (silence margin over the noise floor,
# fraction of the speech level below which sound counts as silence), linearly
# interpolated between these points. Lower sensitivity ignores more background
# talk (speakers, echo, other people); higher picks up a soft voice.
SENSITIVITY_POINTS = [
    (0, 6.0, 0.30),
    (25, 5.0, 0.25),
    (50, 3.5, 0.12),
    (75, 2.5, 0.05),
    (100, 1.8, 0.02),
]


def _sensitivity(value) -> tuple[float, float]:
    v = float(value) if isinstance(value, (int, float)) else 50.0
    v = max(0.0, min(100.0, v))
    for (x0, m0, r0), (x1, m1, r1) in zip(SENSITIVITY_POINTS, SENSITIVITY_POINTS[1:]):
        if v <= x1:
            t = (v - x0) / (x1 - x0)
            return m0 + (m1 - m0) * t, r0 + (r1 - r0) * t
    return SENSITIVITY_POINTS[-1][1], SENSITIVITY_POINTS[-1][2]

config = cfg.Config()

state_lock = threading.Lock()
state = "idle"
force_stop = threading.Event()
cancel_flag = threading.Event()

transcriber = ResidentTranscriber(
    initial_prompt=INITIAL_PROMPTS["es"],
    on_state_change=lambda s: print(f"[model] {s}"),
)
threading.Thread(target=transcriber.warm_up, daemon=True).start()

app, bridge = overlay.create_app_and_overlay(config)


def _should_cancel():
    """Polled from audio.record_until_silence (every BLOCK_MS).

    Manual cutoff = second chord press (see _on_press), which sets
    force_stop. Esc is detected here with GetAsyncKeyState.
    """
    if force_stop.is_set() or cancel_flag.is_set():
        return True
    if hotkey.is_key_down(hotkey.VK_ESCAPE):
        cancel_flag.set()
        return True
    return False


# Prefix of every dictation sent to Claude Code. scripts/talk-context.ps1 and
# scripts/speak.ps1 look for it: keep the three in sync.
SPOKEN_MARK = "🎙️ "


def _language():
    value = config.get("language")
    return None if value == "auto" else value


ducker = duck.Ducker()


def _worker(woken: bool = False):
    global state
    hwnd = inject.get_foreground_window()
    # Started on the desktop or the taskbar: nothing there takes text, so
    # the dictation goes to the last Claude session, like "Oye Claude".
    to_claude = woken or inject.is_shell_surface(hwnd)
    if to_claude and not woken:
        print("[target] focus is on the desktop/taskbar; sending to the last Claude session")
    target = last_claude_session
    if to_claude and not inject.claude_topic(target[0]):
        # Never seen in front since the daemon started (or that window is
        # gone): take the front-most Claude Code window on screen.
        target = inject.find_claude_window()
        print(f"[wake] no tracked Claude session; using {target[1]!r}")
    bridge.recording_started.emit()
    ducker.start()
    if config.get("sound"):
        sounds.chime_start()
    print("[recording] speak now...")
    try:
        margin, peak_ratio = _sensitivity(config.get("sensitivity"))
        pcm = audio.record_until_silence(
            should_cancel=_should_cancel,
            on_level=bridge.level_changed.emit,
            silence_hold_ms=int(config.get("silence_ms")),
            silence_margin=margin,
            peak_ratio=peak_ratio,
            start_timeout_ms=WAKE_START_TIMEOUT_MS if woken else None,
        )
        pcm = audio.noise_gate(pcm, peak_ratio)
    except audio.RecordingCancelled:
        pcm = None
    finally:
        ducker.stop()

    if cancel_flag.is_set() or pcm is None or len(pcm) == 0:
        bridge.recording_stopped.emit()
        print("[cancelled] discarded")
        with state_lock:
            state = "idle"
        cancel_flag.clear()
        force_stop.clear()
        return

    # The pill stays up while transcribing and pasting, then shows how it went.
    bridge.transcribing.emit()
    print(f"[transcribing] {len(pcm) / audio.SAMPLE_RATE:.1f}s of audio")
    t0 = time.time()
    transcriber._initial_prompt = INITIAL_PROMPTS.get(config.get("language"), "")
    text = transcriber.transcribe(pcm, language=_language())
    t1 = time.time()
    print(f"[text] {text!r} ({t1 - t0:.2f}s)")

    if not text:
        bridge.recording_stopped.emit()
        print("[empty] nothing to paste")
    else:
        if to_claude or inject.is_claude_window(hwnd):
            # Tells Claude (and the talk hooks) this prompt was spoken, not typed.
            text = SPOKEN_MARK + text
        if to_claude:
            ok = inject.paste_into_window(text, *target, press_enter=bool(config.get("auto_enter")))
        else:
            ok = inject.paste_text_if_focus_unchanged(text, hwnd, press_enter=bool(config.get("auto_enter")))
        bridge.finished.emit(ok)
        if ok:
            print("[pasted]")
        else:
            print("[not pasted] text left in the clipboard (see [diag] above)")

    force_stop.clear()
    with state_lock:
        state = "idle"


def _on_press():
    """Toggle: first press starts, second press cuts manually
    (while it's still recording/transcribing, any extra press just
    confirms the cutoff, nothing odd happens)."""
    global state
    with state_lock:
        if state == "idle":
            state = "recording"
            threading.Thread(target=_worker, daemon=True).start()
        else:
            force_stop.set()


# The last Claude Code session the user had in front, as (window, topic):
# where a dictation started by "Oye Claude" is sent, wherever the focus is
# by then. The topic tells the terminal's tabs apart.
last_claude_session = (0, None)
CLAUDE_WINDOW_POLL_MS = 500


def _track_claude_window():
    global last_claude_session
    hwnd = inject.get_foreground_window()
    topic = inject.claude_topic(hwnd)
    if topic and (hwnd, topic) != last_claude_session:
        last_claude_session = (hwnd, topic)
        print(f"[wake] Claude session: {topic!r}")


claude_window_timer = QTimer()
claude_window_timer.timeout.connect(_track_claude_window)
claude_window_timer.start(CLAUDE_WINDOW_POLL_MS)


def _on_wake():
    """Same as a first chord press, from the wake word listener's thread."""
    global state
    with state_lock:
        if state != "idle":
            return
        state = "recording"
    threading.Thread(target=_worker, kwargs={"woken": True}, daemon=True).start()


def _busy() -> bool:
    return state != "idle"


wake_listener = wake.WakeListener(
    transcriber,
    on_wake=_on_wake,
    should_listen=lambda: bool(config.get("wake_word")) and wake.talk_mode_on(),
    is_busy=_busy,
    get_margin=lambda: _sensitivity(config.get("sensitivity"))[0],
    get_language=_language,
)
wake_listener.start()

chord = hotkey.ChordHotkey(on_press=_on_press, keys=config.get("hotkey"))
chord.start()


def _on_settings_changed(key, value):
    if key == "hotkey":
        chord.set_keys(value)
        _refresh_tray_label()
    print(f"[settings] {key} = {value!r}")


bridge.settings_changed.connect(_on_settings_changed)


def _reload_config():
    """Picks up edits made outside the panel (Claude changing its voice)."""
    changed = config.reload_if_changed()
    if not changed:
        return
    for key, value in changed.items():
        bridge.settings_changed.emit(key, value)
    for panel in (bridge.panel, bridge.panel._companion):
        if panel is None:
            continue
        if panel.isVisible() and "glass" in changed:
            panel.refresh_background()
        panel.update()


config_watch = QTimer()
config_watch.timeout.connect(_reload_config)
config_watch.start(1000)
bridge.capture_started.connect(chord.pause)
bridge.capture_finished.connect(chord.resume)
bridge.quit_requested.connect(app.quit)


# --- tray ------------------------------------------------------------------


def _tray_icon() -> QIcon:
    return QIcon(icon.paint_icon(64))


tray = QSystemTrayIcon(_tray_icon(), app)
tray_menu = QMenu()
tray_label = QAction("", tray_menu)
tray_label.setEnabled(False)
tray_menu.addAction(tray_label)
tray_menu.addSeparator()
tray_settings = QAction("Settings", tray_menu)
tray_settings.triggered.connect(bridge.panel.open_standalone)
tray_menu.addAction(tray_settings)
tray_quit = QAction("Turn off dictation", tray_menu)


def _quit_from_tray():
    answer = QMessageBox.question(
        None,
        "claudeTalk",
        "Turn off dictation completely?",
        QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        QMessageBox.StandardButton.No,
    )
    if answer == QMessageBox.StandardButton.Yes:
        PERSISTENT_FLAG_PATH.unlink(missing_ok=True)
        app.quit()


tray_quit.triggered.connect(_quit_from_tray)
tray_menu.addAction(tray_quit)
tray.setContextMenu(tray_menu)


def _refresh_tray_label():
    label = cfg.hotkey_label(config.get("hotkey"))
    tray_label.setText(f"Dictation: {label}")
    tray.setToolTip(f"claudeTalk dictation - {label}")


_refresh_tray_label()
tray.show()


# --- auto mode: lives while a registered Claude Code session is alive -------
# The SessionStart hook (scripts/voice-daemon-ensure.ps1) appends the PID of
# each interactive claude.exe to sessions.txt. Headless subprocesses
# (`claude -p`, plugins' stream-json workers) are never registered, so they
# cannot keep the daemon alive after the user closes the last window. Simply
# counting claude.exe processes did exactly that.

SESSIONS_PATH = cfg.CONFIG_DIR / "sessions.txt"
PERSISTENT_FLAG_PATH = cfg.CONFIG_DIR / "persistent.flag"
STILL_ACTIVE = 259
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
STARTUP_GRACE_S = 30
_started_at = time.monotonic()

_kernel32.OpenProcess.restype = ctypes.c_void_p
_kernel32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
_kernel32.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
_kernel32.QueryFullProcessImageNameW.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_ulong)]
_kernel32.CloseHandle.argtypes = [ctypes.c_void_p]


def _is_live_claude(pid: int) -> bool:
    """Alive AND still claude.exe (guards against PID reuse by another program)."""
    handle = _kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        return False
    try:
        code = ctypes.c_ulong()
        if not _kernel32.GetExitCodeProcess(handle, ctypes.byref(code)) or code.value != STILL_ACTIVE:
            return False
        size = ctypes.c_ulong(1024)
        buf = ctypes.create_unicode_buffer(size.value)
        if not _kernel32.QueryFullProcessImageNameW(handle, 0, buf, ctypes.byref(size)):
            return False
        return buf.value.lower().endswith("claude.exe")
    finally:
        _kernel32.CloseHandle(handle)


def _live_sessions() -> list[int]:
    try:
        pids = [int(line) for line in SESSIONS_PATH.read_text(encoding="utf-8").split() if line.isdigit()]
    except OSError:
        return []
    alive = [pid for pid in pids if _is_live_claude(pid)]
    if alive != pids:
        try:
            SESSIONS_PATH.write_text("".join(f"{pid}\n" for pid in alive), encoding="utf-8")
        except OSError:
            pass
    return alive


def _auto_watchdog():
    if PERSISTENT_FLAG_PATH.exists():
        return  # launched as the standalone app; not tied to any Claude Code session
    if _live_sessions():
        return
    if time.monotonic() - _started_at < STARTUP_GRACE_S:
        return  # the hook may still be writing the first session
    print("[auto] no Claude Code session left; dictation exits")
    app.quit()


if args.auto:
    watchdog = QTimer()
    watchdog.timeout.connect(_auto_watchdog)
    watchdog.start(CLAUDE_CHECK_S * 1000)

app.aboutToQuit.connect(chord.stop)
app.aboutToQuit.connect(wake_listener.stop)

# Qt's native loop doesn't give the Python interpreter a chance to handle
# signals (Ctrl+C) while there are no window events; this harmless timer
# wakes it up every 200ms so SIGINT doesn't get stuck.
_signal_pump = QTimer()
_signal_pump.timeout.connect(lambda: None)
_signal_pump.start(200)

print(f"Dictation ready: {cfg.hotkey_label(config.get('hotkey'))}. {'Auto mode (tied to Claude Code).' if args.auto else 'Ctrl+C or Turn off to exit.'}")
sys.exit(app.exec())
