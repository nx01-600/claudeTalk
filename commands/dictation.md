---
description: Starts claudeTalk voice dictation if it isn't already running (sits in the tray; turn it off from the gear)
allowed-tools: ["Bash"]
---

Run this command with Bash and then tell the user, in one line, that dictation is now running (or was already running) and what the default activation chord is (Ctrl + Shift + Space, changeable from the overlay's gear):

```
"${CLAUDE_PLUGIN_ROOT}/bin/claudetalk.exe" dictation
```

If it says it is downloading the dictation app, tell the user it's a one-time ~60 MB download for this version and that dictation starts by itself when it finishes. The first run also downloads the voice model (~0.9 GB) in the background; the tray icon's tooltip shows the progress.
