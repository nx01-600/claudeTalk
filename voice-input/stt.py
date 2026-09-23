"""Whisper model resident on GPU, with automatic unload on idle."""

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
                try:
                    self._model = WhisperModel(MODEL_NAME, device=DEVICE, compute_type=COMPUTE_TYPE)
                except (RuntimeError, ValueError, OSError) as exc:
                    # No usable NVIDIA/CUDA GPU: fall back to CPU with int8. Slower
                    # (seconds per sentence instead of tenths), but works on any machine.
                    self._notify(f"cuda unavailable ({exc.__class__.__name__}), using cpu")
                    self._model = WhisperModel(MODEL_NAME, device="cpu", compute_type="int8")
                self._notify("loaded")
            self._last_use = time.monotonic()

    def warm_up(self):
        """Loads the model ahead of time: the first transcription takes
        seconds if it has to be uploaded to VRAM at that moment."""
        self._ensure_loaded()

    def transcribe(self, audio, language: str | None = "es", use_prompt: bool = True) -> str:
        """language=None lets Whisper detect the language. use_prompt=False
        skips the vocabulary prompt (the wake word check: a prompt that says
        "Claude" would nudge Whisper into hearing it)."""
        self._ensure_loaded()
        segments, _info = self._model.transcribe(
            audio,
            language=language,
            beam_size=BEAM_SIZE,
            initial_prompt=(self._initial_prompt or None) if use_prompt else None,
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
