# claudeTalk - voice-toggle.ps1
# Turns talk mode on / off / flips it / queries it by writing
# .claude/claudetalk.local.md in the current workspace, and changes the
# shared settings the gear panel also edits (%APPDATA%\claudeTalk\dictation.json);
# the dictation app reloads that file by itself within a second.
# Usage: voice-toggle.ps1 on|off|toggle|status|stop
#        voice-toggle.ps1 voice Salome|Gonzalo|Dalia|Jorge|<edge-tts voice id>
#        voice-toggle.ps1 rate slow|normal|fast|faster|<+N%/-N%>
#        voice-toggle.ps1 silence <seconds>

param(
    [Parameter(Mandatory=$true)][ValidateSet("on","off","toggle","status","stop","voice","rate","silence")][string]$Action,
    [string]$Value
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")
$Voices = [ordered]@{
    salom   = "es-CO-SalomeNeural"
    gonzalo = "es-CO-GonzaloNeural"
    dalia   = "es-MX-DaliaNeural"
    jorge   = "es-MX-JorgeNeural"
}
$Rates = @{ slow = "-15%"; lenta = "-15%"; normal = "+0%"; fast = "+20%"; rapida = "+20%"; faster = "+40%"; "muy rapida" = "+40%" }

# Lowercase without accents, so "Salome", "salomé" and "SALOMÉ" all match.
function ConvertTo-Plain($s) {
    $d = ([string]$s).Trim().ToLowerInvariant().Normalize([Text.NormalizationForm]::FormD)
    return (-join ($d.ToCharArray() | Where-Object { [Globalization.CharUnicodeInfo]::GetUnicodeCategory($_) -ne 'NonSpacingMark' }))
}

function Set-DictationSetting($key, $value) {
    $path = Join-Path $env:APPDATA "claudeTalk\dictation.json"
    $data = [ordered]@{}
    if (Test-Path $path) {
        $obj = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8) | ConvertFrom-Json
        foreach ($p in $obj.PSObject.Properties) { $data[$p.Name] = $p.Value }
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path) | Out-Null
    }
    $data[$key] = $value
    # Write-then-rename so the dictation app never reads half a file; no BOM
    # (Python's json would choke on it).
    $tmp = $path + ".ps.tmp"
    [IO.File]::WriteAllText($tmp, ($data | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
    Move-Item -Force $tmp $path
}

if ($Action -eq "voice") {
    $plain = ConvertTo-Plain $Value
    $id = $null
    foreach ($k in $Voices.Keys) { if ($plain.StartsWith($k)) { $id = $Voices[$k] } }
    if (-not $id -and $Value -match '^[a-z]{2}-[A-Z]{2}-\w+Neural$') { $id = $Value }
    if (-not $id) { Write-Output "claudeTalk: unknown voice '$Value'. Options: Salome, Gonzalo, Dalia, Jorge."; exit 1 }
    Set-DictationSetting "tts_voice" $id
    Write-Output "claudeTalk: voice set to $id."
    exit 0
}
if ($Action -eq "rate") {
    $plain = ConvertTo-Plain $Value
    $rate = if ($Rates.ContainsKey($plain)) { $Rates[$plain] } elseif ($plain -match '^[+-]\d{1,3}%$') { $plain } else { $null }
    if (-not $rate) { Write-Output "claudeTalk: unknown speed '$Value'. Options: slow, normal, fast, faster, or like +10%."; exit 1 }
    Set-DictationSetting "tts_rate" $rate
    Write-Output "claudeTalk: speed set to $rate."
    exit 0
}
if ($Action -eq "silence") {
    $sec = 0.0
    if (-not [double]::TryParse(($Value -replace ',', '.'), [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$sec)) {
        Write-Output "claudeTalk: '$Value' is not a number of seconds."; exit 1
    }
    $ms = [int]([math]::Round([math]::Min([math]::Max($sec, 0.5), 10) * 4) * 250)
    Set-DictationSetting "silence_ms" $ms
    Write-Output ("claudeTalk: dictation stops after {0:0.##} s of silence." -f ($ms / 1000))
    exit 0
}
if ($Action -eq "stop") {
    Stop-Speech
    Write-Output "claudeTalk: stopped talking (talk mode stays on)."
    exit 0
}

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
