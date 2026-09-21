"""Modelo Whisper residente en GPU, con descarga automatica por inactividad."""

import os
import sys
import threading
import time

_nvidia_base = os.path.abspath(
    os.path.join(os.path.dirname(sys.executable), "..", "Lib", "site-packages", "nvidia")
)
_dll_dirs = []
for _pkg in ("cudnn", "cublas"):
    _bin = os.path.join(_nvidia_base, _pkg, "bin")
    if os.path.isdir(_bin):
        os.add_dll_directory(_bin)
        _dll_dirs.append(_bin)
if _dll_dirs:
    os.environ["PATH"] = os.pathsep.join(_dll_dirs) + os.pathsep + os.environ["PATH"]

from faster_whisper import WhisperModel

MODEL_NAME = "large-v3-turbo"
DEVICE = "cuda"
COMPUTE_TYPE = "float16"
BEAM_SIZE = 1
IDLE_UNLOAD_SECONDS = 1800


class ResidentTranscriber:
    def __init__(self, initial_prompt: str = "", on_state_change=None):
        self._model = None
        self._lock = threading.Lock()
        self._last_use = 0.0
        self._initial_prompt = initial_prompt
        self._on_state_change = on_state_change
        self._stop = threading.Event()
        self._watchdog = threading.Thread(target=self._idle_watchdog, daemon=True)
        self._watchdog.start()

    def _notify(self, state: str):
        if self._on_state_change:
            self._on_state_change(state)

    def _ensure_loaded(self):
        with self._lock:
            if self._model is None:
                self._notify("loading")
                self._model = WhisperModel(MODEL_NAME, device=DEVICE, compute_type=COMPUTE_TYPE)
                self._notify("loaded")
            self._last_use = time.monotonic()

    def warm_up(self):
        """Carga el modelo por adelantado: la primera transcripcion tarda
        segundos si hay que subirlo a VRAM en ese momento."""
        self._ensure_loaded()

    def transcribe(self, audio) -> str:
        self._ensure_loaded()
        segments, _info = self._model.transcribe(
            audio,
            language="es",
            beam_size=BEAM_SIZE,
            initial_prompt=self._initial_prompt or None,
            vad_filter=True,
        )
        text = "".join(s.text for s in segments).strip()
        self._last_use = time.monotonic()
        return text

    def _idle_watchdog(self):
        while not self._stop.wait(30):
            with self._lock:
                if self._model is not None and (time.monotonic() - self._last_use) > IDLE_UNLOAD_SECONDS:
                    self._model = None
                    self._notify("unloaded")

    def shutdown(self):
        self._stop.set()
        with self._lock:
            self._model = None
