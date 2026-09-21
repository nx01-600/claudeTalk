# claudeTalk - install-app.ps1
# Creates a Start Menu shortcut so dictation.vbs can be launched like any
# other installed app (Windows key -> "claudeTalk" -> Enter), independent of
# Claude Code. Idempotent: safe to re-run after moving the repo.

$root = Split-Path -Parent $PSScriptRoot
$vbs = Join-Path $root "scripts\dictation.vbs"
$iconPath = Join-Path $root "assets\claudetalk.ico"
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$shortcutPath = Join-Path $startMenu "claudeTalk.lnk"

if (-not (Test-Path $vbs)) {
    Write-Error "dictation.vbs not found at $vbs"
    exit 1
}
if (-not (Test-Path $iconPath)) {
    Write-Warning "assets\claudetalk.ico not found; run scripts\make-icon.py first. Shortcut will use a default icon."
}

$wscriptExe = Join-Path $env:WINDIR "System32\wscript.exe"

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $wscriptExe
$shortcut.Arguments = "`"$vbs`""
$shortcut.WorkingDirectory = $root
$shortcut.Description = "claudeTalk voice dictation"
if (Test-Path $iconPath) {
    $shortcut.IconLocation = "$iconPath,0"
}
$shortcut.Save()

Write-Output "Shortcut installed: $shortcutPath"
Write-Output "Press the Windows key and type 'claudeTalk' to launch it."
