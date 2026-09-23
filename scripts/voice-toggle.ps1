# claudeTalk - voice-toggle.ps1
# Turns talk mode on / off / flips it / queries it by writing
# .claude/claudetalk.local.md in the current workspace.
# Usage: voice-toggle.ps1 on|off|toggle|status

param([Parameter(Mandatory=$true)][ValidateSet("on","off","toggle","status")][string]$Action)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")
$file = Get-TalkStateFile (Get-Location).Path
$dir  = Split-Path -Parent $file

if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
if (-not (Test-Path $file)) {
@"
---
enabled: false
skip_code: true
---

# claudeTalk - talk mode status
Controls whether Claude talks to you out loud in this workspace.
Flip 'enabled' with /talk. Voice and speed are picked in the dictation
gear panel ("Claude's voice"), shared by every project.
"@ | Set-Content -Path $file -Encoding UTF8
}

$content = Get-Content -Raw $file

if ($Action -eq "toggle") {
    $on = [regex]::Match($content, '(?m)^\s*enabled\s*:\s*true\s*$').Success
    $Action = if ($on) { "off" } else { "on" }
}

switch ($Action) {
    "on" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}true'
        $content | Set-Content $file -Encoding UTF8
        Set-TalkFlag $true
        Write-Output "claudeTalk: talk mode ON (Claude talks to you until you run /talk again)."
    }
    "off" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}false'
        $content | Set-Content $file -Encoding UTF8
        Set-TalkFlag $false
        Stop-Speech
        Write-Output "claudeTalk: talk mode OFF (silence)."
    }
    "status" {
        $en = [regex]::Match($content, '(?m)^\s*enabled\s*:\s*(\S+)').Groups[1].Value
        $st = Get-TalkState (Get-Location).Path
        $estado = if ($en -eq 'true') { 'ON' } else { 'OFF' }
        Write-Output ("claudeTalk: talk mode " + $estado + " | voice: " + $st.voice + " | speed: " + $st.rate + " | file: " + $file)
    }
}
