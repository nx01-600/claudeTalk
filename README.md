# claudeTalk

Voice for **Claude Code** on Windows, in both directions:

- **Claude speaks**: reads its responses out loud (Microsoft neural voices via `edge-tts`).
- **You dictate**: press a key chord, talk, and the text appears transcribed in the window you had focused. All local: Whisper runs on your GPU (or CPU), the audio never leaves your machine.

Dictation comes with a floating *liquid glass* overlay (black and white, light or dark) with bars that follow your voice, and a settings panel from the gear icon.

> Status: functional and in daily use. This is stage 2 of a project whose north star is a live, interruptible conversation with Claude, like a call.

---

## What it does

| | |
|---|---|
| **Voice dictation** | A tap of `Ctrl + Shift + Space` (configurable) starts recording. It stops after 2 s of silence, or with another tap. `Esc` cancels. |
| **Smart paste** | The text is pasted into the window that had focus when recording started; if you switched windows, it doesn't paste anywhere else. The text **always** ends up on the clipboard too. |
| **Liquid glass overlay** | Floating pill made of live glass: what is behind it shows through blurred and in color, with edge refraction and a specular rim. Live volume bars and a settings gear. Never steals focus. |
| **Live settings** | Activation keys (captures the chord you press), silence cutoff, mic sensitivity, sound, send with Enter, light/dark theme, glass intensity, position, language (Spanish / English / auto). Turning it off asks for confirmation. |
| **Lifecycle** | With the plugin installed, dictation starts on its own when Claude Code opens and shuts down on its own when the last interactive Claude Code session ends (the `SessionStart` hook registers each session; headless `claude -p` subprocesses spawned by other plugins are ignored). You can also launch it by hand as an app (it sits in the tray). |
| **Claude's voice (TTS)** | `/voice-on` and `/voice-off`. Reads the latest response, skips code blocks, doesn't block the terminal. |

## Requirements

