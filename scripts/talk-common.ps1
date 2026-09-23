# claudeTalk - talk-common.ps1
# Shared helpers, dot-sourced by speak.ps1, say-server.ps1, talk-context.ps1
# and voice-toggle.ps1.
#
# Speech goes through a queue so several phrases in one turn play in order
# instead of cutting each other off:
#   %TEMP%\claudetalk_queue\<ticks>.json   one item per phrase (text + voice)
#   speak.ps1 -Worker                      drains the queue; a named mutex keeps
#                                          a single drainer alive at a time
#   %TEMP%\claudetalk_player.pid           the cmd.exe running edge-tts | ffplay
# Stop-Speech empties the queue and kills the player: that is how a new prompt
# (or /talk off) cuts Claude off mid-sentence.

$script:TalkLog     = Join-Path $env:TEMP "claudetalk.log"
$script:TalkQueue   = Join-Path $env:TEMP "claudetalk_queue"
$script:TalkPidFile = Join-Path $env:TEMP "claudetalk_player.pid"
$script:TalkSpeak   = Join-Path $PSScriptRoot "speak.ps1"
# Exists while talk mode is on; the dictation daemon only listens for
# "Oye Claude" while it does (voice-input/wake.py).
$script:TalkFlag    = Join-Path $env:APPDATA "claudeTalk\talk-active.flag"

function Set-TalkFlag([bool]$on) {
    try {
        if ($on) {
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $script:TalkFlag) | Out-Null
            Set-Content -Path $script:TalkFlag -Value (Get-Date -Format o)
        } else {
            Remove-Item $script:TalkFlag -Force -ErrorAction SilentlyContinue
        }
    } catch {}
}

# Hook payloads arrive as UTF-8, but [Console]::In decodes them with the
# console's OEM code page: "ñ" and accents came out mangled and were then
# spoken wrong. Read the raw stream as UTF-8 instead.
function Read-HookInput {
    $reader = New-Object IO.StreamReader([Console]::OpenStandardInput(), (New-Object Text.UTF8Encoding($false)))
    return $reader.ReadToEnd()
}

function Write-TalkLog($msg) {
    try { "[$(Get-Date)] $msg" | Add-Content $script:TalkLog } catch {}
}

# The session's cwd moves when Claude cd's into a subfolder, so look for the
# state file from there up to the drive root; if there is none, it belongs in
# the project root (CLAUDE_PROJECT_DIR) or, failing that, the cwd.
function Get-TalkStateFile($cwd) {
    if (-not $cwd) { $cwd = (Get-Location).Path }
    $dir = $cwd
    while ($dir) {
        $candidate = Join-Path $dir ".claude\claudetalk.local.md"
        if (Test-Path $candidate) { return $candidate }
        $parent = Split-Path -Parent $dir
        if ($parent -eq $dir) { break }
        $dir = $parent
    }
    $root = if ($env:CLAUDE_PROJECT_DIR) { $env:CLAUDE_PROJECT_DIR } else { $cwd }
    return (Join-Path $root ".claude\claudetalk.local.md")
}

# The daemon starts every dictation it sends to Claude Code with this mark
# (SPOKEN_MARK in voice-input/daemon_cli.py): a prompt that starts with it
# was spoken, anything else was typed.
$SpokenMark = [char]::ConvertFromUtf32(0x1F399) + [char]0xFE0F

function Test-IsSpoken($prompt) {
    return ([string]$prompt).TrimStart().StartsWith($SpokenMark.Substring(0, 2), [StringComparison]::Ordinal)
}

