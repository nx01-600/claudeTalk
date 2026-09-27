<p align="center">
  <img src="assets/logo.png" width="112" alt="claudeTalk logo">
</p>

<h1 align="center">claudeTalk</h1>

<p align="center">
  <b>Talk to Claude Code, and hear it talk back.</b><br>
  Local Whisper dictation, a hands-free wake phrase and neural voices, in two small native programs for Windows.
</p>

<p align="center">
  <a href="https://github.com/nx01-600/claudeTalk/releases/latest"><img src="https://img.shields.io/github/v/release/nx01-600/claudeTalk?style=flat-square&color=2f6f7a" alt="Release"></a>
  <a href="https://github.com/nx01-600/claudeTalk/actions/workflows/build.yml"><img src="https://img.shields.io/github/actions/workflow/status/nx01-600/claudeTalk/build.yml?style=flat-square&label=build" alt="Build"></a>
  <a href="https://github.com/nx01-600/claudeTalk/releases"><img src="https://img.shields.io/github/downloads/nx01-600/claudeTalk/total?style=flat-square&color=7a4a3a" alt="Downloads"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/nx01-600/claudeTalk?style=flat-square" alt="MIT license"></a>
  <br>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4?style=flat-square&logo=windows&logoColor=white" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/Rust-native-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/whisper.cpp-Vulkan-A41E11?style=flat-square&logo=vulkan&logoColor=white" alt="whisper.cpp on Vulkan">
  <img src="https://img.shields.io/badge/Claude%20Code-plugin-D97757?style=flat-square&logo=claude&logoColor=white" alt="Claude Code plugin">
</p>

---

Voice for **Claude Code** on Windows, in both directions:

- **Claude speaks**: reads its responses out loud (Microsoft Edge neural voices, streamed by a small native player).
- **You dictate**: press a key chord, talk, and the text appears transcribed in the window you had focused. All local: Whisper runs on your GPU (any GPU with Vulkan: NVIDIA, AMD or Intel) or the CPU, and the audio never leaves your machine.

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
| **"Oye Claude" (hands-free)** | With talk mode on, say "Oye Claude" (or **your own wake phrase**, set in the gear) and dictate. Only dictations you start by voice work this way; the keyboard shortcut always writes into the window you had in front. The text goes to your last Claude Code session (right terminal tab included) from any app, games in borderless windows included, and is sent. It is typed straight into that session's console, so it works with any terminal (Warp, Windows Terminal, PowerShell) and your focus never moves. |
| **Spoken vs typed** | Dictations sent to Claude start with 🎙️, so Claude knows they were spoken and reads past transcription slips. With **Speak only when I talk** on, a typed message gets a silent text answer while talk mode stays on. |
| **Any language** | One setting switches the voices, the dictation, the wake phrase, the spoken lines and the gear panel. Spanish and English are built in; for any other language Claude writes the language pack itself the first time you ask ("put claudeTalk in French"). See [docs/LANGUAGES.md](docs/LANGUAGES.md). |
| **One voice per session** | Talk mode is per Claude Code session. When several sessions talk at once, each one gets its own voice automatically (6 per built-in language: Colombian, Mexican, Argentine and US Spanish; US and British English) and introduces it when talk mode starts, so you can tell by ear which session is answering. They take turns, never talk over each other. |
| **No echo** | If Claude is talking when you start dictating, its voice dips in volume until you finish, so the mic doesn't write Claude's words into your message. |
| **Live settings** | Activation keys (captures the chord you press), silence cutoff, mic sensitivity, sound, send with Enter, glass intensity, position, remember dragged spot, language (plus automatic language detection). Turning it off asks for confirmation. |
| **Lifecycle** | With the plugin installed, dictation starts on its own when Claude Code opens and shuts down on its own when the last interactive Claude Code session ends (the `SessionStart` hook registers each session; headless `claude -p` subprocesses spawned by other plugins are ignored). Before closing, it double-checks the running `claude.exe` processes, so a session whose registration got lost doesn't shut dictation down, and it never closes in the middle of a dictation. You can also launch it by hand as an app (it sits in the tray). |
| **Talk mode (TTS)** | `/talk` turns it on and off. Claude answers for a listener: short answers are read aloud, long ones get a spoken summary while the detail stays on screen. Answers queue up instead of cutting each other. Claude can change any setting when asked in plain words ("habla más rápido", "cambia a la voz de Elena"). |

