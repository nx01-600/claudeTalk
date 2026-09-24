# claudeTalk - voice-toggle.ps1
# Turns talk mode on / off / flips it / queries it for THIS Claude Code session
# (see the sessions notes in talk-common.ps1), and changes any of the
# settings the gear panel edits (%APPDATA%\claudeTalk\dictation.json); the
# dictation app reloads that file by itself within a second.
# Usage: voice-toggle.ps1 on|off|toggle|status|stop|settings
#        voice-toggle.ps1 set <setting> <value>      (see $Help below)
#        voice-toggle.ps1 voice|rate|volume|silence <value> (shortcuts for set)

param(
    [Parameter(Mandatory=$true)][ValidateSet("on","off","toggle","status","stop","settings","set","voice","rate","volume","silence")][string]$Action,
    [string]$Value,
    [string]$Extra
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "talk-common.ps1")
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
$SettingsPath = Join-Path $env:APPDATA "claudeTalk\dictation.json"

$Help = @"
Settings (set <setting> <value>):
  voice        Salome | Gonzalo | Dalia | Jorge | Elena | Alonso   (Claude's voice in this session)
  rate         slow | normal | fast | faster | +N% | -N%   (Claude's speed)
  volume       0-100                                     (Claude's volume)
  silence      seconds, 0.5 to 10   (pause that ends a dictation)
  sensitivity  0 to 100   (mic sensitivity; higher picks up a softer voice)
  hotkey       keys joined by +, e.g. ctrl+shift+space, alt+f2, lctrl+lshift+space
  sound        on | off   (chime when recording starts)
  enter        on | off   (press Enter after pasting = send the message)
  wake         on | off   (start dictating by saying the wake phrase)
  phrase       any words, e.g. "Hola Jarvis"   (the wake phrase; default "Oye Claude")
  spoken       on | off   (talk mode answers out loud only dictated messages)
  share        on | off   (overlay visible in screen sharing)
  glass        0 to 100   (glass effect intensity)
  position     bottom | top
  language     es | en | auto   (dictation language)
"@

# Lowercase without accents, so "Salome", "salome" with an accent and
# "SALOME" all match.
function ConvertTo-Plain($s) {
    $d = ([string]$s).Trim().ToLowerInvariant().Normalize([Text.NormalizationForm]::FormD)
    return (-join ($d.ToCharArray() | Where-Object { [Globalization.CharUnicodeInfo]::GetUnicodeCategory($_) -ne 'NonSpacingMark' }))
}

function Read-Settings {
    $data = [ordered]@{}
    if (Test-Path $SettingsPath) {
        $obj = [IO.File]::ReadAllText($SettingsPath, [Text.Encoding]::UTF8) | ConvertFrom-Json
        foreach ($p in $obj.PSObject.Properties) { $data[$p.Name] = $p.Value }
    }
    return $data
}

function Set-DictationSetting($key, $value) {
    $data = Read-Settings
    if (-not (Test-Path $SettingsPath)) {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $SettingsPath) | Out-Null
    }
    $data[$key] = $value
    # Write-then-rename so the dictation app never reads half a file; no BOM
    # (Python's json would choke on it).
    $tmp = $SettingsPath + ".ps.tmp"
    [IO.File]::WriteAllText($tmp, ($data | ConvertTo-Json -Depth 5), (New-Object Text.UTF8Encoding($false)))
    Move-Item -Force $tmp $SettingsPath
}

function ConvertTo-Bool($v) {
    $p = ConvertTo-Plain $v
    if ($p -match '^(on|true|yes|si|1|activ|encend|prend)') { return $true }
    if ($p -match '^(off|false|no|0|desactiv|apag)') { return $false }
    throw "'$v' is not on/off."
}

function ConvertTo-Number($v, $min, $max) {
    $n = 0.0
    $text = ([string]$v) -replace ',', '.' -replace '%', ''
    if (-not [double]::TryParse($text, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$n)) {
        throw "'$v' is not a number."
    }
    return [math]::Min([math]::Max($n, $min), $max)
}

$KeyCodes = @{
    ctrl = 0x11; control = 0x11; shift = 0x10; alt = 0x12; win = 0x5B; windows = 0x5B
    lctrl = 0xA2; rctrl = 0xA3; lshift = 0xA0; rshift = 0xA1; lalt = 0xA4; ralt = 0xA5; altgr = 0xA5; lwin = 0x5B; rwin = 0x5C
    space = 0x20; espacio = 0x20; tab = 0x09; enter = 0x0D; esc = 0x1B; backspace = 0x08; capslock = 0x14
    insert = 0x2D; delete = 0x2E; home = 0x24; end = 0x23; pageup = 0x21; pagedown = 0x22
    left = 0x25; up = 0x26; right = 0x27; down = 0x28; menu = 0x5D
}

function ConvertTo-Hotkey($v) {
    $vks = @()
    foreach ($part in ((ConvertTo-Plain $v) -split '\+' | Where-Object { $_.Trim() })) {
        $k = $part -replace '\s', ''
        if ($KeyCodes.ContainsKey($k)) { $vks += $KeyCodes[$k] }
        elseif ($k -match '^f([1-9]|1[0-2])$') { $vks += 0x6F + [int]$Matches[1] }
        elseif ($k -match '^[a-z]$') { $vks += [int][char]$k.ToUpperInvariant() }
        elseif ($k -match '^[0-9]$') { $vks += 0x30 + [int]$k }
        else { throw "unknown key '$part'." }
    }
    if ($vks.Count -lt 2) { throw "a hotkey needs at least two keys, e.g. ctrl+shift+space." }
    return ,$vks
}

function Get-OnOff($b) { if ($b) { return "on" } return "off" }

# Returns @(json key, value, description) or throws with the reason.
function Resolve-Setting($name, $v) {
    $plain = ConvertTo-Plain $v
    $n = ConvertTo-Plain $name
    if ($n -in "voice", "voz") {
        $voices = [ordered]@{ salom = "es-CO-SalomeNeural"; gonzalo = "es-CO-GonzaloNeural"; dalia = "es-MX-DaliaNeural"; jorge = "es-MX-JorgeNeural"; elena = "es-AR-ElenaNeural"; alonso = "es-US-AlonsoNeural" }
        foreach ($k in $voices.Keys) { if ($plain.StartsWith($k)) { return @("tts_voice", $voices[$k], "voice $($voices[$k])") } }
        if ($v -match '^[a-z]{2}-[A-Z]{2}-\w+Neural$') { return @("tts_voice", $v, "voice $v") }
        throw "unknown voice '$v'. Options: Salome, Gonzalo, Dalia, Jorge, Elena, Alonso."
    }
    if ($n -in "rate", "speed", "velocidad") {
        $rates = @{ slow = "-15%"; lenta = "-15%"; normal = "+0%"; fast = "+20%"; rapida = "+20%"; faster = "+40%"; "muy rapida" = "+40%" }
        if ($rates.ContainsKey($plain)) { return @("tts_rate", $rates[$plain], "speed $($rates[$plain])") }
        if ($plain -match '^[+-]\d{1,3}%$') { return @("tts_rate", $plain, "speed $plain") }
        throw "unknown speed '$v'. Options: slow, normal, fast, faster, or like +10%."
    }
    if ($n -in "volume", "volumen") { $x = [int](ConvertTo-Number $v 0 100); return @("tts_volume", $x, "Claude's volume $x") }
    if ($n -in "silence", "silencio") {
        $ms = [int]([math]::Round((ConvertTo-Number $v 0.5 10) * 4) * 250)
        return @("silence_ms", $ms, ("silence cutoff {0:0.##} s" -f ($ms / 1000)))
    }
    if ($n -in "sensitivity", "sensibilidad") { $x = [int](ConvertTo-Number $v 0 100); return @("sensitivity", $x, "mic sensitivity $x") }
    if ($n -in "glass", "vidrio") { $x = [int](ConvertTo-Number $v 0 100); return @("glass", $x, "glass $x") }
    if ($n -in "hotkey", "keys", "teclas", "atajo") { $vk = ConvertTo-Hotkey $v; return @("hotkey", $vk, "hotkey $v") }
    if ($n -in "sound", "sonido") { $b = ConvertTo-Bool $v; return @("sound", $b, "start chime $(Get-OnOff $b)") }
    if ($n -in "enter", "auto_enter", "send") { $b = ConvertTo-Bool $v; return @("auto_enter", $b, "send with Enter $(Get-OnOff $b)") }
    if ($n -in "wake", "wake_word", "oye") { $b = ConvertTo-Bool $v; return @("wake_word", $b, "Oye Claude $(Get-OnOff $b)") }
    if ($n -in "phrase", "wake_phrase", "frase") {
        $p = (([string]$v).Trim() -replace '\s+', ' ')
        if (-not $p) { throw "the wake phrase can't be empty." }
        if ($p.Length -gt 40) { throw "the wake phrase is 40 characters at most." }
        return @("wake_phrase", $p, "wake phrase `"$p`"")
    }
    if ($n -in "spoken", "speak_only_spoken", "solo_voz") { $b = ConvertTo-Bool $v; return @("speak_only_spoken", $b, "speak only to dictated messages $(Get-OnOff $b)") }
    if ($n -in "share", "show_in_capture", "capture") { $b = ConvertTo-Bool $v; return @("show_in_capture", $b, "visible in screen share $(Get-OnOff $b)") }
    if ($n -in "theme", "tema") { throw "there is no theme setting any more: the overlay is always dark glass." }
    if ($n -in "position", "posicion") {
        if ($plain -match '^(bottom|abajo)') { return @("position", "bottom", "overlay at the bottom") }
        if ($plain -match '^(top|arriba)') { return @("position", "top", "overlay at the top") }
        throw "position is bottom or top."
    }
    if ($n -in "language", "idioma") {
        if ($plain -match '^(es|span|espa)') { return @("language", "es", "dictation in Spanish") }
        if ($plain -match '^(en|engl|ingl)') { return @("language", "en", "dictation in English") }
        if ($plain -match '^auto') { return @("language", "auto", "dictation language auto") }
        throw "language is es, en or auto."
    }
    throw "unknown setting '$name'.`n$Help"
}

if ($Action -in "voice", "rate", "volume", "silence") { $Extra = $Value; $Value = $Action; $Action = "set" }

if ($Action -eq "set") {
    try {
        $r = Resolve-Setting $Value $Extra
    } catch {
        Write-Output "claudeTalk: $($_.Exception.Message)"
        exit 1
    }
    $sid = Get-TalkSessionId
    if ($r[0] -eq "tts_voice" -and $sid) {
        # The voice belongs to this session. The session that speaks with the
        # gear voice (or the only one talking) also moves the gear along.
        $sess = Get-SessionState $sid
        $others = Get-OtherVoices $sid
        $followGear = $sess.follows_default -or -not $others.Count
        $sess.voice = $r[1]
        $sess.follows_default = $followGear
        Invoke-SessionLock { Set-SessionState $sid $sess } | Out-Null
        if ($followGear) { Set-DictationSetting $r[0] $r[1] }
        $note = if ($others -contains $r[1]) { " Another session already talks with that voice." } else { "" }
        Write-Output "claudeTalk: $($r[2]) for this session.$note Applied now."
        exit 0
    }
    Set-DictationSetting $r[0] $r[1]
    Write-Output "claudeTalk: $($r[2]). Applied now."
    exit 0
}
if ($Action -eq "settings") {
    Write-Output "claudeTalk settings ($SettingsPath):"
    foreach ($e in (Read-Settings).GetEnumerator()) {
        Write-Output ("  {0} = {1}" -f $e.Key, ($e.Value | ConvertTo-Json -Compress))
    }
    Write-Output $Help
    exit 0
}
if ($Action -eq "stop") {
    Stop-Speech (Get-TalkSessionId)
    Write-Output "claudeTalk: stopped talking (talk mode stays on)."
    exit 0
}

$sid = Get-TalkSessionId
if (-not $sid) {
    Write-Output "claudeTalk: can't tell which Claude Code session this is (run it from inside Claude Code)."
    exit 1
}
$sess = Get-SessionState $sid

if ($Action -eq "toggle") {
    $Action = if ($sess.enabled) { "off" } else { "on" }
}

switch ($Action) {
    "on" {
        $r = Enable-TalkSession $sid
        $name = Get-VoiceName $r.voice
        if ($r.own) {
            $busy = (@($r.others | Select-Object -Unique | ForEach-Object { Get-VoiceName $_ }) -join ", ")
            Write-Output "claudeTalk: talk mode ON for this session with its OWN voice: $name ($($r.voice)). Other sessions are talking with: $busy. Announce the voice (see the skill)."
        } else {
            Write-Output "claudeTalk: talk mode ON for this session (voice: $name). Claude talks to you until you run /talk again."
        }
    }
    "off" {
        Disable-TalkSession $sid
        Stop-Speech $sid
        Write-Output "claudeTalk: talk mode OFF for this session (silence)."
    }
    "status" {
        $estado = if ($sess.enabled) { 'ON' } else { 'OFF' }
        $voice = Get-EffectiveVoice $sess
        $others = Get-OtherVoices $sid
        $rate = (Get-TalkState (Get-Location).Path $sid).rate
        Write-Output ("claudeTalk: talk mode " + $estado + " in this session | voice: " + (Get-VoiceName $voice) + " (" + $voice + ") | speed: " + $rate + " | other sessions talking: " + $others.Count)
    }
}
