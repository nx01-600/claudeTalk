"""Soft synthesized sounds: a sine wave with a slight harmonic and an
attack/decay envelope, played back with sounddevice. Replaces winsound.Beep,
which is a square wave at full volume (shrill) and blocks the thread while
it plays.
"""

import numpy as np
import sounddevice as sd

SAMPLE_RATE = 44100
VOLUME = 0.16


def _tone(freq: float, seconds: float, attack: float = 0.008, decay: float | None = None) -> np.ndarray:
    n = int(SAMPLE_RATE * seconds)
    t = np.linspace(0.0, seconds, n, endpoint=False)
    envelope = np.exp(-t / (decay or seconds / 3.0))
    a = max(1, int(SAMPLE_RATE * attack))
    envelope[:a] *= np.linspace(0.0, 1.0, a)
    wave = np.sin(2 * np.pi * freq * t) + 0.22 * np.sin(2 * np.pi * freq * 2 * t)
    return (wave * envelope * VOLUME).astype(np.float32)


def chime_start():
    """Two short, soft ascending notes (E5 -> A5), overlapping."""
    first = _tone(659.25, 0.16)
    second = _tone(880.0, 0.28)
    overlap = int(SAMPLE_RATE * 0.06)
    out = np.zeros(len(first) + len(second) - overlap, dtype=np.float32)
    out[: len(first)] += first
    out[len(first) - overlap :] += second
    sd.play(out, SAMPLE_RATE)
