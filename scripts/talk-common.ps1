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
# Session id of whatever the player is playing right now, so /talk off in one
# session only cuts its own voice. Kept apart from the PID file because the
# wake listener (wake.py) reads that one as a bare number.
$script:TalkPlayerSession = Join-Path $env:TEMP "claudetalk_player.session"

# --- sessions ------------------------------------------------------------------
# Talk mode is per Claude Code session, and parallel sessions talk with
# different voices so the user can tell them apart by ear:
#   %APPDATA%\claudeTalk\sessions\<session_id>.json   { enabled, voice, follows_default }
#   %APPDATA%\claudeTalk\live.json                   { "<claude.exe PID>": "<session_id>" }
# Hooks get session_id in their payload. The `say` MCP server and
# voice-toggle.ps1 don't: they find the claude.exe they run under and look its
# PID up in live.json, which SessionStart (and, as a fallback, every prompt)
# keeps current. follows_default means the session speaks with whatever voice
# the gear panel has, so changing it there moves that session along.
$script:TalkStateDir    = Join-Path $env:APPDATA "claudeTalk"
$script:TalkSessionsDir = Join-Path $script:TalkStateDir "sessions"
$script:TalkLiveFile    = Join-Path $script:TalkStateDir "live.json"
$script:TalkSettings    = Join-Path $script:TalkStateDir "dictation.json"

# Handed out in this order to the second, third... session that turns talk
# mode on while others are talking: men and women alternate, and the accents
# (Colombian, Mexican, Argentine, US neutral) help tell them apart.
$script:TalkVoicePool = @(
    "es-CO-GonzaloNeural", "es-CO-SalomeNeural", "es-MX-JorgeNeural",
    "es-MX-DaliaNeural", "es-US-AlonsoNeural", "es-AR-ElenaNeural")
$script:TalkVoiceNames = @{
    "es-CO-SalomeNeural" = "Salom$([char]0xE9)"; "es-CO-GonzaloNeural" = "Gonzalo"
    "es-MX-DaliaNeural" = "Dalia"; "es-MX-JorgeNeural" = "Jorge"
    "es-AR-ElenaNeural" = "Elena"; "es-US-AlonsoNeural" = "Alonso"
}

function Get-VoiceName($voice) {
    if ($script:TalkVoiceNames.ContainsKey([string]$voice)) { return $script:TalkVoiceNames[$voice] }
    return [string]$voice
}

function Read-JsonFile($path) {
    # The daemon or another hook may be rewriting it right now.
    for ($try = 0; $try -lt 5; $try++) {
        if (-not (Test-Path $path)) { return $null }
        try { return ([IO.File]::ReadAllText($path, [Text.Encoding]::UTF8) | ConvertFrom-Json) }
        catch { Start-Sleep -Milliseconds 40 }
    }
    return $null
}

