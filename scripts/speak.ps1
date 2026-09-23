# claudeTalk - speak.ps1
# 'Stop' hook: the safety net of talk mode. Claude is told (talk-context.ps1)
# to either answer short, which gets read here, or to speak through the `say`
# MCP tool. This script looks at the whole turn that just ended and decides:
#   - text written after Claude's last `say`, and short  -> read it out loud
#   - long text and Claude never spoke (or only spoke before doing work)
#                                                        -> "I left it on screen"
#   - long text right after a `say`                      -> nothing, `say` covered it
#
# Two modes:
#   (entry)    -> parses stdin, picks what to say, queues it and exits fast.
#   -Worker    -> hidden process that drains the speech queue: edge-tts streams
#                 mp3 into ffplay so audio starts before synthesis finishes.

param([switch]$Worker)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")

$MaxSpokenChars = 400

# ----------------- WORKER MODE (detached) -----------------
function Get-NextItem {
    Get-ChildItem $script:TalkQueue -Filter *.json -ErrorAction SilentlyContinue |
        Sort-Object Name | Select-Object -First 1
}

function Invoke-Item($file) {
    try {
        $item = [IO.File]::ReadAllText($file.FullName, [Text.Encoding]::UTF8) | ConvertFrom-Json
        $edge = Resolve-Edge $item.edge
        $ffplay = Resolve-Ffplay $item.ffplay
        if (-not $edge -or -not $ffplay) {
            Write-TalkLog "worker: edge or ffplay not found (edge=$edge ffplay=$ffplay)"
            return
        }
        $txt = Join-Path $env:TEMP ("claudetalk_txt_" + [guid]::NewGuid().ToString("N") + ".txt")
        [IO.File]::WriteAllText($txt, $item.text, (New-Object Text.UTF8Encoding($false)))
        try {
            # Cut while we were preparing: Stop-Speech deleted the item.
            if (-not (Test-Path $file.FullName)) { return }
            $line = '""{0}" --voice {1} --rate={2} --file "{3}" --write-media - 2>nul | "{4}" -nodisp -autoexit -loglevel quiet -i -"' -f `
                $edge, $item.voice, $item.rate, $txt, $ffplay
            $p = Start-Process -FilePath "cmd.exe" -ArgumentList "/d /s /c $line" -WindowStyle Hidden -PassThru
            Set-Content -Path $script:TalkPidFile -Value $p.Id
            $p.WaitForExit()
            # Done talking: the dictation's wake word listener (wake.py)
            # reads this file to stay deaf while Claude speaks.
            Remove-Item $script:TalkPidFile -Force -ErrorAction SilentlyContinue
        } finally {
            Remove-Item $txt -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Write-TalkLog "worker error: $_"
    } finally {
        Remove-Item $file.FullName -Force -ErrorAction SilentlyContinue
    }
}

if ($Worker) {
    $mutex = New-Object System.Threading.Mutex($false, "Local\claudetalk_speaker")
    while ($true) {
        $owned = $false
        try { $owned = $mutex.WaitOne(0) } catch [System.Threading.AbandonedMutexException] { $owned = $true }
        if (-not $owned) { exit 0 }  # another drainer is running and will play our item
        try {
            while ($next = Get-NextItem) { Invoke-Item $next }
        } finally {
            $mutex.ReleaseMutex()
        }
        # An item may have landed between the last check and the release.
        if (-not (Get-NextItem)) { break }
    }
    exit 0
}

# ----------------- ENTRY MODE (Stop hook) -----------------
function ConvertTo-Speech($text, $skipCode) {
    $t = $text
    if ($skipCode) {
        $t = [regex]::Replace($t, '(?s)```.*?```', ' ')
    } else {
        $t = $t -replace '```', ''
    }
    $t = [regex]::Replace($t, '`([^`]+)`', '$1')                 # inline code -> text
    $t = [regex]::Replace($t, '!?\[([^\]]+)\]\([^)]+\)', '$1')   # links/images -> text
    $t = [regex]::Replace($t, '(?m)^\s{0,3}#{1,6}\s*', '')       # headings
    $t = [regex]::Replace($t, '(?m)^\s*>\s?', '')                # quotes
    $t = [regex]::Replace($t, '(?m)^\s*[-*+]\s+', '')            # bullets
    $t = [regex]::Replace($t, '(?m)^\s*[-*_]{3,}\s*$', '')       # horizontal rules
    $t = [regex]::Replace($t, '(?m)^\s*\|.*\|\s*$', '')          # table rows
    $t = [regex]::Replace($t, '(?<!\w)[*_]{1,3}|[*_]{1,3}(?!\w)', '')  # bold/italic, keeps snake_case
    $t = $t -replace '`', ''
    $t = [regex]::Replace($t, '[ \t]+', ' ')
    $t = [regex]::Replace($t, '(\r?\n){2,}', '. ')
    return $t.Trim()
}

function Test-IsSay($block) {
    return ($block.type -eq "tool_use" -and $block.name -like "mcp__plugin_claudeTalk*__say")
}

# True when a transcript entry is a prompt the user typed (the start of a turn),
# as opposed to tool results, injected skill bodies or subagent traffic.
function Test-IsPrompt($obj) {
    if ($obj.type -ne "user" -or $obj.isMeta -or $obj.isSidechain) { return $false }
    $c = $obj.message.content
    if ($c -is [string]) { return $true }
    foreach ($b in $c) { if ($b.type -eq "tool_result") { return $false } }
    return $true
}

# Entries of the turn that just ended, walked in order: the text written after
# Claude's last `say`, whether it spoke at all, and whether it used other tools
# after speaking.
function Read-Turn($transcript) {
    $lines = Get-Content -Encoding UTF8 $transcript
    $turn = New-Object System.Collections.Generic.List[object]
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
        $line = $lines[$i]
        if (-not $line -or -not $line.Trim()) { continue }
        try { $obj = $line | ConvertFrom-Json } catch { continue }
        if (Test-IsPrompt $obj) { break }
        if ($obj.isSidechain) { continue }
        if ($obj.message -and $obj.message.role -eq "assistant") { $turn.Insert(0, $obj) }
    }
    $info = @{ spoke = $false; workAfterSay = $false; textAfter = "" }
    foreach ($obj in $turn) {
        foreach ($block in $obj.message.content) {
            if (Test-IsSay $block) {
                $info.spoke = $true; $info.workAfterSay = $false; $info.textAfter = ""
            } elseif ($block.type -eq "tool_use") {
                # Notes written between tool calls are progress chatter, not
                # the answer: only the text after the last tool gets read.
                $info.workAfterSay = $true; $info.textAfter = ""
            } elseif ($block.type -eq "text" -and $block.text) {
                $info.textAfter += $block.text + "`n"
            }
        }
    }
    return $info
}

