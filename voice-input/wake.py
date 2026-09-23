"""Hands-free start: listens for "Oye Claude" and starts a dictation.

No extra model: the Whisper model already resident for dictation (stt.py)
doubles as the wake word detector. A cheap energy check splits the mic
stream into short sound bursts; only those bursts reach Whisper, and a
burst whose text starts with the wake phrase fires `on_wake`. Silence costs
nothing but reading the mic.

It only listens while `should_listen()` holds (the gear's toggle is on AND
talk mode is on: see TALK_FLAG), and steps aside while a dictation is
recording or transcribing (`is_busy`) and while Claude is talking, so
Claude's own voice coming out of the speakers can't wake it.
"""

import ctypes
import os
import re
import threading
import time
import unicodedata
from collections import deque
from pathlib import Path

import numpy as np
import sounddevice as sd

import audio

BLOCK_MS = audio.BLOCK_MS
PREROLL_MS = 300  # audio kept from before the burst, so "oye" isn't clipped
BURST_END_MS = 450  # silence that closes a burst
BURST_MIN_MS = 300  # shorter bursts are clicks and knocks
BURST_MAX_MS = 2500  # "oye claude" fits easily; longer is someone talking
FLOOR_WINDOW_MS = 4000  # noise floor = low percentile of this recent window
MIN_FLOOR = 1e-4
TTS_TAIL_S = 0.8  # stay deaf this long after Claude stops talking

# Whisper writes the name in many ways, and a clipped "oye" comes out as
# "y", "Roger" or anything else. So: either a call word right before the
# name at the start, or a short burst (up to SHORT_WORDS words) that ends in
# the name. A plain "claude" in the middle of a sentence doesn't count.
NAMES = r"(claude|claud|clod|clode|cloud|clau|claus|klaus|claudio|glod|klod|clo)"
WAKE_RE = re.compile(r"^(?:\w+\s+)?(oye|oy|oi|hey|ey|ei|oiga|okay|ok)\s+" + NAMES + r"\b")
WAKE_SHORT_RE = re.compile(NAMES + r"$")
SHORT_WORDS = 3
MAX_WAKE_MARGIN = 3.0  # a burst starts easier than a dictation, so "oye" isn't cut

_TEMP = Path(os.environ.get("TEMP", "."))
TTS_QUEUE = _TEMP / "claudetalk_queue"
TTS_PID = _TEMP / "claudetalk_player.pid"
# Written by scripts/voice-toggle.ps1 while talk mode is on (/talk).
TALK_FLAG = Path(os.environ.get("APPDATA", str(Path.home()))) / "claudeTalk" / "talk-active.flag"


def talk_mode_on() -> bool:
    return TALK_FLAG.exists()


def _normalize(text: str) -> str:
    text = unicodedata.normalize("NFKD", text.lower())
    text = "".join(c for c in text if not unicodedata.combining(c))
    return " ".join(re.sub(r"[^\w\s]", " ", text).split())


def is_wake_phrase(text: str) -> bool:
    text = _normalize(text)
    if WAKE_RE.search(text):
        return True
    words = text.split()
    return 0 < len(words) <= SHORT_WORDS and WAKE_SHORT_RE.fullmatch(words[-1]) is not None


_kernel32 = ctypes.WinDLL("kernel32")
_kernel32.OpenProcess.restype = ctypes.c_void_p
_kernel32.OpenProcess.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
_kernel32.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
_kernel32.CloseHandle.argtypes = [ctypes.c_void_p]
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
STILL_ACTIVE = 259


def _pid_alive(pid: int) -> bool:
    handle = _kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not handle:
        return False
    try:
        code = ctypes.c_ulong()
        return bool(_kernel32.GetExitCodeProcess(handle, ctypes.byref(code))) and code.value == STILL_ACTIVE
    finally:
        _kernel32.CloseHandle(handle)


def claude_is_talking() -> bool:
    """Talk mode queues phrases in %TEMP%\\claudetalk_queue and the player
    writes its PID; a queued phrase or a live player means Claude's voice may
    be on the speakers."""
    try:
        if any(TTS_QUEUE.glob("*.json")):
            return True
        pid = TTS_PID.read_text(encoding="utf-8", errors="ignore").strip()
    except OSError:
        return False
    return pid.isdigit() and _pid_alive(int(pid))


class WakeListener:
    def __init__(self, transcriber, on_wake, should_listen, is_busy, get_margin, get_language):
        self._transcriber = transcriber
        self._on_wake = on_wake
        self._should_listen = should_listen
        self._is_busy = is_busy
        self._get_margin = get_margin
        self._get_language = get_language
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self._thread.start()

    def stop(self):
        self._stop.set()

    def _paused(self) -> bool:
        return not self._should_listen() or self._is_busy()

    def _run(self):
        was_on = False
        while not self._stop.is_set():
            on = self._should_listen()
            if on != was_on:
                print(f"[wake] {'listening for Oye Claude' if on else 'off'}")
                was_on = on
            if self._paused():
                self._stop.wait(0.5)
                continue
            try:
                self._listen()
            except Exception as exc:  # device unplugged, driver hiccup...
                print(f"[wake] mic error: {exc!r}; retrying")
                time.sleep(2)

    def _listen(self):
        """Holds the mic open until paused; returns so a dictation can use it."""
        block_size = int(audio.SAMPLE_RATE * BLOCK_MS / 1000)
        preroll = deque(maxlen=PREROLL_MS // BLOCK_MS)
        recent = deque(maxlen=FLOOR_WINDOW_MS // BLOCK_MS)
        burst: list[np.ndarray] = []
        loud_ms = quiet_ms = 0
        deaf_until = 0.0

        with sd.InputStream(
            samplerate=audio.SAMPLE_RATE, channels=audio.CHANNELS, dtype="float32", blocksize=block_size
        ) as stream:
            while not self._stop.is_set() and not self._paused():
                block, _ = stream.read(block_size)
                block = block[:, 0].copy()
                level = audio._rms(block)

                if claude_is_talking():
                    deaf_until = time.monotonic() + TTS_TAIL_S
                if time.monotonic() < deaf_until:
                    burst.clear()
                    preroll.clear()
                    continue

                recent.append(level)
                if len(recent) < recent.maxlen // 4:
                    preroll.append(block)
                    continue
                floor = max(float(np.percentile(recent, 20)), MIN_FLOOR)
                loud = level > floor * min(self._get_margin(), MAX_WAKE_MARGIN)

                if not burst:
                    if loud:
                        burst = list(preroll) + [block]
                        loud_ms, quiet_ms = BLOCK_MS, 0
                    else:
                        preroll.append(block)
                    continue

                burst.append(block)
                if loud:
                    loud_ms += BLOCK_MS
                    quiet_ms = 0
                else:
                    quiet_ms += BLOCK_MS
                length_ms = len(burst) * BLOCK_MS
                if quiet_ms < BURST_END_MS and length_ms < BURST_MAX_MS:
                    continue

                pcm = np.concatenate(burst)
                burst = []
                preroll.clear()
                if loud_ms < BURST_MIN_MS:
                    continue
                if self._check(pcm):
                    return  # the dictation takes the mic from here

    def _check(self, pcm: np.ndarray) -> bool:
        if self._paused():
            return False
        try:
            text = self._transcriber.transcribe(pcm, language=self._get_language(), use_prompt=False)
        except Exception as exc:
            print(f"[wake] transcription failed: {exc!r}")
            return False
        if not text:
            return False
        hit = is_wake_phrase(text)
        print(f"[wake] heard {text!r}{' -> wake' if hit else ''}")
        if hit and not self._paused():
            self._on_wake()
            return True
        return False
