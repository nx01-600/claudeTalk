# claudeTalk - voice-daemon-ensure.ps1
# Hook 'SessionStart': deja corriendo el dictado por voz en modo --auto si no
# esta ya corriendo. Idempotente y rapido: Claude Code no espera al daemon.
# El daemon se cierra solo cuando no queda ningun Claude Code abierto.
#
# Busca el interprete en este orden:
#   1. la ruta guardada por setup-voice.ps1 en %APPDATA%\claudeTalk\venv-path.txt
#   2. <plugin>\voice-input\.venv        (instalacion dentro del repo)
#   3. %LOCALAPPDATA%\claudeTalk\venv    (lo que crea setup-voice.ps1 por defecto)
# Si no hay ninguno, el dictado no esta instalado y no hace nada.
# Claude Code ejecuta el plugin desde su cache, no desde el repo: por eso el
# venv se busca fuera del plugin y no solo al lado de este script.

$ErrorActionPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root "voice-input\daemon_cli.py"

$candidates = @()
$saved = Join-Path $env:APPDATA "claudeTalk\venv-path.txt"
if (Test-Path $saved) { $candidates += (Join-Path (Get-Content $saved -Raw).Trim() "Scripts\pythonw.exe") }
$candidates += (Join-Path $root "voice-input\.venv\Scripts\pythonw.exe")
$candidates += (Join-Path $env:LOCALAPPDATA "claudeTalk\venv\Scripts\pythonw.exe")
$py = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
if (-not $py) { exit 0 }

$running = Get-CimInstance Win32_Process -Filter "Name='pythonw.exe' OR Name='python.exe'" |
    Where-Object { $_.CommandLine -like '*daemon_cli.py*' }
if ($running) { exit 0 }

Start-Process -FilePath $py -ArgumentList "`"$script`" --auto" -WorkingDirectory (Split-Path $script) -WindowStyle Hidden
exit 0
