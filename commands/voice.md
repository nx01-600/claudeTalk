---
description: Shows the status of claudeTalk talk mode (ON/OFF and current voice)
allowed-tools: ["Bash"]
---

Check the talk mode status by running this command with Bash and report its output to the user as is:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" status
```
