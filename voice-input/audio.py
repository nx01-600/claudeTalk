"""Audio capture with automatic cutoff on silence."""

import time

import numpy as np
import sounddevice as sd

SAMPLE_RATE = 16000
CHANNELS = 1
BLOCK_MS = 30
CALIBRATION_MS = 300
SILENCE_HOLD_MS = 2000
MIN_SPEECH_MS = 400
MAX_RECORDING_S = 60
SILENCE_MARGIN = 2.5  # multiple of the noise floor to consider "there is speech"


class RecordingCancelled(Exception):
    pass


def _rms(block: np.ndarray) -> float:
    return float(np.sqrt(np.mean(np.square(block.astype(np.float32)))))


def record_until_silence(should_cancel=None, on_level=None, silence_hold_ms: int = SILENCE_HOLD_MS) -> np.ndarray:
    """Records from the default microphone until sustained silence is detected.

    should_cancel: optional callable that returns True to cut the recording
    short (used by the second hotkey press or Esc).
    on_level: optional callable that receives a float in [0, 1] for each
    block, to feed a visual volume indicator (see overlay.py).
    silence_hold_ms: sustained silence that cuts the recording short (configurable).
    """
    block_size = int(SAMPLE_RATE * BLOCK_MS / 1000)
    blocks: list[np.ndarray] = []
    noise_floor_samples: list[float] = []
    noise_floor = None
    silence_ms = 0
    speech_ms = 0
    start = time.monotonic()

    with sd.InputStream(
        samplerate=SAMPLE_RATE, channels=CHANNELS, dtype="float32", blocksize=block_size
    ) as stream:
        while True:
            if should_cancel is not None and should_cancel():
                raise RecordingCancelled()

            elapsed_ms = (time.monotonic() - start) * 1000
            if elapsed_ms > MAX_RECORDING_S * 1000:
                break

            block, _ = stream.read(block_size)
            block = block[:, 0]
            level = _rms(block)
            blocks.append(block.copy())

            if elapsed_ms < CALIBRATION_MS:
                noise_floor_samples.append(level)
                if on_level is not None:
                    on_level(0.0)
                continue

            if noise_floor is None:
                noise_floor = max(np.mean(noise_floor_samples), 1e-4)

            if on_level is not None:
                on_level(min(1.0, level / (noise_floor * SILENCE_MARGIN * 3)))

            is_speech = level > noise_floor * SILENCE_MARGIN

            if is_speech:
                silence_ms = 0
                speech_ms += BLOCK_MS
            else:
                silence_ms += BLOCK_MS

            if speech_ms >= MIN_SPEECH_MS and silence_ms >= silence_hold_ms:
                break

    if not blocks:
        return np.zeros(0, dtype=np.float32)

    return np.concatenate(blocks)
