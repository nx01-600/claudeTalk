# claudeTalk - voice-daemon-ensure.ps1
# 'SessionStart' hook: registers this Claude Code session (for the daemon and
# for per-session talk mode) and leaves voice dictation running in --auto mode
# if it isn't already. Idempotent and fast:
# Claude Code doesn't wait on the daemon.
#
# Lifecycle: the hook walks up its parent chain to the claude.exe that started
# this session and appends that PID to %APPDATA%\claudeTalk\sessions.txt. The
# daemon (in --auto mode) checks every few seconds that at least one of those
# PIDs is still a live claude.exe and shuts down when none is. Headless
# Claude subprocesses (plugins spawning `claude -p` / `--output-format
# stream-json`) are ignored: they are not windows the user can dictate into
# and they would keep the daemon alive forever.
#
# Interpreter lookup order:
#   1. the path saved by setup-voice.ps1 in %APPDATA%\claudeTalk\venv-path.txt
#   2. <plugin>\voice-input\.venv        (install inside the repo)
#   3. %LOCALAPPDATA%\claudeTalk\venv    (what setup-voice.ps1 creates by default)
# If none exist, dictation isn't installed and this does nothing.
# Claude Code runs the plugin from its cache, not from the repo: that's why the
# venv is looked up outside the plugin and not only next to this script.

$ErrorActionPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root "voice-input\daemon_cli.py"
$stateDir = Join-Path $env:APPDATA "claudeTalk"
$sessionsFile = Join-Path $stateDir "sessions.txt"

# --- find the interactive claude.exe that owns this session --------------------
$claudePid = $null
$current = Get-CimInstance Win32_Process -Filter "ProcessId=$PID"
for ($i = 0; $i -lt 12 -and $current; $i++) {
    $parent = Get-CimInstance Win32_Process -Filter "ProcessId=$($current.ParentProcessId)"
    if (-not $parent) { break }
    if ($parent.Name -ieq "claude.exe") {
        $cmd = [string]$parent.CommandLine
        if ($cmd -match "--output-format" -or $cmd -match "(^|\s)-p(\s|$)" -or $cmd -match "--print") {
            exit 0  # headless session: not a dictation target
        }
        $claudePid = $parent.ProcessId
        break
    }
    $current = $parent
}
if (-not $claudePid) { exit 0 }

# --- tie this claude.exe to the session id (per-session talk mode) -------------
# The `say` server and /talk only know their claude.exe; live.json tells them
# which session that is. See the sessions notes in talk-common.ps1.
try {
    . (Join-Path $PSScriptRoot "talk-common.ps1")
    $payload = Read-HookInput | ConvertFrom-Json
    $script:ClaudePidCache = [int]$claudePid
    Register-TalkSession $payload.session_id
    Update-TalkFlag
} catch {}

New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
$known = @()
if (Test-Path $sessionsFile) { $known = Get-Content $sessionsFile | Where-Object { $_ -match "^\d+$" } }
if ($known -notcontains "$claudePid") { Add-Content -Path $sessionsFile -Value "$claudePid" }

# --- launch the daemon if needed ----------------------------------------------
$candidates = @()
$saved = Join-Path $stateDir "venv-path.txt"
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
