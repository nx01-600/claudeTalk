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
| **Talk mode (TTS)** | `/talk` turns it on and off. Claude answers for a listener: short answers are read aloud, long ones get a spoken summary while the detail stays on screen. |

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
- **Activation**: **Keys** — click and press the new combination; it saves on release, `Esc` cancels. **Silence cutoff** — a slider from 0.5 to 10 s, in quarter seconds. **Mic sensitivity** — a 0-100 slider: how easily sound counts as speech; lower it if voices from your speakers (a call, echo) keep the recording going or get transcribed. **Sound on start** — the chime when recording begins. **Send with Enter** — press Enter right after pasting, so the dictated message is sent without touching the keyboard (off by default).
- **Appearance**: **Theme** — Light / Dark. **Glass** — how much blur and transparency. **Position** — Bottom / Top.
- **Transcription**: **Language** — Spanish / English / Auto.
- **Turn off dictation**: shuts the daemon down completely, asks for confirmation.

Settings live in `%APPDATA%\claudeTalk\dictation.json` and apply instantly.

### Starting and stopping

- It starts on its own with every Claude Code session (`SessionStart` hook) and shuts down on its own when you close the last Claude Code window.
- By hand: the **claudeTalk** Start Menu app (see [Installation](#3-start-menu-app-optional)), the desktop **claudeTalk Dictation** shortcut, `wscript scripts\dictation.vbs`, or `/dictation` inside Claude Code. Launched by hand, it stays running (even through Claude Code sessions opening and closing) until you turn it off.
- Turning it off: gear → **Turn off dictation**, or tray → **Turn off dictation**. Always asks for confirmation.

### Talk mode (Claude's voice)

- `/talk` turns it on and stays on until you run `/talk` again. Saying it in your own words also works ("háblame", "ya no hables"): Claude invokes the skill itself. `/voice` shows the status.
- While it is on, each prompt reminds Claude that you are listening. Claude then:
  - writes short answers once, in plain sentences, and they are read aloud (no double generation);
  - for long or technical answers, speaks a 1-2 sentence summary ("I left the three steps on screen") with the `say` tool and writes the detail. The `say` call stays folded in the transcript: **Ctrl+O** shows what was said;
  - can speak while it works ("let me check the hook") because `say` plays in the background.
- Answers queue up: a new prompt does not cut what Claude is saying, so you can send the next message while still listening. To silence it now, say "cállate" (talk mode stays on).
- Safety net: if Claude writes something long without speaking, the `Stop` hook only says "I left the answer on screen".
- The first time Claude uses `say`, Claude Code asks for permission; pick "don't ask again" (or add `mcp__plugin_claudeTalk_voice__say` to `permissions.allow`).
- **Voice and speed**: open the dictation gear. Next to the dictation settings, a **Claude's voice** panel lets you pick the voice (Salomé, Gonzalo, Dalia, Jorge) and the speed. Each change plays a sample. The choice is global (`%APPDATA%\claudeTalk\dictation.json`). You can also just ask Claude ("cambia a la voz de Salomé", "habla más rápido", "ponle 5 segundos de silencio"): the talk skill edits that file and the dictation app reloads it within a second.
- **"Oye Claude" (hands-free)**: turn on **Start with "Oye Claude"** in the same panel. While talk mode is on, saying "Oye Claude" starts a dictation, as if you had pressed the keys: wait for the chime, then speak. If nothing is said within 6 seconds, it gives up.
  - The text always goes to the **last Claude Code session you had in front**, even if you are in another app or another tab by then. The daemon jumps to that window, finds the tab by its title (Claude Code titles it "✳ topic"; it cycles tabs with Ctrl+Tab), pastes and sends, then puts the tab and your focus back. If that session can't be found, the text stays on the clipboard.
  - It only listens while talk mode is on (`/talk` writes `%APPDATA%\claudeTalk\talk-active.flag`). With talk mode off, the mic is closed.
  - The detector is the Whisper model already loaded for dictation: no extra download. Short sound bursts are transcribed and checked for the phrase; silence costs nothing. On CPU-only machines each burst takes longer.
  - It goes deaf while Claude is speaking, so its own voice can't trigger it. Background music or video may still cause an occasional false start (it cancels itself after 6 seconds).
- The on/off switch is per project, in `.claude/claudetalk.local.md` (`enabled`, `skip_code`).
- Voices come from edge-tts: Microsoft Edge's "Read aloud" service. It's free and needs no key or account, but it isn't an official API, so Microsoft could limit or change it.

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
commands/           /voice /dictation
skills/talk/        /talk: turns talk mode on and off
.mcp.json           `voice` MCP server → say-server.ps1 (the `say` tool)
hooks/hooks.json    UserPromptSubmit → talk-context.ps1   Stop → speak.ps1   SessionStart → voice-daemon-ensure.ps1
scripts/
  talk-common.ps1         shared helpers: state file, speech queue, cutting audio
  say-server.ps1          MCP server with the `say` tool (queues a phrase)
  talk-context.ps1        cuts audio on each prompt, injects the talk mode rules
  speak.ps1               end of turn: picks what to speak; -Worker plays the queue (edge-tts | ffplay)
  voice-toggle.ps1        talk mode on/off/toggle/status
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
