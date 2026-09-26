---
description: Starts claudeTalk voice dictation if it isn't already running (sits in the tray; turn it off from the gear)
allowed-tools: ["Bash"]
---

Run this command with Bash and then tell the user, in one line, that dictation is now running (or was already running) and what the default activation chord is (Ctrl + Shift + Space, changeable from the overlay's gear):

```
"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" dictation
```

If it says dictation isn't installed, say that `scripts/setup-voice.ps1` from the plugin needs to be run once (see README).
