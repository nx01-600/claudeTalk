# claudeTalk - setup-voice.ps1
# Installs voice dictation: creates a Python virtual environment and installs
# the dependencies (faster-whisper, PySide6, sounddevice, CUDA runtime). The
# Whisper model (~1.6 GB) downloads on its own the first time it's used.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts\setup-voice.ps1
#        (optional) -VenvPath <folder>   defaults to %LOCALAPPDATA%\claudeTalk\venv
#        (optional) -Shortcut            creates a "claudeTalk Dictation" shortcut on the desktop

param(
    [string]$VenvPath = (Join-Path $env:LOCALAPPDATA "claudeTalk\venv"),
    [switch]$Shortcut
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$requirements = Join-Path $root "voice-input\requirements.txt"

function Find-Python {
    foreach ($candidate in @("py -3.12", "py -3.11", "python3.12", "python3.11", "python")) {
        $parts = $candidate.Split(" ")
        try {
            $version = & $parts[0] $parts[1..($parts.Length - 1)] -c "import sys; print(sys.version_info[:2])" 2>$null
            if ($LASTEXITCODE -eq 0 -and $version -match "\((3), (1[1-3])\)") { return $candidate }
        } catch {}
    }
    return $null
}

$python = Find-Python
if (-not $python) {
    Write-Host "Python 3.11-3.13 not found. Install it from https://www.python.org/downloads/ (check 'Add to PATH')." -ForegroundColor Red
    exit 1
}

$venvPython = Join-Path $VenvPath "Scripts\python.exe"
if (-not (Test-Path $venvPython)) {
    Write-Host "Creating virtual environment at $VenvPath ..."
    $parts = $python.Split(" ")
    & $parts[0] $parts[1..($parts.Length - 1)] -m venv $VenvPath
}

Write-Host "Installing dependencies (this may take a few minutes) ..."
& $venvPython -m pip install --upgrade pip --quiet
& $venvPython -m pip install -r $requirements

# The hook and the launcher read this path: Claude Code runs the plugin from
# its cache, so the venv needs to be findable outside the plugin.
$stateDir = Join-Path $env:APPDATA "claudeTalk"
New-Item -ItemType Directory -Force -Path $stateDir | Out-Null
Set-Content -Path (Join-Path $stateDir "venv-path.txt") -Value $VenvPath -Encoding utf8

if ($Shortcut) {
    $desktop = [Environment]::GetFolderPath("Desktop")
    $link = Join-Path $desktop "claudeTalk Dictation.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut($link)
    $lnk.TargetPath = "wscript.exe"
    $lnk.Arguments = "`"" + (Join-Path $root "scripts\dictation.vbs") + "`""
    $lnk.WorkingDirectory = Join-Path $root "voice-input"
    $lnk.IconLocation = (Join-Path $VenvPath "Scripts\pythonw.exe") + ",0"
    $lnk.Description = "claudeTalk voice dictation"
    $lnk.Save()
    Write-Host "Shortcut created: $link"
}

Write-Host ""
Write-Host "Done. To start dictation by hand: wscript scripts\dictation.vbs" -ForegroundColor Green
Write-Host "With the plugin installed in Claude Code, it starts on its own with every session."
