---
description: Arranca el dictado por voz de claudeTalk si no esta corriendo (queda en la bandeja; se apaga desde la tuerca)
allowed-tools: ["Bash"]
---

Ejecuta este comando con Bash y luego dile al usuario, en una linea, que el dictado quedo corriendo (o que ya estaba) y cual es el acorde de activacion por defecto (Alt izquierdo + Ctrl derecho, cambiable desde la tuerca del overlay):

```
powershell -NoProfile -ExecutionPolicy Bypass -File "${CLAUDE_PLUGIN_ROOT}/scripts/voice-daemon-ensure.ps1"
```

Si el script no lanza nada porque el dictado no esta instalado, indica que hay que correr `scripts/setup-voice.ps1` del plugin una vez (ver README).
