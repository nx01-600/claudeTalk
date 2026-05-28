# claudeTalk - speak.ps1
# Hook 'Stop': lee la ultima respuesta del asistente desde el transcript y la
# reproduce en voz alta con edge-tts + ffplay, si el modo voz esta activo.
#
# Dos modos:
#   (entrada)  -> parsea stdin, valida estado, extrae y limpia el texto, lanza el worker desacoplado y sale rapido.
#   -Worker    -> proceso oculto que sintetiza (edge-tts) y reproduce (ffplay). No bloquea Claude Code.

param(
    [switch]$Worker,
    [string]$TextFile,
    [string]$Voice = "es-CO-GonzaloNeural",
    [string]$Rate = "+0%",
    [string]$EdgePath = "",
    [string]$FfplayPath = ""
)

$ErrorActionPreference = "Stop"
$log = Join-Path $env:TEMP "claudetalk.log"

function Resolve-Edge($p) {
    if ($p -and (Test-Path $p)) { return $p }
    $c = (Get-Command edge-tts.exe -ErrorAction SilentlyContinue).Source
    if ($c) { return $c }
    $fb = "C:\Users\nicol.HP-PAVILION\AppData\Local\Programs\Python\Python313\Scripts\edge-tts.exe"
    if (Test-Path $fb) { return $fb }
    return $null
}

function Resolve-Ffplay($p) {
    if ($p -and (Test-Path $p)) { return $p }
    $c = (Get-Command ffplay.exe -ErrorAction SilentlyContinue).Source
    if ($c) { return $c }
    $fb = "C:\Users\nicol.HP-PAVILION\AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-8.1.1-full_build\bin\ffplay.exe"
    if (Test-Path $fb) { return $fb }
    return $null
}

function Stop-PreviousPlayer {
    $pidFile = Join-Path $env:TEMP "claudetalk_player.pid"
    if (Test-Path $pidFile) {
        $old = Get-Content $pidFile -ErrorAction SilentlyContinue
        if ($old) { Stop-Process -Id $old -Force -ErrorAction SilentlyContinue }
    }
    return $pidFile
}

# ----------------- MODO WORKER (desacoplado) -----------------
if ($Worker) {
    try {
        $edge = Resolve-Edge $EdgePath
        $ffplay = Resolve-Ffplay $FfplayPath
        if (-not $edge -or -not $ffplay) {
            "[$(Get-Date)] worker: edge o ffplay no encontrados (edge=$edge ffplay=$ffplay)" | Add-Content $log
            exit 0
        }
        if (-not (Test-Path $TextFile)) { exit 0 }
        $text = Get-Content -Raw -Encoding UTF8 $TextFile
        if (-not $text -or $text.Trim() -eq "") { exit 0 }

        $mp3 = Join-Path $env:TEMP ("claudetalk_" + [guid]::NewGuid().ToString("N") + ".mp3")
        & $edge --voice $Voice --rate $Rate --file $TextFile --write-media $mp3 2>> $log
        if (-not (Test-Path $mp3)) { "[$(Get-Date)] worker: no se genero el mp3" | Add-Content $log; exit 0 }

        $pidFile = Stop-PreviousPlayer
        $p = Start-Process -FilePath $ffplay -ArgumentList "-nodisp","-autoexit","-loglevel","quiet",$mp3 -WindowStyle Hidden -PassThru
        Set-Content -Path $pidFile -Value $p.Id
    } catch {
        "[$(Get-Date)] worker error: $_" | Add-Content $log
    }
    exit 0
}

