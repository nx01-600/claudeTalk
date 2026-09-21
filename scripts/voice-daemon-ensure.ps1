# claudeTalk - voice-daemon-ensure.ps1
# Hook 'SessionStart': deja corriendo el dictado por voz en modo --auto si no
# esta ya corriendo. Idempotente y rapido: Claude Code no espera al daemon.
# El daemon se cierra solo cuando no queda ningun Claude Code abierto.
#
# Busca el interprete en este orden:
#   1. <plugin>\voice-input\.venv        (instalacion dentro del repo)
#   2. %LOCALAPPDATA%\claudeTalk\venv    (lo que crea scripts\setup-voice.ps1)
# Si no hay ninguno, el dictado no esta instalado y no hace nada.

$ErrorActionPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root "voice-input\daemon_cli.py"

$candidates = @(
    (Join-Path $root "voice-input\.venv\Scripts\pythonw.exe"),
    (Join-Path $env:LOCALAPPDATA "claudeTalk\venv\Scripts\pythonw.exe")
)
$py = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $py) { exit 0 }

$running = Get-CimInstance Win32_Process -Filter "Name='pythonw.exe' OR Name='python.exe'" |
    Where-Object { $_.CommandLine -like '*daemon_cli.py*' }
if ($running) { exit 0 }

Start-Process -FilePath $py -ArgumentList "`"$script`" --auto" -WorkingDirectory (Split-Path $script) -WindowStyle Hidden
exit 0
