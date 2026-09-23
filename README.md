# claudeTalk

Voice for **Claude Code** on Windows, in both directions:

- **Claude speaks**: reads its responses out loud (Microsoft neural voices via `edge-tts`).
- **You dictate**: press a key chord, talk, and the text appears transcribed in the window you had focused. All local: Whisper runs on your GPU (or CPU), the audio never leaves your machine.

Dictation comes with a floating *liquid glass* overlay (dark glass, white ink) with bars that follow your voice, and a settings panel from the gear icon.

> Status: functional and in daily use. This is stage 2 of a project whose north star is a live, interruptible conversation with Claude, like a call.

---

## What it does

| | |
|---|---|
| **Voice dictation** | A tap of `Ctrl + Shift + Space` (configurable) starts recording. It stops after 2 s of silence, or with another tap. `Esc` cancels. |
| **Smart paste** | The text is pasted into the window that had focus when recording started; if you switched windows, it doesn't paste anywhere else. If the focus was on the desktop or the taskbar (nothing there takes text), it goes to your last Claude Code session instead, like "Oye Claude". The text **always** ends up on the clipboard too. |
| **Liquid glass overlay** | Floating pill made of live glass: what is behind it shows through blurred and in color, with edge refraction and a specular rim. Live volume bars and a settings gear. Never steals focus. |
| **Send confirmation** | The pill stays up while it transcribes, then compacts into a glass circle: green check when the text was pasted, amber clipboard icon when it was only left on the clipboard (e.g. you switched windows). |
| **"Oye Claude" (hands-free)** | With talk mode on, say "Oye Claude" and dictate. The text goes to your last Claude Code session (right terminal tab included) from any app, and is sent. The terminal stays invisible while it happens, so nothing pops up over what you are doing. |
| **Spoken vs typed** | Dictations sent to Claude start with 🎙️, so Claude knows they were spoken and reads past transcription slips. With **Speak only when I talk** on, a typed message gets a silent text answer while talk mode stays on. |
| **One voice per session** | Talk mode is per Claude Code session. When several sessions talk at once, each one gets its own voice automatically (6 voices: Colombian, Mexican, Argentine and US Spanish) and introduces it when talk mode starts, so you can tell by ear which session is answering. They take turns, never talk over each other. |
| **No echo** | If Claude is talking when you start dictating, its voice dips in volume until you finish, so the mic doesn't write Claude's words into your message. |
| **Live settings** | Activation keys (captures the chord you press), silence cutoff, mic sensitivity, sound, send with Enter, glass intensity, position, language (Spanish / English / auto). Turning it off asks for confirmation. |
| **Lifecycle** | With the plugin installed, dictation starts on its own when Claude Code opens and shuts down on its own when the last interactive Claude Code session ends (the `SessionStart` hook registers each session; headless `claude -p` subprocesses spawned by other plugins are ignored). You can also launch it by hand as an app (it sits in the tray). |
| **Talk mode (TTS)** | `/talk` turns it on and off. Claude answers for a listener: short answers are read aloud, long ones get a spoken summary while the detail stays on screen. Answers queue up instead of cutting each other. Claude can change any setting when asked in plain words ("habla más rápido", "cambia a la voz de Elena"). |

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
3. Talk. Once you stop for 2 seconds (or tap the chord again) it transcribes and pastes. The pill stays up while it transcribes (the bars ripple), then compacts into a glass circle (the gear fades away) with a green check inside when the text was pasted, or an amber clipboard icon when it was only left on the clipboard, and fades out.

If you switched windows in the meantime, it pastes nothing: the text stays on the clipboard for `Ctrl+V`.

### Settings

Click the gear on the pill (or right-click the tray icon → **Settings**).

- **Dictation**
- **Activation**: **Keys** — click and press the new combination; it saves on release, `Esc` cancels. **Silence cutoff** — a slider from 0.5 to 10 s, in quarter seconds. **Mic sensitivity** — a 0-100 slider: how easily sound counts as speech; lower it if voices from your speakers (a call, echo) keep the recording going or get transcribed. **Sound on start** — the chime when recording begins. **Send with Enter** — press Enter right after pasting, so the dictated message is sent without touching the keyboard (off by default).
- **Appearance**: the overlay is always dark glass with white text (a light theme existed but read poorly on most desktops). **Glass** — how much blur and transparency. **Position** — Bottom / Top. **Show in screen share** — off by default, the pill and panels are invisible to screen sharing, screenshots and recordings. On, they show up there too; the glass then uses one snapshot of the background taken as the window appears instead of refreshing live (it would otherwise capture itself). It never takes focus either way.
- **Transcription**: **Language** — Spanish / English / Auto.
- **Turn off dictation**: shuts the daemon down completely, asks for confirmation.

