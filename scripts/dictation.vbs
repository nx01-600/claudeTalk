' claudeTalk - dictation.vbs
' Launches voice dictation by hand (the "app"), with no console window.
' Sits in the system tray until turned off from the gear or the tray icon.
' Looks for the Python of the virtual environment in voice-input\.venv or in %LOCALAPPDATA%\claudeTalk\venv.

Set sh = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")

root = fso.GetParentFolderName(fso.GetParentFolderName(WScript.ScriptFullName))
script = root & "\voice-input\daemon_cli.py"

py = ""
saved = sh.ExpandEnvironmentStrings("%APPDATA%") & "\claudeTalk\venv-path.txt"
If fso.FileExists(saved) Then
    py = Trim(fso.OpenTextFile(saved, 1).ReadAll()) & "\Scripts\pythonw.exe"
End If
If Not fso.FileExists(py) Then py = root & "\voice-input\.venv\Scripts\pythonw.exe"
If Not fso.FileExists(py) Then
    py = sh.ExpandEnvironmentStrings("%LOCALAPPDATA%") & "\claudeTalk\venv\Scripts\pythonw.exe"
End If
If Not fso.FileExists(py) Then
    MsgBox "Dictation is not installed yet. Run scripts\setup-voice.ps1 first.", 48, "claudeTalk"
    WScript.Quit 1
End If

' Marks this run as the standalone app, so it stays open even if a Claude
' Code SessionStart hook later launches its own --auto instance and that
' one later decides no session is left.
appData = sh.ExpandEnvironmentStrings("%APPDATA%") & "\claudeTalk"
If Not fso.FolderExists(appData) Then fso.CreateFolder(appData)
Set flag = fso.CreateTextFile(appData & "\persistent.flag", True)
flag.Close

sh.CurrentDirectory = root & "\voice-input"
sh.Run """" & py & """ """ & script & """", 0, False
