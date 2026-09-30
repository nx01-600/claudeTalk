# claudeTalk behavior spec (v0.5.4 baseline)

This is the contract for the Rust port. The PowerShell and Python code of
v0.5.4 is the reference implementation. Every port PR is checked against this
file, and a behavior that changes on purpose is noted in the "Changes" section
at the end.

## 1. Plugin surface

### Hooks
- `Stop`: speak the end of the turn (see §7). Timeout 15 s.
- `SessionStart`: register the session and start dictation. Timeout 10 s.
- `UserPromptSubmit`: inject the talk mode rules (see §6). Timeout 10 s.
- A new prompt does NOT cut speech; answers queue up.

### MCP server `voice`
- stdio JSON-RPC, one message per line, UTF-8 without BOM.
- `initialize` echoes `params.protocolVersion` (default `2024-11-05`) and
  returns `capabilities: {tools: {}}` and
  `serverInfo: {name: "claudeTalk-voice", version}`.
- `ping` returns `{}`.
- `tools/list` returns one tool, `say(text)`:
  - Description: "Speak a short phrase out loud to the user (claudeTalk talk mode). Plays in the background and returns at once; several calls play in order. Use only while talk mode is on, with 1-2 natural spoken sentences: no code, paths, symbols or markdown."
  - Property `text`: "What to say, in the user's language."
  - `text` is required.
- `tools/call`:
  - Result content is `[{type:"text", text}]`, where `text` is `spoken` or
    "talk mode is off: nothing was spoken. Don't call say until the user turns talk mode on."
  - Unknown tool → error `-32602` "unknown tool: X".
- Error codes:
  - Unknown method with an id → `-32601` "method not found: X".
  - Notifications (no id) get no answer.
  - Exception → `-32603`.
- The Stop hook recognises the tool by the pattern `mcp__plugin_claudeTalk*__say`.

### Skill `talk` and commands
- Skill `talk` runs `toggle ACTION [SETTING] [VALUE]` (§1.1).
- `/voice` runs `toggle status`.
- `/dictation` ensures the daemon is running.

### 1.1 toggle actions
`on | off | toggle | status | stop | settings | set`. The shortcuts
`voice | rate | volume | silence <v>` are rewritten to `set`.

Setting names and values go through ConvertTo-Plain first: trim, lowercase,
NFD, and combining marks dropped.

| name (aliases) | key | values |
|---|---|---|
| voice, voz | tts_voice | prefix salom/gonzalo/dalia/jorge/elena/alonso, or `^[a-z]{2}-[A-Z]{2}-\w+Neural$`. Stored per session (see below). |
| rate, speed, velocidad | tts_rate | slow/lenta -15%, normal +0%, fast/rapida +20%, faster/"muy rapida" +40%, or `^[+-]\d{1,3}%$` |
| volume, volumen | tts_volume | number clamped 0..100, int |
| silence, silencio | silence_ms | seconds clamped 0.5..10, stored as `round(s*4)*250` |
| sensitivity, sensibilidad | sensitivity | 0..100 |
| glass, vidrio | glass | 0..100 |
| hotkey, keys, teclas, atajo | hotkey | VK list, split on `+` (see KeyCodes). f1-f12 map to 0x6F+n; a-z and 0-9 to their VK. At least 2 keys. |
| sound, sonido | sound | bool |
| enter, auto_enter, send | auto_enter | bool |
| wake, wake_word, oye | wake_word | bool |
| phrase, wake_phrase, frase | wake_phrase | whitespace collapsed, not empty, at most 40 chars |
| spoken, speak_only_spoken, solo_voz | speak_only_spoken | bool |
| share, show_in_capture, capture | show_in_capture | bool |
| position, posicion | position | `^(bottom\|abajo)` → bottom, `^(top\|arriba)` → top |
| drag, remember_drag, arrastre | remember_drag | bool |
| language, idioma | language | `^(es\|span\|espa)` es, `^(en\|engl\|ingl)` en, `^auto` auto |
| theme, tema | — | always an error: "there is no theme setting any more: the overlay is always dark glass." |

Parsing rules:
- **Bool:** `^(on|true|yes|si|1|activ|encend|prend)` → true,
  `^(off|false|no|0|desactiv|apag)` → false, anything else is an error
  "'v' is not on/off.".
- **Numbers:** `,` becomes `.` and `%` is removed before parsing
  (invariant culture); failure → "'v' is not a number.".