function Write-JsonFile($path, $obj) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    # Write-then-rename so readers never see half a file; no BOM.
    $tmp = "$path.$PID.tmp"
    [IO.File]::WriteAllText($tmp, ($obj | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
    Move-Item -Force $tmp $path
}

# Runs $block while holding the lock on the session files.
function Invoke-SessionLock([scriptblock]$block) {
    $mutex = New-Object System.Threading.Mutex($false, "Local\claudetalk_sessions")
    $owned = $false
    try {
        try { $owned = $mutex.WaitOne(3000) } catch [System.Threading.AbandonedMutexException] { $owned = $true }
        return (& $block)
    } finally {
        if ($owned) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}

# PID of the claude.exe this process runs under (hooks, the MCP server and the
# Bash tool are all its descendants), or $null.
function Get-ClaudePid {
    if ($script:ClaudePidCache) { return $script:ClaudePidCache }
    $current = Get-CimInstance Win32_Process -Filter "ProcessId=$PID" -ErrorAction SilentlyContinue
    for ($i = 0; $i -lt 12 -and $current; $i++) {
        $parent = Get-CimInstance Win32_Process -Filter "ProcessId=$($current.ParentProcessId)" -ErrorAction SilentlyContinue
        if (-not $parent) { break }
        if ($parent.Name -ieq "claude.exe") { $script:ClaudePidCache = [int]$parent.ProcessId; return $script:ClaudePidCache }
        $current = $parent
    }
    return $null
}

function Test-ClaudeAlive($procId) {
    $p = Get-Process -Id ([int]$procId) -ErrorAction SilentlyContinue
    return ($p -and $p.ProcessName -ieq "claude")
}

# live.json as a hashtable PID -> session_id, without dead sessions.
function Get-LiveSessions {
    $map = @{}
    $obj = Read-JsonFile $script:TalkLiveFile
    if ($obj) {
        foreach ($p in $obj.PSObject.Properties) {
            if ((Test-ClaudeAlive $p.Name) -and $p.Value) { $map[$p.Name] = [string]$p.Value }
        }
    }
    return $map
}

function Get-SessionFile($sid) { return (Join-Path $script:TalkSessionsDir ("{0}.json" -f ($sid -replace '[^\w-]', '_'))) }

function Get-SessionState($sid) {
    $s = if ($sid) { Read-JsonFile (Get-SessionFile $sid) } else { $null }
    return @{
        enabled         = [bool]($s -and $s.enabled)
        voice           = if ($s -and $s.voice) { [string]$s.voice } else { $null }
        follows_default = if ($s -and $null -ne $s.follows_default) { [bool]$s.follows_default } else { $true }
    }
}

function Set-SessionState($sid, $state) {
    Write-JsonFile (Get-SessionFile $sid) ([ordered]@{
        enabled = [bool]$state.enabled; voice = $state.voice
        follows_default = [bool]$state.follows_default; updated = (Get-Date -Format o) })
}

# Maps this process's claude.exe to $sid. When that claude.exe already had
# another session (after /clear, or /resume inside the same window), the new
# one inherits its talk mode and voice: it is the same conversation window.
function Register-TalkSession($sid) {
    if (-not $sid) { return }
    $claudePid = Get-ClaudePid
    if (-not $claudePid) { return }
    Invoke-SessionLock {
        $live = Get-LiveSessions
        $old = $live["$claudePid"]
        if ($old -and $old -ne $sid -and -not (Test-Path (Get-SessionFile $sid))) {
            $prev = Get-SessionState $old
            if ($prev.enabled -or $prev.voice) { Set-SessionState $sid $prev }
        }
        $live["$claudePid"] = $sid
        Write-JsonFile $script:TalkLiveFile $live
        # Forget sessions untouched for a month.
        Get-ChildItem $script:TalkSessionsDir -Filter *.json -ErrorAction SilentlyContinue |
            Where-Object { $_.LastWriteTime -lt (Get-Date).AddDays(-30) } |
            Remove-Item -Force -ErrorAction SilentlyContinue
    } | Out-Null
}

# The session this process belongs to: the hook payload's when there is one,
# otherwise the one live.json maps our claude.exe to. CLAUDETALK_SESSION_ID
# overrides both (tests).
function Get-TalkSessionId($payloadSid) {
    if ($env:CLAUDETALK_SESSION_ID) { return $env:CLAUDETALK_SESSION_ID }
    if ($payloadSid) { return [string]$payloadSid }
    $claudePid = Get-ClaudePid
    if (-not $claudePid) { return $null }
    $obj = Read-JsonFile $script:TalkLiveFile
    if ($obj -and $obj."$claudePid") { return [string]$obj."$claudePid" }
    return $null
}

# Voice the gear panel has (the preferred one).
function Get-DefaultVoice {
    $g = Read-JsonFile $script:TalkSettings
    if ($g -and $g.tts_voice) { return [string]$g.tts_voice }
    return "es-CO-GonzaloNeural"
}

function Get-EffectiveVoice($sess) {
    if ($sess.follows_default -or -not $sess.voice) { return (Get-DefaultVoice) }
    return $sess.voice
}

# Voices in use by the OTHER live sessions that have talk mode on.
function Get-OtherVoices($sid) {
    $voices = @()
    foreach ($other in ((Get-LiveSessions).Values | Select-Object -Unique)) {
        if ($other -eq $sid) { continue }
        $s = Get-SessionState $other
        if ($s.enabled) { $voices += (Get-EffectiveVoice $s) }
    }
    return ,$voices
}

# Turns talk mode on for $sid and gives it a voice nobody else is using.
# Returns @{ voice; own = $true when other sessions are talking and this one
# doesn't use the gear voice (worth announcing); others = their voices }.
function Enable-TalkSession($sid) {
    return (Invoke-SessionLock {
        $sess = Get-SessionState $sid
        $others = Get-OtherVoices $sid
        $default = Get-DefaultVoice
        $current = Get-EffectiveVoice $sess
        if ($sess.voice -and $others -notcontains $current) {
            $voice = $current
        } elseif ($others -notcontains $default) {
            $voice = $default; $sess.follows_default = $true
        } else {
            $free = @($script:TalkVoicePool | Where-Object { $others -notcontains $_ })
            if ($free.Count) { $voice = $free[0] }
            else {
                # More sessions than voices: repeat the least used one.
                $voice = $script:TalkVoicePool | Sort-Object { $v = $_; @($others | Where-Object { $_ -eq $v }).Count } | Select-Object -First 1
            }
            $sess.follows_default = $false
        }
        if ($voice -ne $default) { $sess.follows_default = $false }
        $sess.voice = $voice
        $sess.enabled = $true
        Set-SessionState $sid $sess
        Update-TalkFlag
        return @{ voice = $voice; own = ($voice -ne $default -and $others.Count -gt 0); others = $others }
    })
}

function Disable-TalkSession($sid) {
    Invoke-SessionLock {
        $sess = Get-SessionState $sid
        $sess.enabled = $false
        Set-SessionState $sid $sess
        Update-TalkFlag
    } | Out-Null
}

# "Oye Claude" listens while at least one live session has talk mode on.
function Update-TalkFlag {
    $any = $false
    foreach ($s in ((Get-LiveSessions).Values | Select-Object -Unique)) {
        if ((Get-SessionState $s).enabled) { $any = $true; break }
    }
    if ($env:CLAUDETALK_SESSION_ID -and (Get-SessionState $env:CLAUDETALK_SESSION_ID).enabled) { $any = $true }
    Set-TalkFlag $any
}

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

# Everything a hook needs to speak for session $sid: on/off and voice come from
# the session, speed and "speak only when I talk" from the gear panel
# (dictation.json), and skip_code / tool paths from the project's
# .claude/claudetalk.local.md when it exists. Its old 'enabled' key is no
# longer read: talk mode is per session now.
function Get-TalkState($cwd, $sid) {
    $file = Get-TalkStateFile $cwd
    $raw = if (Test-Path $file) { Get-Content -Raw -Encoding UTF8 $file } else { "" }
    $get = {
        param($key, $default)
        $pattern = '(?m)^\s*' + [regex]::Escape($key) + '\s*:\s*"?([^"\r\n]+?)"?\s*$'
        $m = [regex]::Match($raw, $pattern)
        if ($m.Success) { return $m.Groups[1].Value.Trim() }
        return $default
    }
    $rate = "+0%"
    $onlySpoken = $false
    $global = Read-JsonFile $script:TalkSettings
    if ($global) {
        if ($global.tts_rate) { $rate = $global.tts_rate }
        $onlySpoken = [bool]$global.speak_only_spoken
    }
    $sess = Get-SessionState $sid
    return @{
        file     = $file
        session  = $sid
        enabled  = ($sid -and $sess.enabled)
        voice    = (Get-EffectiveVoice $sess)
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
    $item = @{ text = $text.Trim(); voice = $state.voice; rate = $state.rate; edge = $state.edge; ffplay = $state.ffplay; session = $state.session }
    $name = "{0:D20}.json" -f [DateTime]::UtcNow.Ticks
    $json = $item | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $script:TalkQueue $name), $json, (New-Object Text.UTF8Encoding($false)))
    Start-Process -FilePath "powershell.exe" -WindowStyle Hidden -ArgumentList @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden",
        "-File", $script:TalkSpeak, "-Worker")
}

# Empties the queue and kills whatever is playing. With $sid, only that
# session's phrases: the other sessions keep talking.
function Stop-Speech($sid) {
    Get-ChildItem $script:TalkQueue -Filter *.json -ErrorAction SilentlyContinue | Where-Object {
        if (-not $sid) { return $true }
        $item = Read-JsonFile $_.FullName
        return (-not $item -or $item.session -eq $sid)
    } | Remove-Item -Force -ErrorAction SilentlyContinue
    if ($sid) {
        $playing = (Get-Content $script:TalkPlayerSession -ErrorAction SilentlyContinue | Select-Object -First 1)
        if ($playing -and $playing -ne $sid) { return }
    }
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
