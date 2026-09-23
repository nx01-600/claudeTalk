---
name: talk
description: Controls claudeTalk. Turns talk mode on or off (while on, Claude talks to the user out loud until turned off), stops the current speech, and changes ANY claudeTalk setting from the gear panel - Claude's voice and speed, dictation silence time, mic sensitivity, hotkey, start chime, send with Enter, "Oye Claude" wake word, speaking only to dictated messages, visibility in screen sharing, glass, overlay position, dictation language. Use when the user runs /talk, or asks in any words to start or stop talking out loud, to be quiet for now, or to change any of those settings, e.g. "háblame", "ya no hables", "cállate", "cambia a la voz de Salomé", "habla más rápido", "ponle 5 segundos de silencio", "que no se mande solo con Enter", "que se vea cuando comparto pantalla", "solo háblame cuando yo hable", "cambia el atajo a control alt espacio".
argument-hint: "[on|off|stop|settings|set <setting> <value>]"
allowed-tools: Bash, mcp__plugin_claudeTalk_voice__say
---

Arguments: $ARGUMENTS

Everything runs through one script:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" ACTION [SETTING] [VALUE]
```

## Talk mode
- `on`: start talking out loud. `off`: turn talk mode off. `toggle`: plain /talk with no argument.
- Talk mode is per session: every Claude Code session turns it on and off on its own. When other sessions are already talking, this one gets a different voice automatically, so the user can tell sessions apart by ear.
- `stop`: be quiet right now but keep talk mode on ("cállate", "para", "ya entendí").

## Settings: `set SETTING VALUE`
Every change is saved at once and the dictation app applies it within a second: no restart, the next answer already uses it.

| SETTING | VALUE | What it is |
|---|---|---|
| `voice` | Salome, Gonzalo, Dalia, Jorge, Elena, Alonso | Claude's voice in this session (Salomé, Dalia and Elena women, Gonzalo, Jorge and Alonso men; Salomé/Gonzalo Colombian, Dalia/Jorge Mexican, Elena Argentine, Alonso US neutral) |
| `rate` | slow, normal, fast, faster, or +10% / -5% | how fast Claude talks |
| `silence` | seconds, 0.5 to 10 (e.g. 4.5) | pause that ends a dictation |
| `sensitivity` | 0 to 100 | mic sensitivity; higher picks up a softer voice, lower ignores background voices |
| `hotkey` | keys joined by `+`: ctrl, shift, alt, win, lctrl, rshift, ralt..., space, tab, enter, f1-f12, letters, digits | dictation shortcut, e.g. `ctrl+alt+space` |
| `sound` | on / off | chime when a recording starts |
| `enter` | on / off | press Enter after pasting, so the dictated message is sent |
| `wake` | on / off | start dictating by saying "Oye Claude" (only while talk mode is on) |
| `spoken` | on / off | talk mode answers out loud only messages the user dictated (they start with 🎙️); typed ones get text only |
| `share` | on / off | overlay visible in screen sharing and recordings |
| `glass` | 0 to 100 | glass effect intensity |
| `position` | bottom / top | where the overlay appears |
| `language` | es / en / auto | dictation language |

`settings` prints every current value, to answer "how is it set up now?".

## After running
- **ON**: call the claudeTalk `say` tool with a short greeting in the user's language (for example "Listo, te escucho") and write one line confirming talk mode is on and that /talk turns it off. From now on follow the talk mode rules that arrive with each prompt.
- **ON with its OWN voice** (the script says other sessions are talking): the greeting introduces the voice, e.g. "Hola, soy Elena. Esta va a ser mi voz en esta sesión, para que no me confundas con las otras." Write one line saying which voice this session got and which ones the other sessions use.
- **OFF**: write one line confirming talk mode is off. Don't call `say`.
- **stop**: only a very short acknowledgement.
- **set**: confirm in one short sentence in the user's language what changed. For `voice`/`rate`, that sentence already plays in the new voice.
- **error** (unknown setting or value): tell the user the valid options the script printed.

If the user asks for several changes at once, run `set` once per setting.
The overlay is always dark glass: there is no theme setting. Turning dictation off completely is not a setting: tell the user to use the gear's "Turn off dictation" button.
