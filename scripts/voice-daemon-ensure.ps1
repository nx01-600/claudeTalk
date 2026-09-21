# claudeTalk - voice-daemon-ensure.ps1
# 'SessionStart' hook: leaves voice dictation running in --auto mode if it
# isn't already running. Idempotent and fast: Claude Code doesn't wait on the daemon.
# The daemon shuts down on its own when no Claude Code window is left open.
#
# Looks for the interpreter in this order:
#   1. the path saved by setup-voice.ps1 in %APPDATA%\claudeTalk\venv-path.txt
#   2. <plugin>\voice-input\.venv        (install inside the repo)
#   3. %LOCALAPPDATA%\claudeTalk\venv    (what setup-voice.ps1 creates by default)
# If none exist, dictation isn't installed and this does nothing.
# Claude Code runs the plugin from its cache, not from the repo: that's why the
# venv is looked up outside the plugin and not only next to this script.

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