# ----------------- MODO ENTRADA (hook Stop) -----------------
try {
    $raw = [Console]::In.ReadToEnd()
    if (-not $raw) { exit 0 }
    $payload = $raw | ConvertFrom-Json

    $cwd = $payload.cwd
    if (-not $cwd) { $cwd = (Get-Location).Path }
    $transcript = $payload.transcript_path
    if (-not $transcript -or -not (Test-Path $transcript)) { exit 0 }

    # --- leer estado (.claude/claudetalk.local.md en el workspace) ---
    $stateFile = Join-Path $cwd ".claude\claudetalk.local.md"
    if (-not (Test-Path $stateFile)) { exit 0 }
    $state = Get-Content -Raw $stateFile

    function Get-Val($key, $default) {
        $pattern = '(?m)^\s*' + [regex]::Escape($key) + '\s*:\s*"?([^"\r\n]+?)"?\s*$'
        $m = [regex]::Match($state, $pattern)
        if ($m.Success) { return $m.Groups[1].Value.Trim() }
        return $default
    }

    if ((Get-Val "enabled" "false") -ne "true") { exit 0 }
    $voice      = Get-Val "voice" "es-CO-GonzaloNeural"
    $rate       = Get-Val "rate" "+0%"
    $skipCode   = (Get-Val "skip_code" "true") -eq "true"
    $edgePath   = Get-Val "edge_tts_path" ""
    $ffplayPath = Get-Val "ffplay_path" ""

    # --- extraer la ultima respuesta del asistente desde el transcript .jsonl ---
    $lines = Get-Content -Encoding UTF8 $transcript
    $assistantText = $null
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
        $line = $lines[$i]
        if (-not $line -or -not $line.Trim()) { continue }
        try { $obj = $line | ConvertFrom-Json } catch { continue }
        if (-not ($obj.message) -or $obj.message.role -ne "assistant") { continue }
        $content = $obj.message.content
        if (-not $content) { continue }
        $sb = ""
        foreach ($block in $content) {
            if ($block.type -eq "text" -and $block.text) { $sb += $block.text + "`n" }
        }
        if ($sb.Trim()) { $assistantText = $sb; break }
    }
    if (-not $assistantText) { exit 0 }

    # --- limpiar markdown / codigo para que suene natural ---
    $t = $assistantText
    if ($skipCode) {
        $t = [regex]::Replace($t, '(?s)```.*?```', ' bloque de codigo. ')
    } else {
        $t = $t -replace '```', ''
    }
    $t = [regex]::Replace($t, '`([^`]+)`', '$1')                 # inline code -> texto
    $t = [regex]::Replace($t, '!?\[([^\]]+)\]\([^)]+\)', '$1')   # links/imagenes -> texto
    $t = [regex]::Replace($t, '(?m)^\s{0,3}#{1,6}\s*', '')       # encabezados
    $t = [regex]::Replace($t, '(?m)^\s*>\s?', '')                # citas
    $t = [regex]::Replace($t, '(?m)^\s*[-*+]\s+', '')            # vinetas
    $t = [regex]::Replace($t, '(?m)^\s*[-*_]{3,}\s*$', '')       # lineas horizontales
    $t = $t -replace '[*_]{1,3}', ''                             # negrita/cursiva
    $t = $t -replace '`', ''                                     # backticks sueltos
    $t = [regex]::Replace($t, '[ \t]+', ' ')
    $t = [regex]::Replace($t, '(\r?\n){2,}', '. ')
    $t = $t.Trim()
    if (-not $t) { exit 0 }

    # --- escribir texto y lanzar el worker desacoplado (no bloquea la terminal) ---
    $textFile = Join-Path $env:TEMP ("claudetalk_txt_" + [guid]::NewGuid().ToString("N") + ".txt")
    Set-Content -Path $textFile -Value $t -Encoding UTF8

    $self = $MyInvocation.MyCommand.Path
    $psArgs = @("-NoProfile","-ExecutionPolicy","Bypass","-WindowStyle","Hidden","-File",$self,
                "-Worker","-TextFile",$textFile,"-Voice",$voice,"-Rate",$rate)
    if ($edgePath)   { $psArgs += @("-EdgePath",$edgePath) }
    if ($ffplayPath) { $psArgs += @("-FfplayPath",$ffplayPath) }
    Start-Process -FilePath "powershell.exe" -ArgumentList $psArgs -WindowStyle Hidden
} catch {
    "[$(Get-Date)] entry error: $_" | Add-Content $log
}
exit 0
