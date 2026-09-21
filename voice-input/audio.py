"""Audio capture with automatic cutoff on silence, plus a noise gate.

Silence is judged against two references at once: the noise floor measured
during the first CALIBRATION_MS, and the loudest speech heard so far in this
recording (the user's own voice, since the mic is closest to them). Anything
below `peak * peak_ratio` counts as silence even if it is above the noise
floor: that is what keeps other people's voices coming out of the speakers
(a Discord call, echo) from holding the recording open forever. The same
ratio drives `noise_gate`, which mutes those quiet stretches before
transcription so Whisper does not transcribe the background talk either.
The "Mic sensitivity" setting picks the margin/ratio pair.
"""

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
SILENCE_MARGIN = 3.5  # multiple of the noise floor to consider "there is speech"
PEAK_RATIO = 0.12  # fraction of the loudest block below which a block is silence


class RecordingCancelled(Exception):
    pass


def _rms(block: np.ndarray) -> float:
    return float(np.sqrt(np.mean(np.square(block.astype(np.float32)))))


def record_until_silence(
    should_cancel=None,
    on_level=None,
    silence_hold_ms: int = SILENCE_HOLD_MS,
    silence_margin: float = SILENCE_MARGIN,
    peak_ratio: float = PEAK_RATIO,
) -> np.ndarray:
    """Records from the default microphone until sustained silence is detected.

    should_cancel: optional callable that returns True to cut the recording
    short (used by the second hotkey press or Esc).
    on_level: optional callable that receives a float in [0, 1] for each
    block, to feed a visual volume indicator (see overlay.py).
    silence_hold_ms: sustained silence that cuts the recording short.
    silence_margin / peak_ratio: sensitivity (see module docstring).
    """
    block_size = int(SAMPLE_RATE * BLOCK_MS / 1000)
    blocks: list[np.ndarray] = []
    noise_floor_samples: list[float] = []
    noise_floor = None
    peak = 0.0
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

            floor_threshold = noise_floor * silence_margin
            if level > floor_threshold:
                peak = max(peak, level)
            threshold = max(floor_threshold, peak * peak_ratio)

            if on_level is not None:
                on_level(min(1.0, level / (threshold * 2.5)))

            is_speech = level > threshold

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


def noise_gate(pcm: np.ndarray, peak_ratio: float = PEAK_RATIO) -> np.ndarray:
    """Mutes every 30 ms block quieter than `peak_ratio` of the loudest block,
    keeping one block of context on each side so word edges survive."""
    if len(pcm) == 0:
        return pcm
    block_size = int(SAMPLE_RATE * BLOCK_MS / 1000)
    n_blocks = int(np.ceil(len(pcm) / block_size))
    padded = np.zeros(n_blocks * block_size, dtype=np.float32)
    padded[: len(pcm)] = pcm
    frames = padded.reshape(n_blocks, block_size)
    levels = np.sqrt(np.mean(np.square(frames), axis=1))
    peak = float(levels.max())
    if peak <= 0.0:
        return pcm
    keep = levels >= peak * peak_ratio
    keep = keep | np.roll(keep, 1) | np.roll(keep, -1)
    frames[~keep] = 0.0
    return frames.reshape(-1)[: len(pcm)]
