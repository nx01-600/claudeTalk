# Changelog

## 0.8.3 — Whisper on the right GPU

- **Whisper runs on the discrete GPU on hybrid laptops.** With NVIDIA
  Optimus the Intel iGPU is Vulkan's first device, and whisper.cpp took it:
  about 50 times slower than the RTX (real-time factor 2.3 instead of
  0.05). Each wake-phrase check took seconds, so "Oye Claude" was often
  missed. The dictation app now picks the first discrete GPU and logs it
  (`[model] gpu: ...`).
- **Escape cancels a dictation without interrupting Claude.** The Escape
  that cancelled a recording also reached the window in front, so it
  stopped whatever Claude was doing. While recording, the dictation app now
  takes Escape for itself (a system hotkey), then releases it.

## 0.8.2 — a quiet mic

- **The wake phrase listener no longer eats a CPU core.** While the mic
  was open ("Oye Claude" with talk mode on, all day long in a voice call)
  the dictation app used about 25 % of a core: the resampler that brings
  the mic down to 16 kHz computed a sine and two cosines per filter tap,
  193 taps per sample at 48 kHz. The filter is now precomputed for 256
  fractional positions, so each sample is a plain dot product: the same
  output (within 0.01), 45 times faster, and the whole app idles at about
  1 % while listening.

## 0.8.1 — a player for Claude's voice

- **The talking capsule is a player now**: pause / resume, a red X that
  drops only the answer playing (the queue goes on), skip with the number of
  answers waiting (blue when some come from another session) and a volume
  slider that opens from the speaker icon and applies to the answer playing.
- **Real bars**: the player measures the loudness of the audio as it
  reaches the sound card and shares it with the dictation app through a
  small shared-memory block (`ct_core::voice_link`); no extra analysis.
- **No answer lost**: the X no longer empties the queue. If the player dies
  mid-answer, the dictation app clears its "speaking" signal and starts a new
  player for what is still queued.
- **Draggable capsule**: drag it by its body; the dictation pill moves with
  it, and **Remember dragged spot** keeps the place.
- **Smooth animations**: frames come from a small pacing thread at an even
  60 fps (Windows' timer gave ~40 with uneven gaps), the live glass refreshes
  one window per tick instead of all at once, and the capsule only captures
  the area it paints. `CLAUDETALK_PERF=1` logs frame and glass timings.
- **Isolated tests**: `CLAUDETALK_NS` suffixes every named mutex, event and
  shared-memory block, so test instances can't touch the real ones.

## 0.8.0 — any language

- **One language setting** (`/talk set language en`, or the gear's
  **Language**) switches Claude's voices, the dictation language, Whisper's
  hint, the default wake phrase ("Oye Claude" / "Hey Claude"), the fixed
  spoken lines and the gear panel and tray menu. Spanish and English are
  built in (six Edge voices each).
- **Stop button for Claude's voice**: while Claude talks, a glass pill with
  moving bars and a red X appears where the dictation pill goes. The X
  silences Claude (queue included) without turning talk mode or dictation
  off. It slides left of the dictation pill while that one is up, follows it
  when dragged, and can be hidden (**Show when Claude talks**,
  `/talk set indicator off`).
- **Other languages**: Claude writes a language pack the first time one is
  asked for (`claudetalk voices CODE`, `toggle template CODE`,
  `toggle pack FILE`), saved in `%APPDATA%\claudeTalk\lang\`. See
  `docs/LANGUAGES.md`.
- **Detect language automatically** in the gear (`/talk set dictation auto`)
  lets Whisper detect the spoken language on each dictation.
- Talking sessions get voices of the new language when it changes.
- New installs start in Windows' display language (Spanish or English, else
  English). Older installs keep their dictation language.
- The panel shrinks a label that doesn't fit next to its control, and splits
  the voices evenly over rows.
- The cleanup notice and the README tell Claude to always ask the owner
  before removing an older version's leftovers, even when it installed
  claudeTalk itself.

## 0.7.1 — fixes

- **Model download.** A fresh install panicked when downloading the Whisper
  and VAD models (ureq fell back to Rustls, which isn't compiled in), so
  dictation never started. Every download now uses one agent,
  `ct_core::http::agent()`, pinned to the Windows TLS stack.
- **Privacy.** The dictation log (`%TEMP%\claudetalk-dictation.log`) no longer
  records what the wake-phrase listener hears or what you dictate: only word
  counts and events. The log is emptied every time the daemon starts, which
  also wipes what older versions wrote.
- **Cleanup.** `claudetalk cleanup` listed the Python venv twice when
  `venv-path.txt` pointed at the default folder, doubling the total and
  failing on the second removal.
- **Hooks.** Processes started from a hook no longer inherit Claude Code's
  stdin/stdout/stderr, so a detached daemon can't keep a hook or a Bash task
  waiting.

## 0.7.0 — native dictation, no Python left

- **Dictation.** `bin\claudetalk-dictation.exe` (Rust) replaces the Python
  daemon (`voice-input\`) with the same features and settings. Measured on
  an RTX 5070 Ti laptop:

  | | Python daemon | Rust daemon |
  |---|---|---|
  | RAM, idle | 279 MB | 13 MB |
  | RAM, model loaded | 279 MB | 97 MB |
  | VRAM | ~2.1 GB | 0.85 GB |
  | CPU at rest | 4.5 % of a core | 0.4 % |
  | Spanish WER on the benchmark | 5.86 % | 5.44 % |

- **Speech-to-text.** Whisper large-v3-turbo q8_0 on whisper.cpp with Vulkan,
  instead of faster-whisper fp16 on CUDA, so any GPU works (NVIDIA, AMD or
  Intel). See `native/bench/RESULTS.md`.
- **Wake phrase.** Silero VAD filters out sound bursts that aren't a voice
  before Whisper runs.
- **Chord.** Detected from raw keyboard events instead of polling every 15 ms.
- **Install.** Nothing to install besides the plugin. The dictation app
  (~60 MB, not kept in git) and the models (~0.9 GB) download on first run.
  The dictation app comes from the GitHub release of the same version.
- **Cleanup.** `claudetalk.exe cleanup [--yes]` removes what older versions
  left behind. The `SessionStart` hook tells Claude while leftovers exist.
- **Fix.** "Turn off dictation" from the gear panel now also forgets the
  manual-launch flag, as the tray's button did.
- **Removed:** `voice-input\`, `setup-voice.ps1`, `dictation.vbs`,
  `make-icon.py`.

## 0.6.0 — native plugin binary

- `bin\claudetalk.exe` (Rust) replaces every PowerShell script: hooks, the
  `say` MCP server, `/talk` and the speech player.
- **Hooks.** A prompt hook takes ~17 ms instead of 0.4–1.5 s.
- **Speech.** Edge TTS is spoken natively. Playback starts with the first
  chunk, and back-to-back phrases start in ~20 ms. edge-tts and ffmpeg are no
  longer needed.
- **Talk mode tokens.** The full rules go on the first prompt and every 8th
  one; a one-line reminder goes on the rest.

## 0.5.4 and earlier

PowerShell hooks and a Python/PySide6 dictation daemon. Their behavior is the
reference the native versions keep: see `native/SPEC.md`.