## Requirements

- Windows 10/11.
- [Claude Code](https://claude.com/claude-code).
- Nothing else to install: no Python, no ffmpeg, no CUDA. The plugin ships `bin\claudetalk.exe` (about 3 MB: hooks, the `say` tool, Claude's voice). The dictation app, `claudetalk-dictation.exe` (~60 MB, because it carries whisper.cpp's GPU shaders), is downloaded once per version from this repo's GitHub release, so the plugin itself stays small.
- For fast transcription, a GPU with Vulkan drivers (any recent NVIDIA, AMD or Intel GPU). Without one it runs on the CPU, several times slower.
- Internet for Claude's voice (Microsoft's online voice service) and, once, to download the dictation app (~60 MB) and the voice model (~0.9 GB).

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

Done. The next Claude Code session starts dictation on its own. The first time, it downloads the dictation app for this version to `%LOCALAPPDATA%\claudeTalk\bin\<version>\` (`claudetalk.exe fetch-dictation` does it by hand), then the Whisper `large-v3-turbo` model (q8_0, ~0.9 GB) and the Silero VAD model (~1 MB) to `%LOCALAPPDATA%\claudeTalk\models`; the tray icon's tooltip shows the progress.

### 2. Start Menu app (optional)

```powershell
powershell -ExecutionPolicy Bypass -File C:\path\to\claudeTalk\scripts\install-app.ps1 [-Desktop]
```

Adds a **claudeTalk** shortcut to the Start Menu, with the same icon shown in the system tray. Press the Windows key, type `claudeTalk`, hit Enter: dictation starts standalone, with no Claude Code session required, and keeps running until you turn it off from the gear or the tray icon — even if a Claude Code session later opens and closes.

### Upgrading from v0.5 or earlier

v0.6 and v0.7 replaced the PowerShell scripts and the Python dictation app with two native programs. The old pieces are not needed any more, but they don't delete themselves, and they are big: the Python environment is about 3 GB, and Claude Code's plugin cache may keep a copy of it for every older version (37 GB on the author's machine).

```powershell
"<plugin>\bin\claudetalk.exe" cleanup          # lists what would be removed, with sizes
"<plugin>\bin\claudetalk.exe" cleanup --yes    # removes it
```

It only removes claudeTalk's own leftovers:
- the Python environment (`voice-input\.venv`, `%LOCALAPPDATA%\claudeTalk\venv`, `venv-path.txt`);
- old scripts;
- faster-whisper's model in the Hugging Face cache;
- model files this version doesn't load, and dictation apps downloaded for other versions;
- older plugin copies in `~\.claude\plugins\cache\claudeTalk`.

It also repoints Start Menu and desktop shortcuts that still launch `dictation.vbs`. It doesn't uninstall ffmpeg or edge-tts, because other tools may use them; it only mentions them.

You don't have to remember this: while leftovers exist, the `SessionStart` hook tells Claude, and Claude offers the cleanup once. You can also just ask ("clean up what the old claudeTalk left behind").

> [!IMPORTANT]
> **Claude, if you are installing or upgrading claudeTalk on your own:** check for leftovers of older versions with `bin\claudetalk.exe cleanup` (it only lists them), then **ask the person who owns this computer** whether to remove them, telling them what would go and how many GB. Run `cleanup --yes` only after they say yes. Never delete them unasked, even when you did the install yourself, and don't touch ffmpeg or edge-tts: other tools may use them.

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
- **Appearance**: the overlay is always dark glass with white text (a light theme existed but read poorly on most desktops). **Glass** — how much blur and transparency. **Position** — Bottom / Top. The pill can also be dragged anywhere with the mouse (grab it anywhere except the gear). **Remember dragged spot** — off by default, the pill resets to Bottom/Top every time it appears; on, it comes back where you last dragged it. Picking Bottom or Top again forgets the dragged spot. **Show in screen share** — off by default, the pill and panels are invisible to screen sharing, screenshots and recordings. On, they show up there too; the glass then uses one snapshot of the background taken as the window appears instead of refreshing live (it would otherwise capture itself). It never takes focus either way.
- **Transcription**: **Language** — claudeTalk's language (Español, English and any language pack installed): it also switches Claude's voices, the wake phrase and this panel. **Detect language automatically** — Whisper figures out what you speak on each dictation, for people who mix languages. See [docs/LANGUAGES.md](docs/LANGUAGES.md).
- **Turn off dictation**: shuts the daemon down completely, asks for confirmation.

Settings live in `%APPDATA%\claudeTalk\dictation.json` and apply instantly. Claude can change any of them too: ask in your own words ("que no se mande solo con Enter", "que se vea cuando comparto pantalla", "ponle 5 segundos de silencio", "cambia el atajo a control alt espacio"). The talk skill runs `bin\claudetalk.exe toggle set <setting> <value>` and the dictation app reloads the file within a second; `claudetalk.exe toggle settings` lists every value and option.

### Starting and stopping

- It starts on its own with every Claude Code session (`SessionStart` hook) and shuts down on its own when you close the last Claude Code window.
- By hand: the **claudeTalk** Start Menu app (see [Installation](#2-start-menu-app-optional)), the desktop **claudeTalk Dictation** shortcut (`install-app.ps1 -Desktop`), or `/dictation` inside Claude Code. Launched by hand, it stays running (even through Claude Code sessions opening and closing) until you turn it off.
- Turning it off: gear → **Turn off dictation**, or tray → **Turn off dictation**. Always asks for confirmation.

### Talk mode (Claude's voice)

- `/talk` turns it on **for that session** and stays on until you run `/talk` again there. Other open sessions are not affected, and a resumed session (`--continue`, `/clear`) keeps its state and voice. Saying it in your own words also works ("háblame", "ya no hables"): Claude invokes the skill itself. `/voice` shows the status.
- While it is on, the prompts remind Claude that you are listening: the full rules on the first prompt and every 8th one (and again after `/clear` or a compaction), a one-line reminder in between. Claude then:
  - writes short answers once, in plain sentences, and they are read aloud (no double generation);
  - for long or technical answers, speaks a 1-2 sentence summary ("I left the three steps on screen") with the `say` tool and writes the detail. The `say` call stays folded in the transcript: **Ctrl+O** shows what was said;
  - can speak while it works ("let me check the hook") because `say` plays in the background.
- Answers queue up: a new prompt does not cut what Claude is saying, so you can send the next message while still listening. To silence it now, say "cállate" (talk mode stays on).
- Safety net: if Claude writes something long without speaking, the `Stop` hook only says "I left the answer on screen".
- The first time Claude uses `say`, Claude Code asks for permission; pick "don't ask again" (or add `mcp__plugin_claudeTalk_voice__say` to `permissions.allow`).
- **Voice, speed and volume**: open the dictation gear. Next to the dictation settings, a **Claude's voice** panel lets you pick the voice (the current language's: Salomé, Gonzalo, Dalia, Jorge, Elena, Alonso in Spanish; Andrew, Ava, Brian, Emma, Ryan, Sonia in English), the speed and the volume (0-100 slider). Each change plays a sample. The choice is global (`%APPDATA%\claudeTalk\dictation.json`).
- **Parallel sessions**: the first session that turns talk mode on speaks with the gear voice. A session that turns it on while others are talking gets the first voice they aren't using, and greets you with it ("Esta va a ser mi voz en esta sesión"). Asking a session for another voice ("cambia a la voz de Elena") changes only that session. Per-session state lives in `%APPDATA%\claudeTalk\sessions\`; `live.json` maps each running `claude.exe` to its session so the `say` tool knows whose voice to use. You can also just ask Claude ("cambia a la voz de Salomé", "habla más rápido", "ponle 5 segundos de silencio"): the talk skill edits that file and the dictation app reloads it within a second.
- **"Oye Claude" / "Hey Claude" (hands-free)**: turn on **Start with "Oye Claude"** in the same panel (the default phrase follows the language: "Oye Claude" in Spanish, "Hey Claude" in English). While talk mode is on, saying "Oye Claude" starts a dictation, as if you had pressed the keys: wait for the chime, then speak. If nothing is said within 6 seconds, it gives up.
  - **Your own wake phrase**: click the **Wake phrase** field below that switch and type any words ("Hola Jarvis", "Oye compu"...); `Enter` or clicking away saves it, `Esc` cancels. From Claude Code: `/talk set phrase "Hola Jarvis"`. "Oye Claude" and "Hey Claude" keep their own tuned matching, which also accepts the many ways Whisper spells "Claude". A custom phrase counts when a short burst of speech starts or ends with words close enough to it (75 % similar), so a slightly clipped transcription still wakes it. Pick 2 or 3 words that don't come up in normal talk.
  - The text always goes to the **last Claude Code session you had in front**, even if you are in another app or another tab by then, including a game in a borderless or fullscreen window. The daemon types the text straight into that session's console (each `claude.exe` owns one, whatever the terminal: Warp, Windows Terminal, plain PowerShell...), so your focus never moves and nothing pops up. It remembers the session by its process, not only its title, so it still finds it after Claude Code retitles it. Only if that fails does it fall back to jumping to the window: it finds the tab by its title (Claude Code titles it "✳ topic"; it cycles tabs with Ctrl+Tab), pastes and sends, then puts the tab and your focus back, with the terminal fully transparent meanwhile. The pill's green check tells you it arrived. If no session can be found, the text stays on the clipboard.
  - It only listens while talk mode is on in at least one session (`/talk` writes `%APPDATA%\claudeTalk\talk-active.flag`). With talk mode off, the mic is closed.
  - The detector is the Whisper model already loaded for dictation: no extra download. Short sound bursts are transcribed and checked for the phrase; silence costs nothing. On CPU-only machines each burst takes longer.
  - It goes deaf while Claude is speaking, so its own voice can't trigger it. Background music or video may still cause an occasional false start (it cancels itself after 6 seconds).
- **Spoken messages are marked**: every dictation pasted into Claude Code starts with 🎙️, so Claude knows it was spoken (and reads past transcription slips). Turn on **Speak only when I talk** in the same panel and talk mode answers out loud only those: a message you type gets a silent, text-only answer, without turning talk mode off.
- **Claude lowers its voice while you dictate**: if Claude is still talking when a recording starts, its volume dips (the player's volume in the Windows mixer) until the recording ends, and any phrase that starts meanwhile is read a bit slower. That keeps the mic from writing Claude's words into your message.
- Per project, `.claude/claudetalk.local.md` can set `skip_code: false` so code blocks are read too.
- Voices come from Microsoft Edge's "Read aloud" service (the same protocol as the `edge-tts` project). It's free and needs no key or account, but it isn't an official API, so Microsoft could limit or change it. If it starts rejecting requests, setting `"edge_version"` in `dictation.json` to a current Edge version usually fixes it without an update. The fixed phrases of the current language ("I left the answer on screen", the gear samples) are cached in `%LOCALAPPDATA%\claudeTalk\tts-cache` and play instantly.

## Resource usage

Measured on a laptop with an RTX 5070 Ti Laptop GPU (12 GB) and a 24-thread CPU, with the daemon in `--auto` mode. Your numbers will vary with the hardware, but the proportions hold.

### Dictation daemon (the only resident piece)

| Resource | Idle, model not loaded | Idle, model loaded | While dictating |
|---|---|---|---|
| **VRAM** | 0 | about **0.85 GB** | the same, plus a short spike while Whisper decodes |
| **RAM** | about **13 MB** working set | about **100 MB** | about the same |
| **CPU** | about **0.4 % of one core**: watching which window is in front, the config file and the Claude sessions | the same | Whisper runs on the GPU: about 0.25 s for a 4-second sentence |

The Python daemon of v0.5 used about 275 MB of RAM, 2.1 GB of VRAM and 4-5 % of a core at rest, plus a 2.9 GB virtual environment on disk.

Why it doesn't cost that all the time:

- **One model for everything.** Whisper `large-v3-turbo` (q8_0 quantization, which transcribed our Spanish test set with fewer errors than the float16 model v0.5 used: see `native/bench/RESULTS.md`) stays loaded so a dictation starts without waiting. "Oye Claude" reuses that same model.
- **It unloads itself.** After **30 minutes** without use the model is released and its VRAM goes back to the system. The next dictation reloads it, which takes a couple of seconds.
- **Silence is free.** The wake word listener only reads the mic. A cheap energy check cuts out short sound bursts (0.3 to 2.5 s), a small voice detector (Silero VAD, on the CPU) drops the ones that aren't a voice, and only speech reaches Whisper.
- **The wake word listens only while talk mode is on** in at least one session and the "Oye Claude" toggle is on (whatever the wake phrase is). Otherwise the mic stays closed between dictations.
- **No polling.** The chord is detected from the keyboard's raw input events, so nothing checks the keys 60 times a second; the glass is only re-blurred when what's behind it changed, and the panels only repaint while something in them moves.
- **The daemon closes when you do.** In `--auto` mode it shuts down when the last interactive Claude Code session ends. The gear's or the tray's "Turn off dictation" closes it right away.
- **No GPU?** It runs Whisper on the CPU: several times slower per dictation, but no VRAM.

### Claude's voice (talk mode)

- One small native player (`claudetalk.exe speaker`, about **17 MB of RAM**) plays the queue. Audio starts with the first chunk of the stream, and the player stays up for two minutes after the last phrase so the next one starts at once; then it exits.
- The speech itself is synthesized by Microsoft's online service, so it uses a little network bandwidth (a compressed MP3 stream) and no GPU.
- Hooks run `claudetalk.exe` on each prompt and at the end of each answer: about **15-20 ms**, then it exits (the PowerShell hooks of v0.5 took 0.4-1.5 s).
- The `say` MCP server is the same executable, a few MB per open session (v0.5 kept a ~70 MB PowerShell per session).

### Tokens added to your Claude conversation

claudeTalk adds text to what Claude reads only while talk mode is on in that session. These are estimates, using about 4 characters per token:

| When | What gets added | About |
|---|---|---|
| Talk mode **off** | nothing, not even the hook's reminder | **0 tokens** |
| First prompt with talk mode **on**, then every 8th (and after `/clear` or a compaction) | the full rules for answering a listener | **~300 tokens** |
| The other prompts, talk mode **on** | a one-line reminder of those rules | **~40 tokens** |
| Plus, if the prompt was dictated (🎙️) | the note about transcription slips | **~75 tokens** with the full rules, **~25** with the reminder |
| Typed prompt with "Speak only when I talk" on | a single line saying to answer in text only | **~70 tokens** |
| Always (plugin installed) | the `talk` skill's one-line description in the skill list, and the `say` tool's short schema | **~200 + ~100 tokens**, fixed per conversation |
| When the skill runs (`/talk`, changing a setting) | the skill's instructions | **~1,100 tokens**, only that turn |

Claude's answers also get a little longer in talk mode: each `say` call is one or two sentences (about 30 to 60 tokens). In exchange, long answers are summarized out loud instead of being read in full.

## How it works

```
key chord (raw input) ──► recording (16 kHz, silence cutoff, noise gate) ──► Whisper turbo q8_0 (Vulkan)
        │                        │                                                   │
        │                  glass pill + level bars                                   ▼
        │                                                       clipboard + simulated Ctrl+V
        │                                                       (only if focus didn't change)
"Oye Claude" ──► VAD ──► Whisper ──► same recording ──► typed into the last Claude session's console
```

Decisions worth knowing (explained in the source comments):

- **Chord without hooks or polling.** `RegisterHotKey` can't tell left Alt from right Alt and refuses modifier-only chords; low-level hooks bring auto-repeat storms. Raw keyboard input wakes the daemon only when a key changes, and the chord fires once when all its keys are down.
- **Paste, don't type.** Typing character by character with `SendInput` loses accented characters depending on the app and can land in the wrong window partway through. Pasting via the clipboard is atomic and preserves Unicode.
- **Real, live glass.** Windows 11's native backdrops return a flat panel for hand-painted windows, so the overlay does it itself: the window is excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`), grabs what is behind it, blurs it with color and boosted saturation, tints it, and adds edge lensing and a specular rim. Side effect: the overlay is invisible in screenshots and screen sharing, unless "Show in screen share" is on.
- **Never steals focus.** The overlay and panels use `WS_EX_NOACTIVATE`; if they activated, the paste would go to the overlay.
- **Resident model.** Whisper loads (and warms up its GPU pipelines) at startup and unloads after 30 minutes of no use.

