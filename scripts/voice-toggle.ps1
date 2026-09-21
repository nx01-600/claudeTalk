# claudeTalk - voice-toggle.ps1
# Turns voice mode on / off / queries it by writing .claude/claudetalk.local.md
# in the current workspace. Usage: voice-toggle.ps1 on|off|status

param([Parameter(Mandatory=$true)][ValidateSet("on","off","status")][string]$Action)

$ErrorActionPreference = "Stop"
$dir  = Join-Path (Get-Location).Path ".claude"
$file = Join-Path $dir "claudetalk.local.md"
$defaultVoice = "es-CO-GonzaloNeural"

if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
if (-not (Test-Path $file)) {
@"
---
enabled: false
voice: $defaultVoice
rate: "+0%"
skip_code: true
---

# claudeTalk - voice mode status
Controls whether claudeTalk reads Claude's responses out loud in this workspace.
Change 'enabled' with /voice-on and /voice-off. You can edit 'voice' and 'rate' by hand.
Neutral LATAM voices: es-CO-SalomeNeural, es-CO-GonzaloNeural, es-MX-DaliaNeural, es-MX-JorgeNeural.
"@ | Set-Content -Path $file -Encoding UTF8
}

$content = Get-Content -Raw $file

switch ($Action) {
    "on" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}true'
        $content | Set-Content $file -Encoding UTF8
        Write-Output "claudeTalk: voice mode ON (Claude will read its responses out loud)."
    }
    "off" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}false'
        $content | Set-Content $file -Encoding UTF8
        $pidFile = Join-Path $env:TEMP "claudetalk_player.pid"
        if (Test-Path $pidFile) {
            $old = Get-Content $pidFile -ErrorAction SilentlyContinue
            if ($old) { Stop-Process -Id $old -Force -ErrorAction SilentlyContinue }
        }
        Write-Output "claudeTalk: voice mode OFF (silence)."
    }
    "status" {
        $en = [regex]::Match($content, '(?m)^\s*enabled\s*:\s*(\S+)').Groups[1].Value
        $vo = [regex]::Match($content, '(?m)^\s*voice\s*:\s*(\S+)').Groups[1].Value
        $estado = if ($en -eq 'true') { 'ON' } else { 'OFF' }
        Write-Output ("claudeTalk: voice mode " + $estado + " | voice: " + $vo + " | file: " + $file)
    }
}
