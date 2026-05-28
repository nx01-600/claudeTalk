---
description: Activa el modo voz de claudeTalk (Claude leera sus respuestas en voz alta)
allowed-tools: ["Bash"]
---

Activa el modo voz ejecutando este comando con Bash y reporta su salida al usuario en una sola linea:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" on
```

Si `${CLAUDE_PLUGIN_ROOT}` no se expande o el script no existe, como alternativa edita el archivo `.claude/claudetalk.local.md` del workspace actual y pon `enabled: true` (crealo con esos campos si no existe: enabled, voice: es-CO-GonzaloNeural, rate: "+0%", skip_code: true).

No digas nada mas: solo confirma que el modo voz quedo activo.
