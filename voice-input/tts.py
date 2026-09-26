"""Voice preview for the Claude's voice panel.

Speech itself lives in the plugin's native binary (bin/claudetalk.exe: Edge
TTS streamed through a queue). This only asks it to cut whatever is playing
and say a sample phrase with the voice, speed and volume just picked, so the
panel never duplicates that pipeline.
"""

import subprocess
from pathlib import Path

EXE = Path(__file__).resolve().parent.parent / "bin" / "claudetalk.exe"
SAMPLE = "voice"
# Played by the volume slider, so it sounds different from the voice/speed sample.
VOLUME_SAMPLE = "volume"
CREATE_NO_WINDOW = 0x08000000


def preview(voice: str, rate: str, volume: int = 100, sample: str = SAMPLE):
    if not EXE.exists():
        return
    subprocess.Popen(
        [str(EXE), "preview", voice, rate, str(int(volume)), sample],
        creationflags=CREATE_NO_WINDOW,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
