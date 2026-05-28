---
description: Muestra el estado del modo voz de claudeTalk (ON/OFF y voz actual)
allowed-tools: ["Bash"]
---

Consulta el estado del modo voz ejecutando este comando con Bash y reporta su salida tal cual al usuario:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" status
```
