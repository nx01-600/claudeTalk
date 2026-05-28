# claudeTalk - voice-toggle.ps1
# Activa / desactiva / consulta el modo voz escribiendo .claude/claudetalk.local.md
# en el workspace actual. Uso: voice-toggle.ps1 on|off|status

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

# claudeTalk - estado del modo voz
Controla si claudeTalk lee en voz alta las respuestas de Claude en este workspace.
Cambia 'enabled' con /voz-on y /voz-off. Puedes editar 'voice' y 'rate' a mano.
Voces neutras LATAM: es-CO-SalomeNeural, es-CO-GonzaloNeural, es-MX-DaliaNeural, es-MX-JorgeNeural.
"@ | Set-Content -Path $file -Encoding UTF8
}

$content = Get-Content -Raw $file

switch ($Action) {
    "on" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}true'
        $content | Set-Content $file -Encoding UTF8
        Write-Output "claudeTalk: modo voz ON (Claude leera sus respuestas en voz alta)."
    }
    "off" {
        $content = $content -replace '(?m)^(\s*enabled\s*:\s*).*$', '${1}false'
        $content | Set-Content $file -Encoding UTF8
        $pidFile = Join-Path $env:TEMP "claudetalk_player.pid"
        if (Test-Path $pidFile) {
            $old = Get-Content $pidFile -ErrorAction SilentlyContinue
            if ($old) { Stop-Process -Id $old -Force -ErrorAction SilentlyContinue }
        }
        Write-Output "claudeTalk: modo voz OFF (silencio)."
    }
    "status" {
        $en = [regex]::Match($content, '(?m)^\s*enabled\s*:\s*(\S+)').Groups[1].Value
        $vo = [regex]::Match($content, '(?m)^\s*voice\s*:\s*(\S+)').Groups[1].Value
        $estado = if ($en -eq 'true') { 'ON' } else { 'OFF' }
        Write-Output ("claudeTalk: modo voz " + $estado + " | voz: " + $vo + " | archivo: " + $file)
    }
}
