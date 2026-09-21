' claudeTalk - dictado.vbs
' Lanza el dictado por voz a mano (la "app"), sin ventana de consola.
' Queda en la bandeja del sistema hasta que se apague desde la tuerca o la bandeja.
' Busca el Python del entorno virtual en voice-input\.venv o en %LOCALAPPDATA%\claudeTalk\venv.

Set sh = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

root = fso.GetParentFolderName(fso.GetParentFolderName(WScript.ScriptFullName))
script = root & "\voice-input\daemon_cli.py"

py = root & "\voice-input\.venv\Scripts\pythonw.exe"
If Not fso.FileExists(py) Then
    py = sh.ExpandEnvironmentStrings("%LOCALAPPDATA%") & "\claudeTalk\venv\Scripts\pythonw.exe"
End If
If Not fso.FileExists(py) Then
    MsgBox "Falta instalar el dictado. Ejecuta scripts\setup-voice.ps1 primero.", 48, "claudeTalk"
    WScript.Quit 1
End If

sh.CurrentDirectory = root & "\voice-input"
sh.Run """" & py & """ """ & script & """", 0, False