# Reads the frontmatter of .claude/claudetalk.local.md. Returns $null if the
# file doesn't exist, otherwise a hashtable with every key filled in.
function Get-TalkState($cwd) {
    $file = Get-TalkStateFile $cwd
    if (-not (Test-Path $file)) { return $null }
    $raw = Get-Content -Raw -Encoding UTF8 $file
    $get = {
        param($key, $default)
        $pattern = '(?m)^\s*' + [regex]::Escape($key) + '\s*:\s*"?([^"\r\n]+?)"?\s*$'
        $m = [regex]::Match($raw, $pattern)
        if ($m.Success) { return $m.Groups[1].Value.Trim() }
        return $default
    }
    # Voice and speed are global, picked in the dictation gear panel
    # (%APPDATA%\claudeTalk\dictation.json). The project file only
    # supplies them when that panel was never used.
    $voice = & $get "voice" "es-CO-GonzaloNeural"
    $rate = & $get "rate" "+0%"
    $onlySpoken = $false
    # The daemon may be rewriting the file at this very moment; a failed read
    # used to fall back to the default voice for that one phrase.
    for ($try = 0; $try -lt 5; $try++) {
        try {
            $global = Get-Content -Raw -Encoding UTF8 (Join-Path $env:APPDATA "claudeTalk\dictation.json") | ConvertFrom-Json
            if ($global.tts_voice) { $voice = $global.tts_voice }
            if ($global.tts_rate) { $rate = $global.tts_rate }
            $onlySpoken = [bool]$global.speak_only_spoken
            break
        } catch { Start-Sleep -Milliseconds 40 }
    }
    return @{
        file     = $file
        enabled  = ((& $get "enabled" "false") -eq "true")
        voice    = $voice
        rate     = $rate
        skipCode = ((& $get "skip_code" "true") -eq "true")
        onlySpoken = $onlySpoken
        edge     = (& $get "edge_tts_path" "")
        ffplay   = (& $get "ffplay_path" "")
    }
}

function Resolve-Edge($p) {
    if ($p -and (Test-Path $p)) { return $p }
    $c = (Get-Command edge-tts.exe -ErrorAction SilentlyContinue).Source
    if ($c) { return $c }
    # Typical per-user Python install, any 3.x version
    $fb = Get-ChildItem (Join-Path $env:LOCALAPPDATA "Programs\Python\Python3*\Scripts\edge-tts.exe") -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1
    if ($fb) { return $fb.FullName }
    return $null
}

function Resolve-Ffplay($p) {
    if ($p -and (Test-Path $p)) { return $p }
    $c = (Get-Command ffplay.exe -ErrorAction SilentlyContinue).Source
    if ($c) { return $c }
    # ffmpeg installed via winget (Gyan.FFmpeg), any version
    $fb = Get-ChildItem (Join-Path $env:LOCALAPPDATA "Microsoft\WinGet\Packages\Gyan.FFmpeg*") -Filter ffplay.exe -Recurse -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($fb) { return $fb.FullName }
    return $null
}

# Queues a phrase and makes sure a drainer is running. Returns right away.
function Add-Speech($text, $state) {
    if (-not $text -or -not $text.Trim()) { return }
    New-Item -ItemType Directory -Force -Path $script:TalkQueue | Out-Null
    $item = @{ text = $text.Trim(); voice = $state.voice; rate = $state.rate; edge = $state.edge; ffplay = $state.ffplay }
    $name = "{0:D20}.json" -f [DateTime]::UtcNow.Ticks
    $json = $item | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $script:TalkQueue $name), $json, (New-Object Text.UTF8Encoding($false)))
    Start-Process -FilePath "powershell.exe" -WindowStyle Hidden -ArgumentList @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden",
        "-File", $script:TalkSpeak, "-Worker")
}

# Empties the queue and kills whatever is playing.
function Stop-Speech {
    Get-ChildItem $script:TalkQueue -Filter *.json -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    if (Test-Path $script:TalkPidFile) {
        $old = (Get-Content $script:TalkPidFile -ErrorAction SilentlyContinue | Select-Object -First 1)
        # Only kill if the PID is still our player (a stale PID may be reused).
        $proc = if ($old -match '^\d+$') { Get-Process -Id $old -ErrorAction SilentlyContinue }
        if ($proc -and ($proc.ProcessName -eq "cmd" -or $proc.ProcessName -eq "ffplay")) {
            Start-Process -FilePath "taskkill.exe" -ArgumentList "/T /F /PID $old" -WindowStyle Hidden -Wait
        }
        Remove-Item $script:TalkPidFile -Force -ErrorAction SilentlyContinue
    }
}
