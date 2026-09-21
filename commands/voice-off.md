---
description: Turns off claudeTalk voice mode (silence) and cuts any audio in progress
allowed-tools: ["Bash"]
---

Turn off voice mode by running this command with Bash and report its output to the user in a single line:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" off
```

If `${CLAUDE_PLUGIN_ROOT}` doesn't expand or the script doesn't exist, as a fallback edit the current workspace's `.claude/claudetalk.local.md` file and set `enabled: false`.

Don't say anything else: just confirm that voice mode is now off.
