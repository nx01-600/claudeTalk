# claudeTalk - install-app.ps1
# Creates a Start Menu shortcut so dictation can be launched like any other
# installed app (Windows key -> "claudeTalk" -> Enter), independent of
# Claude Code. Launched this way it stays in the tray until turned off.
# The dictation app is downloaded first if this version doesn't have it yet.
# Idempotent. Pass -Desktop to also put a shortcut on the desktop.

param([switch]$Desktop)

$root = Split-Path -Parent $PSScriptRoot
$claudetalk = Join-Path $root "bin\claudetalk.exe"
$exe = (& $claudetalk fetch-dictation | Select-Object -Last 1)
if ($LASTEXITCODE -ne 0 -or -not $exe -or -not (Test-Path $exe)) {
    Write-Error "could not get claudetalk-dictation.exe (see %TEMP%\claudetalk.log)"
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