## Structure

```
.claude-plugin/     plugin and local marketplace manifest
assets/claudetalk.ico  Start Menu / tray icon (embedded in claudetalk-dictation.exe)
assets/logo.png, assets/screenshots/  README images (from `claudetalk-dictation.exe --render-test`)
bin/                claudetalk.exe, built from native/ (claudetalk-dictation.exe comes from the release)
commands/           /voice /dictation
skills/talk/        /talk: turns talk mode on and off, changes settings
.mcp.json           `voice` MCP server → claudetalk.exe mcp (the `say` tool)
hooks/hooks.json    UserPromptSubmit / Stop / SessionStart → claudetalk.exe hook prompt|stop|session-start
scripts/install-app.ps1  adds the claudeTalk Start Menu (and desktop) shortcut
native/             Rust sources; SPEC.md is the behavior contract
  ct-core/          shared state: sessions and voices, speech queue, transcript rules
  claudetalk/       hooks, MCP server, /talk (toggle), speech player with Edge TTS
  dictation/        claudetalk-dictation.exe: audio, Whisper, wake phrase, hotkey,
                    paste and console typing, glass overlay and panels, tray
  bench/            speech-to-text benchmark behind the model choice
  build.cmd         build environment for the crates that compile whisper.cpp
```

## Diagnostics