- Windows 10/11.
- [Claude Code](https://claude.com/claude-code).
- Python 3.11 to 3.13 (for dictation).
- NVIDIA GPU with CUDA 12 for transcription in tenths of a second. Without a GPU it works on CPU (several seconds per sentence).
- For Claude's voice (TTS): `edge-tts` (`pip install edge-tts`, needs internet) and `ffmpeg` (`winget install Gyan.FFmpeg`).

## Installation

### 1. Plugin in Claude Code

```
git clone https://github.com/nx01-600/claudeTalk.git
```

Inside Claude Code:

```
/plugin marketplace add C:\path\to\claudeTalk
/plugin install claudeTalk@claudeTalk
```

### 2. Voice dictation (one time only)

```powershell
powershell -ExecutionPolicy Bypass -File C:\path\to\claudeTalk\scripts\setup-voice.ps1 -Shortcut
```

Creates a virtual environment at `%LOCALAPPDATA%\claudeTalk\venv`, installs the dependencies (`faster-whisper`, `PySide6`, `sounddevice`, CUDA runtime) and, with `-Shortcut`, leaves a **claudeTalk Dictation** shortcut on the desktop. The Whisper `large-v3-turbo` model (~1.6 GB) downloads on its own the first time you dictate.

Done. The next Claude Code session already starts with dictation active.

### 3. Start Menu app (optional)

```powershell
powershell -ExecutionPolicy Bypass -File C:\path\to\claudeTalk\scripts\install-app.ps1
```

Adds a **claudeTalk** shortcut to the Start Menu, with the same icon shown in the system tray. Press the Windows key, type `claudeTalk`, hit Enter: dictation starts standalone, with no Claude Code session required, and keeps running until you turn it off from the gear or the tray icon — even if a Claude Code session later opens and closes.

## Usage

### Dictate

1. Focus the window where you want the text (the Claude Code terminal, an editor, whatever).
2. Tap **Ctrl + Shift + Space**. A soft chime plays and the pill appears.
3. Talk. Once you stop for 2 seconds (or tap the chord again) it transcribes and pastes.

If you switched windows in the meantime, it pastes nothing: the text stays on the clipboard for `Ctrl+V`.

### Settings

Click the gear on the pill (or right-click the tray icon → **Settings**).

- **Dictation**
- **Activation**: **Keys** — click and press the new combination; it saves on release, `Esc` cancels. **Silence cutoff** — 1, 2, or 3 s. **Mic sensitivity** — Low / Medium / High: how easily sound counts as speech; lower it if voices from your speakers (a call, echo) keep the recording going or get transcribed. **Sound on start** — the chime when recording begins. **Send with Enter** — press Enter right after pasting, so the dictated message is sent without touching the keyboard (off by default).
- **Appearance**: **Theme** — Light / Dark. **Glass** — how much blur and transparency. **Position** — Bottom / Top.
- **Transcription**: **Language** — Spanish / English / Auto.
- **Turn off dictation**: shuts the daemon down completely, asks for confirmation.

Settings live in `%APPDATA%\claudeTalk\dictation.json` and apply instantly.

### Starting and stopping

- It starts on its own with every Claude Code session (`SessionStart` hook) and shuts down on its own when you close the last Claude Code window.
- By hand: the **claudeTalk** Start Menu app (see [Installation](#3-start-menu-app-optional)), the desktop **claudeTalk Dictation** shortcut, `wscript scripts\dictation.vbs`, or `/dictation` inside Claude Code. Launched by hand, it stays running (even through Claude Code sessions opening and closing) until you turn it off.
- Turning it off: gear → **Turn off dictation**, or tray → **Turn off dictation**. Always asks for confirmation.

### Claude's voice (TTS)

- `/voice-on` turns it on, `/voice-off` turns it off and cuts the audio, `/voice` shows the status.
- Per-project configuration in `.claude/claudetalk.local.md`:

```yaml
---
enabled: true
voice: es-CO-GonzaloNeural  # any voice from `edge-tts --list-voices`
rate: "+0%"
skip_code: true
---
```

## How it works

```
key chord ──► recording (16 kHz, silence cutoff) ──► faster-whisper (GPU)
     │                       │                                 │
     │                  overlay.py                             ▼
     │            pill + level bars                 clipboard + simulated Ctrl+V
     │                                               (only if focus didn't change)
     └── hotkey.py: GetAsyncKeyState polling every 15 ms, no keyboard hooks
```

Decisions worth knowing (all explained in the docstrings):

- **Hotkey without hooks.** `RegisterHotKey` doesn't distinguish left Alt from right Alt and doesn't accept modifier-only chords; low-level hooks (`keyboard`, `pynput`) bring auto-repeat storms. Polling `GetAsyncKeyState` solves both problems at negligible cost.
- **Paste, don't type.** Typing character by character with `SendInput` loses accented characters depending on the app and can land in the wrong window partway through. Pasting via the clipboard is atomic and preserves Unicode.
- **Real, live glass.** Windows 11's native backdrops (Mica/Acrylic) return a solid panel for windows whose content Qt paints by hand, so the overlay does it itself: the window is excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`), grabs what is behind it 25 times a second, blurs it with color and boosted saturation, tints it, and adds edge lensing and a specular rim. Side effect: the overlay is invisible in screenshots and screen sharing.
- **Never steals focus.** The overlay and panel use `WS_EX_NOACTIVATE`; if they were to activate, the paste would end up going to the overlay.
- **Resident model.** Whisper preloads on startup and unloads after 30 minutes of no use to free VRAM.

## Structure

```
.claude-plugin/     plugin and local marketplace manifest
assets/claudetalk.ico  Start Menu / tray icon, generated by scripts/make-icon.py
commands/           /voice-on /voice-off /voice /dictation
hooks/hooks.json    Stop → speak.ps1 (TTS)   SessionStart → voice-daemon-ensure.ps1
scripts/
  speak.ps1               TTS: transcript → edge-tts → ffplay
  voice-toggle.ps1        voice mode status
  setup-voice.ps1         installs dictation (venv + dependencies + shortcut)
  voice-daemon-ensure.ps1 launches the daemon in --auto mode if not already running
  dictation.vbs           manual launcher without a console window (the "app")
  install-app.ps1         adds the claudeTalk Start Menu shortcut
  make-icon.py            renders voice-input/icon.py into assets/claudetalk.ico
voice-input/
  daemon_cli.py    orchestration, tray, --auto mode, single instance
  icon.py          tray/shortcut artwork, shared by daemon_cli.py and make-icon.py
  hotkey.py        chord via polling, key capture
  audio.py         recording with noise calibration and silence cutoff
  stt.py           resident faster-whisper (GPU, CPU fallback)
  inject.py        clipboard + Ctrl+V, focus guard, diagnostics
  overlay.py       pill, settings panel, glass, animations
  config.py        persistent settings (%APPDATA%\claudeTalk\dictation.json)
  sounds.py        synthesized start chime
  test_*.py        manual diagnostics
```

## Diagnostics

- Dictation log (when running without a console): `%TEMP%\claudetalk-dictation.log`. Running it by hand in a terminal (`python voice-input\daemon_cli.py`) shows the same live, including a `[diag]` line for each paste with the target window, whether it's running elevated, and how many events `SendInput` accepted.
- TTS log: `%TEMP%\claudetalk.log`.
- "Doesn't paste into that app but does into others": if the app runs as administrator and the daemon doesn't, Windows blocks the synthetic `Ctrl+V` (UIPI). Launch the daemon with the same privilege level.
- Two instances can't coexist: the second one warns and exits.

## Development

```powershell
cd voice-input
python -m venv .venv
.\.venv\Scripts\pip install -r requirements.txt
.\.venv\Scripts\python daemon_cli.py        # with console and live logs
.\.venv\Scripts\python test_hotkey.py       # see which keys it detects
.\.venv\Scripts\python test_inject.py       # paste into a test Notepad
```

If `voice-input\.venv` exists, both the hook and the launcher prefer it over the `%LOCALAPPDATA%` venv.

## License

MIT. See [LICENSE](LICENSE).