- **KeyCodes:**
  - ctrl/control 0x11, shift 0x10, alt 0x12, win/windows 0x5B;
  - lctrl 0xA2, rctrl 0xA3, lshift 0xA0, rshift 0xA1, lalt 0xA4, ralt/altgr 0xA5, lwin 0x5B, rwin 0x5C;
  - space/espacio 0x20, tab 09, enter 0D, esc 1B, backspace 08, capslock 14;
  - insert 2D, delete 2E, home 24, end 23, pageup 21, pagedown 22;
  - left 25, up 26, right 27, down 28, menu 5D.

Output text (stdout, UTF-8). Every message is prefixed `claudeTalk:`, and
errors exit 1.
- `set`: "claudeTalk: <desc>. Applied now."
  - For a voice with a session: "claudeTalk: voice X for this session.[ Another session already talks with that voice.] Applied now."
  - Setting a voice stores it on the session. When the session followed the
    gear voice, or no other session is talking, it also writes `tts_voice` to
    the gear.
- `settings`: prints each key, then `= <compact json>`, then the help text.
- `stop`: stops only this session's speech.
- Without a session id, `on/off/toggle/status` fail with "can't tell which Claude Code session this is (run it from inside Claude Code)." and exit 1.
- `on`, with its own voice: "talk mode ON for this session with its OWN voice: N (code). Other sessions are talking with: a, b. Announce the voice (see the skill)."
- `on`, otherwise: "talk mode ON for this session (voice: N). Claude talks to you until you run /talk again."
- `off`: disables the session, stops its speech, "talk mode OFF for this session (silence)."
- `status`: "talk mode ON|OFF in this session | voice: N (code) | speed: R | other sessions talking: K"

## 2. State and IPC (these contracts must not change)

| Path | Format / notes |
|---|---|
| `%APPDATA%\claudeTalk\dictation.json` | Gear settings. Read utf-8 with the BOM tolerated; written atomically (temp + rename) without a BOM. Keys and defaults below. |
| `%APPDATA%\claudeTalk\sessions\<sid>.json` | `{enabled, voice, follows_default, updated}`. The sid is sanitised with `[^\w-]` replaced by `_`. Files untouched for 30 days are pruned on register. |
| `%APPDATA%\claudeTalk\live.json` | `{"<claude pid>": "<sid>"}`. Dead PIDs (not a live `claude` process) are dropped on every rewrite. |
| `%APPDATA%\claudeTalk\sessions.txt` | One interactive claude.exe PID per line. Appended by SessionStart, pruned by the daemon. |
| `%APPDATA%\claudeTalk\persistent.flag` | Manual launch: the watchdog never quits. |
| `%APPDATA%\claudeTalk\talk-active.flag` | Contains an ISO date. Exists while any live session has talk mode on. The wake listener runs only while it exists. |
| `%APPDATA%\claudeTalk\ducking.flag` | Exists while a recording runs. A phrase that starts while it exists gets its rate reduced by 15 points, floor -50%. |
| `%TEMP%\claudetalk_queue\<UTC ticks D20>.json` | `{text, voice, rate, volume, edge, ffplay, session}`. Played in name order. |
| `%TEMP%\claudetalk_player.pid` | Bare PID of the process playing right now. Exists only while a phrase plays. wake.py reads it. |
| `%TEMP%\claudetalk_player.session` | sid of the phrase playing right now |
| `%TEMP%\claudetalk.log`, `%TEMP%\claudetalk-dictation.log` | Diagnostics |
| Mutex `Local\claudetalk_sessions` | Wait 3 s; an abandoned mutex counts as owned |
| Mutex `Local\claudetalk_speaker` | Single queue drainer. Wait 0; if not owned, exit. Re-check the queue after releasing it. |
| Mutex `Local\claudeTalk-dictation` | Single daemon instance |
| Spoken mark | `🎙️ ` (U+1F399 U+FE0F, then a space) at the start of a dictated prompt. Detection: after TrimStart, the prompt starts with the U+1F399 surrogate pair. |
| Env | `CLAUDETALK_SESSION_ID` beats every other session source (tests) and also turns the talk flag on. Also used: `CLAUDE_PROJECT_DIR`, `CLAUDE_PLUGIN_ROOT`. |
| `.claude/claudetalk.local.md` | Found by walking up from cwd, else `CLAUDE_PROJECT_DIR`, else cwd. Key `skip_code` (default true), read with regex ``(?m)^\s*KEY\s*:\s*"?([^"\r\n]+?)"?\s*$``. |

`dictation.json` defaults:
- `hotkey [0x11,0x10,0x20]`, `silence_ms 2000`, `sensitivity 50`
  (legacy low/medium/high = 25/50/75)
