# claudeTalk - setup-voice.ps1
# Instala el dictado por voz: crea un entorno virtual de Python e instala las
# dependencias (faster-whisper, PySide6, sounddevice, CUDA runtime). El modelo
# Whisper (~1.6 GB) se descarga solo la primera vez que se usa.
#
# Uso:  powershell -ExecutionPolicy Bypass -File scripts\setup-voice.ps1
#       (opcional) -VenvPath <carpeta>   por defecto %LOCALAPPDATA%\claudeTalk\venv
#       (opcional) -Shortcut             crea un acceso directo "claudeTalk Dictado" en el escritorio

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
    Write-Host "No se encontro Python 3.11-3.13. Instalalo desde https://www.python.org/downloads/ (marca 'Add to PATH')." -ForegroundColor Red
    exit 1
}

$venvPython = Join-Path $VenvPath "Scripts\python.exe"
if (-not (Test-Path $venvPython)) {
    Write-Host "Creando entorno virtual en $VenvPath ..."
    $parts = $python.Split(" ")
    & $parts[0] $parts[1..($parts.Length - 1)] -m venv $VenvPath
}

Write-Host "Instalando dependencias (puede tardar unos minutos) ..."
& $venvPython -m pip install --upgrade pip --quiet
& $venvPython -m pip install -r $requirements

if ($Shortcut) {
    $desktop = [Environment]::GetFolderPath("Desktop")
    $link = Join-Path $desktop "claudeTalk Dictado.lnk"
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut($link)
    $lnk.TargetPath = "wscript.exe"
    $lnk.Arguments = "`"" + (Join-Path $root "scripts\dictado.vbs") + "`""
    $lnk.WorkingDirectory = Join-Path $root "voice-input"
    $lnk.IconLocation = (Join-Path $VenvPath "Scripts\pythonw.exe") + ",0"
    $lnk.Description = "Dictado por voz de claudeTalk"
    $lnk.Save()
    Write-Host "Acceso directo creado: $link"
}

Write-Host ""
Write-Host "Listo. Para arrancar el dictado a mano: wscript scripts\dictado.vbs" -ForegroundColor Green
Write-Host "Con el plugin instalado en Claude Code arranca solo en cada sesion."
