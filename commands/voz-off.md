---
description: Desactiva el modo voz de claudeTalk (silencio) y corta el audio en curso
allowed-tools: ["Bash"]
---

Desactiva el modo voz ejecutando este comando con Bash y reporta su salida al usuario en una sola linea:

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-toggle.ps1" off
```

Si `${CLAUDE_PLUGIN_ROOT}` no se expande o el script no existe, como alternativa edita el archivo `.claude/claudetalk.local.md` del workspace actual y pon `enabled: false`.

No digas nada mas: solo confirma que el modo voz quedo desactivado.