- `sound true`, `auto_enter false`
- `wake_word false`, `wake_phrase "Oye Claude"`, `speak_only_spoken false`
- `glass 60`, `position "bottom"`, `remember_drag false`, `drag_pos null`, `show_in_capture false`
- `language "es"`
- `tts_voice "es-CO-GonzaloNeural"`, `tts_rate "+0%"`, `tts_volume 100`

Hook payload fields that are read: `session_id`, `cwd`, `transcript_path`,
`last_assistant_message`, `prompt`, `source`. Stdin is raw UTF-8.

## 3. Sessions and voices

- **Voice pool, in order:** Gonzalo CO, Salomé CO, Jorge MX, Dalia MX, Alonso US, Elena AR. Display names come from the same list.
- **Finding claude.exe:** walk up the parent chain, at most 12 steps, to the
  first `claude.exe`. SessionStart ignores headless sessions: command lines
  matching `--output-format`, `(^|\s)-p(\s|$)` or `--print`.
- **Session id resolution:** env override, then payload sid, then
  `live.json[claude pid]`.
- **Register, under the lock:**
  - If this claude.exe already mapped to another sid and the new sid has no
    file, copy the old state when it was enabled or had a voice. This covers
    /clear and /resume.
  - Map pid → sid, then prune old session files.
- **Effective voice:** the gear voice when `follows_default` is set or no voice is stored; otherwise the stored voice.
- **Enable, under the lock.** `others` = the effective voices of the other
  live, enabled sessions. Choose the voice in this order:
  1. The session's own voice, if it has one and it is not taken.
  2. Otherwise the gear default, if it is free; set follows_default.
  3. Otherwise the first free voice in the pool.
  4. Otherwise the least used pool voice.

  Then:
  - follows_default is false whenever voice ≠ default.
  - Set enabled, save, update the flag.
  - Return `own = voice ≠ default && others non-empty`.
- **Update flag:** if any live session is enabled (or the env override
  session is), write talk-active.flag; otherwise delete it.

## 4. Speech queue and playback

- **Add-Speech:**
  - Nothing for empty or whitespace-only text.
  - Write the item JSON (no BOM) and make sure a drainer runs.
  - The item snapshots voice, rate and volume when it is queued.
- **Drainer, per item in name order:**
  1. Apply the ducking rate.
  2. Clamp volume to 0..100.
  3. If the item file vanished (Stop), skip it.
  4. Write the pid and session files, then play it.
  5. Remove the pid and session files, and delete the item.
- **Stop-Speech(sid):**
  - Delete the queue items of that sid, plus unparsable ones. With no sid,
    delete all items.
  - If a sid is given and the playing session differs, stop here.
  - Otherwise cut the current playback. Never kill an unrelated process that
    reused the PID.
- **Previews** from the gear panel call Stop-Speech with no sid, then queue
  the sample with no session. Samples:
  - "Hola, así sueno cuando te hablo." (voice or speed changed);
  - "Hola, este es el volumen de mi voz." (volume slider released).

## 5. Transcript reading (Stop hook)

- **Test-IsPrompt:** `type=="user"`, not `isMeta`, not `isSidechain`, and
  content that is a string or an array without a `tool_result` block.
- **Read-Turn:**
  - Walk the lines from the end, skipping blank or unparsable ones.
  - Stop at the first prompt.
  - Skip sidechain entries.
  - Collect assistant messages (`message.role == "assistant"`) in order.
- **Prompt text:** the string itself, or the array's `text` blocks joined with `\n`.
- **Blocks, forward:**
  - `say` tool_use → spoke=true, workAfterSay=false, textAfter="".
  - Other tool_use → workAfterSay=true, textAfter="".
  - Text block with text → textAfter += text + "\n".
- **Race guard:**
  - `last = trim(last_assistant_message)`; `probe` = its first 40 chars.
  - Retry up to 12 times at 150 ms until textAfter contains probe. With no
    probe, until spoke or any text.
  - If the probe is still missing: textAfter = last, workAfterSay = false.
- **Decision:**
  - If onlySpoken is on and the prompt was not spoken: exit.
  - `clean = ConvertTo-Speech(textAfter)`; if empty, exit.
  - `fits = len(clean) ≤ 400 && textAfter has no "```"`.
  - fits → speak clean.
  - Else if `!spoke || workAfterSay` → speak "Te dejé la respuesta en pantalla.".
