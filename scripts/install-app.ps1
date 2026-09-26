# claudeTalk - install-app.ps1
# Creates a Start Menu shortcut so dictation can be launched like any other
# installed app (Windows key -> "claudeTalk" -> Enter), independent of
# Claude Code. Launched this way it stays in the tray until turned off.
# Idempotent: safe to re-run after moving the plugin. Pass -Desktop to also
# put a shortcut on the desktop.

param([switch]$Desktop)

$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "bin\claudetalk-dictation.exe"
if (-not (Test-Path $exe)) {
    Write-Error "claudetalk-dictation.exe not found at $exe"
    exit 1
}

$targets = @(Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\claudeTalk.lnk")
if ($Desktop) { $targets += (Join-Path ([Environment]::GetFolderPath("Desktop")) "claudeTalk Dictation.lnk") }

$shell = New-Object -ComObject WScript.Shell
foreach ($path in $targets) {
    $shortcut = $shell.CreateShortcut($path)
    $shortcut.TargetPath = $exe
    $shortcut.WorkingDirectory = $root
    $shortcut.Description = "claudeTalk voice dictation"
    $shortcut.IconLocation = "$exe,0"
    $shortcut.Save()
    Write-Output "Shortcut installed: $path"
}
Write-Output "Press the Windows key and type 'claudeTalk' to launch it."