try {
    $raw = Read-HookInput
    if (-not $raw) { exit 0 }
    $payload = $raw | ConvertFrom-Json
    $transcript = $payload.transcript_path
    if (-not $transcript -or -not (Test-Path $transcript)) { exit 0 }

    $state = Get-TalkState $payload.cwd
    if (-not $state -or -not $state.enabled) { exit 0 }

    # Claude Code can fire Stop a moment before the final message reaches the
    # transcript. Newer versions hand that message over as
    # last_assistant_message: wait until the transcript shows it. Older ones
    # don't, so give the write a head start and retry while the turn is empty.
    $last = ([string]$payload.last_assistant_message).Trim()
    $probe = if ($last.Length -gt 40) { $last.Substring(0, 40) } else { $last }
    Start-Sleep -Milliseconds 300
    $turnInfo = $null
    for ($try = 0; $try -lt 12; $try++) {
        $turnInfo = Read-Turn $transcript
        $found = if ($probe) { $turnInfo.textAfter.Contains($probe) } else { $turnInfo.spoke -or $turnInfo.textAfter.Trim() }
        if ($found) { break }
        Start-Sleep -Milliseconds 150
    }
    $spoke = $turnInfo.spoke
    $workAfterSay = $turnInfo.workAfterSay
    $textAfter = $turnInfo.textAfter
    if ($probe -and -not $textAfter.Contains($probe)) {
        # still not written: it is the final message, after any `say` or tool
        $textAfter = $last
        $workAfterSay = $false
    }

    $clean = ConvertTo-Speech $textAfter $state.skipCode
    if (-not $clean) { exit 0 }
    $fits = $clean.Length -le $MaxSpokenChars -and $textAfter -notmatch '```'

    if ($fits) {
        Add-Speech $clean $state
    } elseif (-not $spoke -or $workAfterSay) {
        Add-Speech ("Te dej" + [char]0x00E9 + " la respuesta en pantalla.") $state
    }
} catch {
    Write-TalkLog "entry error: $_"
}
exit 0