- **ConvertTo-Speech,** in order:
  1. Code fences: `(?s)```.*?```` becomes a space (skipCode), or all ``` marks are removed.
  2. `` `x` `` → x.
  3. `!?\[t\]\(u\)` → t.
  4. `(?m)^\s{0,3}#{1,6}\s*` removed.
  5. `(?m)^\s*>\s?` removed.
  6. `(?m)^\s*[-*+]\s+` removed.
  7. `(?m)^\s*[-*_]{3,}\s*$` removed.
  8. `(?m)^\s*\|.*\|\s*$` removed.
  9. `(?<!\w)[*_]{1,3}|[*_]{1,3}(?!\w)` removed.
  10. Stray backticks removed.
  11. `[ \t]+` → one space.
  12. `(\r?\n){2,}` → ". ".
  13. Trim.

  Note: .NET `\w` and `\s` are Unicode-aware.

## 6. UserPromptSubmit context

- Resolve the sid. If it is not a value in live.json (and there is no env
  override), register it.
- If talk mode is disabled: no output.
- Otherwise write talk-active.flag.
- If onlySpoken is on and the prompt was typed, output the typed-only paragraph.
- Otherwise output the rules block, plus the microphone-mark bullet when the prompt was spoken.
- Output shape: `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":…}}`.

## 7. SessionStart

1. Find the interactive claude.exe; exit if there is none or it is headless.
2. Register the session and update the flag.
3. Append the PID to sessions.txt if it is not there.
4. Start the dictation daemon in `--auto` mode if it is installed and not running.

## 8-10. Dictation daemon, wake, overlay

The full detail (audio thresholds, the wake regex and fuzzy matching, inject
tricks, overlay geometry, timings and easings, panel widgets, tray) lives in
the reference sources:
- `voice-input/audio.py`, `wake.py`, `inject.py`, `overlay.py`, `daemon_cli.py`, `config.py`;
- the numbers are summarised in `native/SPEC-dictation.md`, written when the
  daemon port starts.

Fragile details that must survive:
- **The overlay never takes focus.**
  - The paste target is the HWND captured when recording starts.
  - The paste happens only if that window is still in front; otherwise the
    text stays on the clipboard.
  - The text is always left on the clipboard.
- **Routing:** a wake-phrase dictation, or a dictation started from the
  desktop or taskbar, goes to the last Claude session:
  - The last session is tracked by (hwnd, topic, pid).
  - Preferred path: console input injection.
  - Fallback: an invisible alpha-0 window, then Ctrl+Tab through the tabs.
- **`SendInput`** needs a 40-byte `INPUT` struct on x64. A return value below 4 means UIPI is blocking it.
- **Wait for modifiers:** before pasting, wait up to 1 s for Shift, Alt and Win to be released.
- **Strict wake mode** while Claude talks and for 0.8 s after.
- **P75 speech level:** coughs don't mute later speech.
- **Calibration** keeps collecting samples until it has some, so a cold device never yields NaN.

## Changes on purpose (Rust port)

- **Talk rules (tokens):** the full rules go on the first talk prompt of a
  session, after SessionStart with source compact, clear or resume, and every
  8th prompt. The other prompts get a one-line reminder.
- **The Stop hook drops its fixed 300 ms sleep.** The retry loop covers the race.
- **Speech plays in-process** (native Edge TTS and rodio). edge-tts and ffplay
  are no longer needed. The `edge_tts_path` and `ffplay_path` keys are ignored.
- **Panel "Turn off dictation"** also deletes `persistent.flag`, like the tray does.
- **`transcribe` is serialised** (it could overlap wake and dictation before).
- **Dictation daemon (v0.7):**
  - Whisper large-v3-turbo q8_0 on whisper.cpp with Vulkan, instead of
    faster-whisper fp16 on CUDA. It makes fewer errors on the test set; see
    `bench/RESULTS.md`.
  - Silero VAD trims silence before Whisper (faster-whisper's vad_filter did
    this before) and decides which wake bursts are a voice.
  - The chord is detected from raw input events instead of polling every 15
    ms. The same GetAsyncKeyState edge logic decides.
  - The glass re-blurs only when the capture changed. The panels repaint
    only while they animate.
  - Ducking and "Claude is talking" are named events
    (`Local\claudetalk_ducking`, `Local\claudetalk_speaking`), and the
    speaker ramps its own volume. `ducking.flag` is still written.
  - Settings keys the daemon doesn't know are kept when it saves.
  - Launching the exe by hand (no `--auto`) writes `persistent.flag`, as
    dictation.vbs did.
  - The drag spot is stored in physical pixels, where v0.5 stored Qt's
    logical pixels.
  - The pill's hover cursors are the system hand and move cursors, not
    Qt's open and closed hands.
