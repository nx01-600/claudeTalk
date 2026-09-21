"""Fase 1: nucleo funcional sin interfaz. Hotkey -> graba -> transcribe -> inyecta.

Ctrl+Shift+Espacio: mantener apretado graba, soltar corta (push-to-talk). El
corte por silencio sigue activo por si se suelta la tecla sin querer o se
prefiere no mantenerla.
Esc durante la grabacion: cancela y descarta.
"""

import threading
import time
import winsound

import audio
import hotkey
import inject
from stt import ResidentTranscriber

BEEP_START = (880, 90)
BEEP_STOP = (440, 90)
BEEP_INJECT = (1200, 60)
BEEP_CLIPBOARD = [(700, 70), (700, 70)]


def _beep(freq, dur):
    threading.Thread(target=winsound.Beep, args=(freq, dur), daemon=True).start()


def _beep_seq(pairs):
    def _run():
        for freq, dur in pairs:
            winsound.Beep(freq, dur)
            time.sleep(0.05)

    threading.Thread(target=_run, daemon=True).start()

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


def _should_cancel():
    """Polling desde audio.record_until_silence (cada BLOCK_MS).

    RegisterHotKey no tiene evento de soltar tecla, asi que el release y el
    Esc se detectan aca con GetAsyncKeyState en vez de callbacks separados.
    """
    if force_stop.is_set() or cancel_flag.is_set():
        return True
    if hotkey.is_key_down(hotkey.VK_ESCAPE):
        cancel_flag.set()
        return True
    if not hotkey.is_key_down(hotkey.VK_SPACE):
        force_stop.set()
        return True
    return False


def _worker():
    global state
    hwnd = inject.get_foreground_window()
    _beep(*BEEP_START)
    print("[grabando] habla ahora...")
    try:
        pcm = audio.record_until_silence(should_cancel=_should_cancel)
    except audio.RecordingCancelled:
        pcm = None
    _beep(*BEEP_STOP)

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
            _beep(*BEEP_INJECT)
            print("[inyectado]")
        else:
            _beep_seq(BEEP_CLIPBOARD)
            print("[foco cambio] texto copiado al portapapeles")

    force_stop.clear()
    with state_lock:
        state = "idle"


def _on_press():
    global state
    with state_lock:
        if state == "idle":
            state = "recording"
            threading.Thread(target=_worker, daemon=True).start()
        # si ya esta grabando, es auto-repeat de Windows por tener la tecla apretada: se ignora


global_hotkey = hotkey.GlobalHotkey(on_press=_on_press)
global_hotkey.start()

print(f"Escuchando {HOTKEY}. Ctrl+C para salir.")
try:
    threading.Event().wait()
except KeyboardInterrupt:
    global_hotkey.stop()
