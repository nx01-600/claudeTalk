---
description: Turns on claudeTalk voice mode (Claude will read its responses out loud)
allowed-tools: ["Bash"]
---

Turn on voice mode by running this command with Bash and report its output to the user in a single line:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" on
```

If `${CLAUDE_PLUGIN_ROOT}` doesn't expand or the script doesn't exist, as a fallback edit the current workspace's `.claude/claudetalk.local.md` file and set `enabled: true` (create it with these fields if it doesn't exist: enabled, voice: es-CO-GonzaloNeural, rate: "+0%", skip_code: true).

Don't say anything else: just confirm that voice mode is now on.