- **Cleanup (v0.7):**
  - `claudetalk cleanup [--yes]` removes older versions' leftovers: the Python
    environment, old scripts, faster-whisper's model, unused model files and
    old cached plugin copies. It also repoints stale shortcuts.
  - While leftovers exist, the SessionStart hook outputs `additionalContext`
    asking Claude to offer the cleanup. It costs no tokens once they're gone.
- **Distribution (v0.7):**
  - `claudetalk-dictation.exe` is not in git. `claudetalk.exe` downloads it
    from the GitHub release of its own version into
    `%LOCALAPPDATA%\claudeTalk\bin\<version>\`. It does this in a detached
    `fetch-dictation --start`, so the hook returns at once.
  - A copy next to `claudetalk.exe` wins over the download.
  - The daemon finds `claudetalk.exe` through
    `%APPDATA%\claudeTalk\claudetalk-path.txt`, which every SessionStart
    rewrites.
- **v0.7.1:**
  - `%TEMP%\claudetalk-dictation.log` is emptied when a daemon instance
    starts, and never holds transcribed text: `[wake] heard N words` and
    `[text] N words` replace the quoted text.
  - Every HTTP download goes through `ct_core::http::agent()` (NativeTls).
  - `cleanup` skips a path already listed (compared after canonicalizing).
  - Detached children don't inherit the spawner's std handles.
- **v0.8 (languages):**
  - `lang` in dictation.json is claudeTalk's language; `language` stays the
    dictation's (the same code, or `auto`). Without `lang`, the language is
    `language` when it's a code, else Windows' UI language (es/en), else en.
  - Voices, the voice pool for parallel sessions, the default wake phrase,
    the spoken samples, `left_on_screen`, Whisper's prompt and the panel text
    come from the language pack (`ct_core::lang`), not constants. Packs for
    other languages live in `%APPDATA%\claudeTalk\lang\<code>.json`.
  - `toggle set language X` applies a pack (exit 3 with instructions when
    there is none); new actions `toggle template CODE`, `toggle pack FILE`;
    new command `claudetalk voices CODE`; new setting `dictation auto|same`.
  - The talk rules name the configured language instead of "usually Spanish".
  - "Hey Claude" uses the same tuned matching as "Oye Claude".
- **v0.8 (talking pill):** the daemon polls every 150 ms (`wake::claude_is_talking`:
  the speaking event or a non-empty queue) while `speaking_indicator` (default
  true) is on. It shows a second glass window (`ui/talk.rs`) while Claude
  talks and hides it 700 ms after the last sign of speech. Its X runs `queue::stop(None)` and ignores
  speech for 900 ms. Placed at the dictation pill's center, or `GAP` to its
  left while the dictation pill is up; it glides between the two.
- **v0.8.1 (player):**
  - `ct_core::voice_link` (`Local\claudetalk_voice_link`, shared memory):
    `level` (0..1, written by the speaker's `Meter` source every ~25 ms as
    samples play), `paused` (the speaker pauses/resumes its player),
    `volume` + `volume_seq` (applied to the phrase playing).
  - The capsule's X and skip both cut only the phrase playing
    (`queue::signal_cut`) and unpause; `queue::stop` also unpauses.
  - The daemon's 150 ms poll heals a dead player: a dead pid in
    `claudetalk_player.pid` resets the speaking event and the player files;
    a non-empty queue with no speaker for 1.5 s starts `claudetalk speaker`
    (null stdio, detached).
  - Frames: `ui::pacer` thread, 16.667 ms, one message in flight at most.
    Live glass: `LIVE_MS` 45, one window per tick in turn.
  - `CLAUDETALK_NS` suffixes every kernel object name (`lock::named`); with
    it set, `Local\claudetalk_test_record` toggles a fake recording pill.
- **v0.8.2:** `audio::Resampler` uses a polyphase table (256 phases, the
  same Blackman-windowed sinc, rows normalized) instead of evaluating the
  kernel per tap; the history starts with K zeros instead of 32.
- **v0.8.3:** `stt::pick_gpu` passes whisper.cpp the `gpu_device` of the
  first discrete GPU (ggml device type GPU, counted among GPUs and iGPUs as
  whisper.cpp does), 0 when there is none. whisper.cpp's default 0 is the
  first Vulkan device, which on a hybrid laptop is the iGPU.
- **v0.8.3:** while a recording runs, `hotkey::EscapeGrab` registers Escape
  as a system hotkey (`RegisterHotKey`, worker thread) so the cancelling
  Escape is not delivered to the foreground window; it is released right
  after the recording.
