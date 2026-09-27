---
name: talk
description: Controls claudeTalk. Turns talk mode on or off (while on, Claude talks to the user out loud until turned off), stops the current speech, and changes ANY claudeTalk setting from the gear panel - Claude's voice, speed and volume, dictation silence time, mic sensitivity, hotkey, start chime, send with Enter, "Oye Claude" wake word and its phrase, speaking only to dictated messages, the "Claude is talking" pill with its stop button, visibility in screen sharing, glass, overlay position, remembering where the overlay was dragged, claudeTalk's language (voices, dictation, wake phrase and panel follow it; Spanish and English built in, any other language by writing a language pack), automatic language detection for dictation. Also cleans up the files older claudeTalk versions left behind (old Python environment, old plugin copies). Use when the user runs /talk, or asks in any words to start or stop talking out loud, to be quiet for now, or to change any of those settings, e.g. "háblame", "ya no hables", "cállate", "cambia a la voz de Salomé", "habla más rápido", "baja el volumen", "habla más duro", "ponle 5 segundos de silencio", "que no se mande solo con Enter", "que se vea cuando comparto pantalla", "solo háblame cuando yo hable", "cambia el atajo a control alt espacio", "que la frase para activarte sea Hola Jarvis", "limpia lo que dejó la versión vieja de claudeTalk", "put claudeTalk in English", "quiero claudeTalk en francés".
argument-hint: "[on|off|stop|settings|set <setting> <value>]"
allowed-tools: Bash, mcp__plugin_claudeTalk_voice__say
---

Arguments: $ARGUMENTS

Everything runs through one script:

```
"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" toggle ACTION [SETTING] [VALUE]
```

## Talk mode
- `on`: start talking out loud. `off`: turn talk mode off. `toggle`: plain /talk with no argument.
- Talk mode is per session: every Claude Code session turns it on and off on its own. When other sessions are already talking, this one gets a different voice automatically, so the user can tell sessions apart by ear.
- `stop`: be quiet right now but keep talk mode on ("cállate", "para", "ya entendí").

## Settings: `set SETTING VALUE`
Every change is saved at once and the dictation app applies it within a second: no restart, the next answer already uses it.

| SETTING | VALUE | What it is |
|---|---|---|
| `voice` | a voice of the current language (`settings` lists them): Spanish Salome, Gonzalo, Dalia, Jorge, Elena, Alonso; English Andrew, Ava, Brian, Emma, Ryan, Sonia; or any Edge id like `fr-FR-DeniseNeural` | Claude's voice in this session |
| `rate` | slow, normal, fast, faster, or +10% / -5% | how fast Claude talks |
| `volume` | 0 to 100 | how loud Claude talks (100 = full) |
| `silence` | seconds, 0.5 to 10 (e.g. 4.5) | pause that ends a dictation |
| `sensitivity` | 0 to 100 | mic sensitivity; higher picks up a softer voice, lower ignores background voices |
| `hotkey` | keys joined by `+`: ctrl, shift, alt, win, lctrl, rshift, ralt..., space, tab, enter, f1-f12, letters, digits | dictation shortcut, e.g. `ctrl+alt+space` |
| `sound` | on / off | chime when a recording starts |
| `enter` | on / off | press Enter after pasting, so the dictated message is sent |
| `wake` | on / off | start dictating by saying the wake phrase (only while talk mode is on) |
| `phrase` | any words, quoted, e.g. `"Hola Jarvis"` | the wake phrase itself (default "Oye Claude"); 2 or 3 words that don't come up in normal talk work best |
| `spoken` | on / off | talk mode answers out loud only messages the user dictated (they start with 🎙️); typed ones get text only |
| `indicator` | on / off | while Claude talks, a glass pill with a red X that silences Claude (talk mode stays on) |
| `share` | on / off | overlay visible in screen sharing and recordings |
| `glass` | 0 to 100 | glass effect intensity |
| `position` | bottom / top | where the overlay appears (also forgets a dragged spot) |
| `drag` | on / off | the overlay can be dragged anywhere with the mouse; on = it comes back where it was last dragged, off = it resets to `position` every time |
| `language` | es, en, or any language code or name (fr, pt, "francés") | claudeTalk's language: voices, dictation, default wake phrase, spoken lines and gear panel all switch (see "Languages" below) |
| `dictation` | auto / same | auto = Whisper detects the spoken language on each dictation; same = claudeTalk's language |

`settings` prints every current value, to answer "how is it set up now?".

## Languages
Spanish and English are built in. For any other language, `set language CODE` exits with code 3 and prints the steps; do them yourself, without asking the user to:
1. Run `"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" voices CODE` and pick 2 to 6 voices: men and women alternating, mixed accents, the most natural first (it becomes the default).
2. Run `... toggle template CODE`, translate every value into that language (keep the JSON keys and the `{phrase}`, `{label}`, `{pct}` placeholders; keep panel labels short), fill `name` with the language's own name and `voices` with the ids you picked and a short name for each.
3. Write it to a file in your scratchpad, run `... toggle pack FILE`, fix what it reports, then `... toggle set language CODE`.
Then confirm in one sentence, in the new language, which voices it has.

## Cleaning up older versions
Up to v0.5 claudeTalk used a Python environment (~3 GB) that the native versions no longer need, and Claude Code's cache may keep old plugin copies with it. Run `"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" cleanup` to list what would go (with sizes), tell the user the total, and only after they agree (never on your own, even if you installed claudeTalk yourself) run `"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" cleanup --yes`. It only touches claudeTalk's own leftovers; ffmpeg and edge-tts are just mentioned, since other tools may use them.

## After running
- **ON**: call the claudeTalk `say` tool with a short greeting in the user's language (for example "Listo, te escucho" or "Ready, I'm listening") and write one line confirming talk mode is on and that /talk turns it off. From now on follow the talk mode rules that arrive with each prompt.
- **ON with its OWN voice** (the script says other sessions are talking): the greeting introduces the voice, e.g. "Hola, soy Elena. Esta va a ser mi voz en esta sesión, para que no me confundas con las otras." Write one line saying which voice this session got and which ones the other sessions use.
- **OFF**: write one line confirming talk mode is off. Don't call `say`.
- **stop**: only a very short acknowledgement.
- **set**: confirm in one short sentence in the user's language what changed. For `voice`/`rate`, that sentence already plays in the new voice.
- **error** (unknown setting or value): tell the user the valid options the script printed.

If the user asks for several changes at once, run `set` once per setting.
The overlay is always dark glass: there is no theme setting. Turning dictation off completely is not a setting: tell the user to use the gear's "Turn off dictation" button.