Settings live in `%APPDATA%\claudeTalk\dictation.json` and apply instantly. Claude can change any of them too: ask in your own words ("que no se mande solo con Enter", "que se vea cuando comparto pantalla", "ponle 5 segundos de silencio", "cambia el atajo a control alt espacio"). The talk skill runs `scripts\voice-toggle.ps1 set <setting> <value>` and the dictation app reloads the file within a second; `voice-toggle.ps1 settings` lists every value and option.

### Starting and stopping

- It starts on its own with every Claude Code session (`SessionStart` hook) and shuts down on its own when you close the last Claude Code window.
- By hand: the **claudeTalk** Start Menu app (see [Installation](#3-start-menu-app-optional)), the desktop **claudeTalk Dictation** shortcut, `wscript scripts\dictation.vbs`, or `/dictation` inside Claude Code. Launched by hand, it stays running (even through Claude Code sessions opening and closing) until you turn it off.
- Turning it off: gear → **Turn off dictation**, or tray → **Turn off dictation**. Always asks for confirmation.

### Talk mode (Claude's voice)

- `/talk` turns it on **for that session** and stays on until you run `/talk` again there. Other open sessions are not affected, and a resumed session (`--continue`, `/clear`) keeps its state and voice. Saying it in your own words also works ("háblame", "ya no hables"): Claude invokes the skill itself. `/voice` shows the status.
- While it is on, each prompt reminds Claude that you are listening. Claude then:
  - writes short answers once, in plain sentences, and they are read aloud (no double generation);
  - for long or technical answers, speaks a 1-2 sentence summary ("I left the three steps on screen") with the `say` tool and writes the detail. The `say` call stays folded in the transcript: **Ctrl+O** shows what was said;
  - can speak while it works ("let me check the hook") because `say` plays in the background.
- Answers queue up: a new prompt does not cut what Claude is saying, so you can send the next message while still listening. To silence it now, say "cállate" (talk mode stays on).
- Safety net: if Claude writes something long without speaking, the `Stop` hook only says "I left the answer on screen".
- The first time Claude uses `say`, Claude Code asks for permission; pick "don't ask again" (or add `mcp__plugin_claudeTalk_voice__say` to `permissions.allow`).
- **Voice and speed**: open the dictation gear. Next to the dictation settings, a **Claude's voice** panel lets you pick the voice (Salomé, Gonzalo, Dalia, Jorge, Elena, Alonso) and the speed. Each change plays a sample. The choice is global (`%APPDATA%\claudeTalk\dictation.json`).
- **Parallel sessions**: the first session that turns talk mode on speaks with the gear voice. A session that turns it on while others are talking gets the first voice they aren't using, and greets you with it ("Esta va a ser mi voz en esta sesión"). Asking a session for another voice ("cambia a la voz de Elena") changes only that session. Per-session state lives in `%APPDATA%\claudeTalk\sessions\`; `live.json` maps each running `claude.exe` to its session so the `say` tool knows whose voice to use. You can also just ask Claude ("cambia a la voz de Salomé", "habla más rápido", "ponle 5 segundos de silencio"): the talk skill edits that file and the dictation app reloads it within a second.
- **"Oye Claude" (hands-free)**: turn on **Start with "Oye Claude"** in the same panel. While talk mode is on, saying "Oye Claude" starts a dictation, as if you had pressed the keys: wait for the chime, then speak. If nothing is said within 6 seconds, it gives up.
  - The text always goes to the **last Claude Code session you had in front**, even if you are in another app or another tab by then. The daemon jumps to that window, finds the tab by its title (Claude Code titles it "✳ topic"; it cycles tabs with Ctrl+Tab), pastes and sends, then puts the tab and your focus back. The terminal is made fully transparent while this happens, so it never pops up over what you are doing; the pill's green check tells you it arrived. If that session can't be found, the text stays on the clipboard.
  - It only listens while talk mode is on in at least one session (`/talk` writes `%APPDATA%\claudeTalk\talk-active.flag`). With talk mode off, the mic is closed.
  - The detector is the Whisper model already loaded for dictation: no extra download. Short sound bursts are transcribed and checked for the phrase; silence costs nothing. On CPU-only machines each burst takes longer.
  - It goes deaf while Claude is speaking, so its own voice can't trigger it. Background music or video may still cause an occasional false start (it cancels itself after 6 seconds).
- **Spoken messages are marked**: every dictation pasted into Claude Code starts with 🎙️, so Claude knows it was spoken (and reads past transcription slips). Turn on **Speak only when I talk** in the same panel and talk mode answers out loud only those: a message you type gets a silent, text-only answer, without turning talk mode off.
- **Claude lowers its voice while you dictate**: if Claude is still talking when a recording starts, its volume dips (the ffplay player's volume in the Windows mixer) until the recording ends, and any phrase that starts meanwhile is read a bit slower. That keeps the mic from writing Claude's words into your message.
- The on/off switch is per project, in `.claude/claudetalk.local.md` (`enabled`, `skip_code`).
- Voices come from edge-tts: Microsoft Edge's "Read aloud" service. It's free and needs no key or account, but it isn't an official API, so Microsoft could limit or change it.

## Resource usage

Measured on a laptop with an RTX 5070 Ti Laptop GPU (12 GB) and a 24-thread CPU, with the daemon in `--auto` mode. Your numbers will vary with the hardware, but the proportions hold.

### Dictation daemon (the only resident piece)

| Resource | While idle | While dictating |
|---|---|---|
| **VRAM** | about **2.1 GB** (measured: 1.5 GB used by the system without the daemon, 3.7 GB with it) | the same, plus a short spike while Whisper decodes |
| **RAM** | about **275 MB** working set. Windows reports about 3.1 GB *committed* (reserved address space for the CUDA and cuBLAS libraries); that is not memory in use. | about the same |
| **CPU** | about **4-5 % of one core** (0.2 % of the whole CPU), with or without talk mode: the overlay's timers, the hotkey hook, watching which window is in front, and reading the mic for the wake word | Whisper runs on the GPU. 30 s of speech took 0.5 s to transcribe; 49 s took 3.2 s. |

Why it doesn't cost that all the time:

- **One model for everything.** Whisper `large-v3-turbo` (float16, CUDA) stays loaded in VRAM so a dictation starts without waiting. "Oye Claude" reuses that same model; no second wake word model is loaded.
- **It unloads itself.** After **30 minutes** without use the model is released and the 2.1 GB of VRAM go back to the system. The next dictation reloads it, which takes a few seconds.
- **Silence is free.** The wake word listener only reads the mic. A cheap energy check cuts out short sound bursts (0.3 to 2.5 s), and only those reach Whisper. Silence and steady noise never touch the GPU.
- **The wake word listens only while talk mode is on** in at least one session and the "Oye Claude" toggle is on. Otherwise the mic stays closed between dictations.
- **The daemon closes when you do.** In `--auto` mode it shuts down when the last interactive Claude Code session ends, and all its memory goes back to the system. The gear's "Turn off dictation" closes it right away.
- **No GPU?** It falls back to CPU (int8). That uses more CPU and is several times slower per dictation, but it needs no VRAM.

### Claude's voice (talk mode)

- Each spoken phrase starts `edge-tts` (about 5 MB) and `ffplay` (about 25 MB). Together they peak around **30 MB of RAM** and exit when the phrase ends. Nothing stays resident.
- The speech itself is synthesized by Microsoft's online service, so it uses a little network bandwidth (a compressed MP3 stream) and no GPU.
- Hooks run a short PowerShell process on each prompt and at the end of each answer (about 0.3 to 1 s of CPU), then exit.

### Tokens added to your Claude conversation

claudeTalk adds text to what Claude reads only while talk mode is on in that session. These are estimates, using about 4 characters per token:

| When | What gets added | About |
|---|---|---|
| Talk mode **off** | nothing, not even the hook's reminder | **0 tokens** |
| Every prompt, talk mode **on** | the reminder rules for answering a listener | **~300 tokens** per prompt |
| Plus, if the prompt was dictated (🎙️) | the note about transcription slips | **~75 tokens** more |
| Typed prompt with "Speak only when I talk" on | a single line saying to answer in text only | **~70 tokens** |
| Always (plugin installed) | the `talk` skill's one-line description in the skill list, and the `say` tool's short schema | **~200 + ~100 tokens**, fixed per conversation |
| When the skill runs (`/talk`, changing a setting) | the skill's instructions | **~1,100 tokens**, only that turn |

Claude's answers also get a little longer in talk mode: each `say` call is one or two sentences (about 30 to 60 tokens). In exchange, long answers are summarized out loud instead of being read in full.

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
