---
name: talk
description: Turns claudeTalk talk mode on or off. While on, Claude talks to the user out loud (short answers are read, long ones get a spoken summary) until the mode is turned off. Use when the user runs /talk, or asks in any words to start or stop talking out loud, e.g. "háblame", "respóndeme con voz", "activa el modo voz", "ya no hables", "cállate", "apaga la voz".
argument-hint: "[on|off]"
allowed-tools: Bash, mcp__plugin_claudeTalk_voice__say
---

Switch claudeTalk talk mode. Pick the action:
- `on` if the user asked to start talking (or passed `on`),
- `off` if the user asked to stop talking (or passed `off`),
- `toggle` if they just ran /talk with no argument.

Arguments: $ARGUMENTS

Run with Bash, replacing ACTION:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" ACTION
```

Then, depending on the output:
- **ON**: call the claudeTalk `say` tool with a short greeting in the user's language (for example "Listo, te escucho") and write one line confirming talk mode is on and that /talk turns it off. From now on follow the talk mode rules that arrive with each prompt.
- **OFF**: write one line confirming talk mode is off. Don't call `say`.

If the script is missing, edit the workspace's `.claude/claudetalk.local.md` and set `enabled: true` or `false` (create it with `enabled`, `voice: es-CO-GonzaloNeural`, `rate: "+0%"`, `skip_code: true` if needed).