- Dictation log: `%TEMP%\claudetalk-dictation.log`, with a `[diag]` line for each paste: the target window, whether it runs elevated, and how many events `SendInput` accepted.
- TTS and hooks log: `%TEMP%\claudetalk.log`.
- "Doesn't paste into that app but does into others": if the app runs as administrator and the daemon doesn't, Windows blocks the synthetic `Ctrl+V` (UIPI). Launch the daemon with the same privilege level.
- Two instances can't coexist: the second one warns and exits.
- Leftovers from older versions: `claudetalk.exe cleanup` (see [Upgrading](#upgrading-from-v05-or-earlier)).

## Development

Needs Rust, and for the dictation crate (whisper.cpp) Visual Studio Build Tools, CMake, Ninja, LLVM and the Vulkan SDK (`winget install Kitware.CMake Ninja-build.Ninja LLVM.LLVM KhronosGroup.VulkanSDK`).

```powershell
native\build.cmd . cargo test --workspace                # unit tests
native\build.cmd . cargo build --release --workspace     # both executables, in C:\ctb\release
copy C:\ctb\release\claudetalk*.exe bin\       # a local claudetalk-dictation.exe here wins over the download (git-ignored)
C:\ctb\release\claudetalk-dictation.exe --render-test %TEMP%\ct 1.5   # PNGs of the pill and panels
powershell -File native\tests\gen-golden.ps1             # optional: parity fixtures from your own transcripts
```

`build.cmd` sets up the MSVC environment, uses Ninja (MSBuild trips over long paths in the Vulkan shader build) and a short target directory (`C:\ctb`).

## License

MIT. See [LICENSE](LICENSE).
