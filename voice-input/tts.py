"""Voice preview for the Claude's voice panel.

Speech itself lives in the plugin's PowerShell scripts (edge-tts streamed
into ffplay through a queue, see scripts/talk-common.ps1). This only asks
them to cut whatever is playing and say a sample phrase with the voice and
speed and volume just picked, so the panel never duplicates that pipeline.
"""

import subprocess
from pathlib import Path

COMMON = Path(__file__).resolve().parent.parent / "scripts" / "talk-common.ps1"
SAMPLE = "Hola, así sueno cuando te hablo."
CREATE_NO_WINDOW = 0x08000000


def _ps_quote(text: str) -> str:
    return "'" + text.replace("'", "''") + "'"


def preview(voice: str, rate: str, volume: int = 100):
    if not COMMON.exists():
        return
    command = (
        f". {_ps_quote(str(COMMON))}; Stop-Speech; "
        f"Add-Speech {_ps_quote(SAMPLE)} @{{ voice = {_ps_quote(voice)}; rate = {_ps_quote(rate)}; volume = {int(volume)}; edge = ''; ffplay = '' }}"
    )
    subprocess.Popen(
        ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command],
        creationflags=CREATE_NO_WINDOW,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
