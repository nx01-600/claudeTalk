"""Turns Claude's voice down while the user is dictating.

Talk mode plays Claude's answers through ffplay (scripts/speak.ps1). If the
user starts dictating while Claude is still talking, the mic hears Claude
too and Whisper writes its words into the user's message. While a recording
runs, this keeps every ffplay audio session (per-app volume, the one in the
Windows volume mixer) at DUCK_LEVEL, including phrases that start playing
mid-recording, and drops a flag file so speak.ps1 reads any phrase that
starts meanwhile a bit slower. Everything goes back when the recording ends.
"""

import threading
import time

import comtypes
from pycaw.pycaw import AudioUtilities

import config as cfg

DUCK_LEVEL = 0.3
FADE_STEP = 0.14  # volume change per tick, so the dip and the return are smooth
TICK_S = 0.06  # while the volume is moving
IDLE_S = 0.4  # while it is where it should be
PLAYER_EXE = "ffplay.exe"
# Read by scripts/speak.ps1 (Invoke-Item): keep the name in sync.
FLAG_PATH = cfg.CONFIG_DIR / "ducking.flag"


def _player_volumes():
    """ISimpleAudioVolume of every ffplay audio session."""
    for session in AudioUtilities.GetAllSessions():
        try:
            if session.Process and session.Process.name().lower() == PLAYER_EXE:
                yield session.SimpleAudioVolume
        except Exception:
            pass  # the player exited between listing and asking


class Ducker:
    """One thread for the daemon's lifetime. Windows remembers an app's
    volume in the mixer, so a player that exited while ducked would make
    the next phrase start low too: the thread also lifts any ffplay session
    back to full volume whenever no recording is running."""

    def __init__(self):
        self._ducked = threading.Event()
        threading.Thread(target=self._run, daemon=True).start()

    def start(self):
        self._ducked.set()
        try:
            FLAG_PATH.touch()
        except OSError:
            pass

    def stop(self):
        self._ducked.clear()
        try:
            FLAG_PATH.unlink(missing_ok=True)
        except OSError:
            pass

    def _step(self) -> bool:
        """Moves every player one step toward its target. True if any moved."""
        target = DUCK_LEVEL if self._ducked.is_set() else 1.0
        moved = False
        for volume in _player_volumes():
            try:
                now = volume.GetMasterVolume()
                if abs(now - target) > 0.01:
                    step = max(-FADE_STEP, min(FADE_STEP, target - now))
                    volume.SetMasterVolume(now + step, None)
                    moved = True
            except Exception:
                pass
        return moved

    def _run(self):
        comtypes.CoInitialize()
        while True:
            try:
                moved = self._step()
            except Exception as e:
                print(f"[duck] {e}")
                moved = False
            time.sleep(TICK_S if moved or self._ducked.is_set() else IDLE_S)
