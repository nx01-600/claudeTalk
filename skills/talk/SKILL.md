---
name: talk
description: Controls claudeTalk talk mode. Turns it on or off (while on, Claude talks to the user out loud until turned off), stops the current speech, and changes Claude's voice, speaking speed or the dictation silence time. Use when the user runs /talk, or asks in any words to start or stop talking out loud, to be quiet for now, or to change the voice, speed or silence time, e.g. "háblame", "respóndeme con voz", "ya no hables", "cállate", "cambia a la voz de Salomé", "habla más rápido", "ponle 5 segundos de silencio".
argument-hint: "[on|off|stop|voice <name>|rate <speed>|silence <seconds>]"
allowed-tools: Bash, mcp__plugin_claudeTalk_voice__say
---

Pick the action from what the user asked (or from the arguments):
- `on`: start talking out loud. `off`: turn talk mode off. `toggle`: plain /talk with no argument.
- `stop`: be quiet right now but keep talk mode on ("cállate", "para", "ya entendí").
- `voice NAME`: change Claude's voice. NAME is one of Salome, Gonzalo, Dalia, Jorge (Salomé and Dalia are women, Gonzalo and Jorge men; Salomé and Gonzalo are Colombian, Dalia and Jorge Mexican).
- `rate SPEED`: change how fast Claude talks: slow, normal, fast, faster, or a percentage like +10% or -5%.
- `silence SECONDS`: how long a pause ends a dictation (0.5 to 10, e.g. 4.5).

Arguments: $ARGUMENTS

Run with Bash, replacing ACTION (and VALUE when the action takes one):

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" ACTION VALUE
```

Then, depending on the output:
- **ON**: call the claudeTalk `say` tool with a short greeting in the user's language (for example "Listo, te escucho") and write one line confirming talk mode is on and that /talk turns it off. From now on follow the talk mode rules that arrive with each prompt.
- **OFF**: write one line confirming talk mode is off. Don't call `say`.
- **stop**: write nothing more than a very short acknowledgement.
- **voice / rate set**: the change is live at once. Confirm with one short spoken sentence in the new voice (just write it if talk mode reads short answers, or use `say`).
- **silence set**: confirm in one line. The dictation app picks it up within a second.
- **unknown value**: tell the user the options the script listed.

If the script is missing, edit the workspace's `.claude/claudetalk.local.md` and set `enabled: true` or `false` (create it with `enabled`, `skip_code: true` if needed).
