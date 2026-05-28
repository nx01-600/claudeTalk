# claudeTalk

Le da **voz** a Claude Code. Cuando el modo voz esta activo, cada respuesta de
Claude se lee en voz alta usando `edge-tts` (voces neuronales de Microsoft) y se
reproduce con `ffplay`. Es la **etapa 1** del proyecto; el norte es una
conversacion en vivo interrumpible (hablar y que Claude escuche).

## Como funciona

1. Cuando Claude termina de responder se dispara el hook `Stop`.
2. `scripts/speak.ps1` lee la ultima respuesta del transcript, le quita el
   markdown y los bloques de codigo, y la sintetiza con `edge-tts`.
3. El audio se reproduce en segundo plano con `ffplay` (no bloquea la terminal).
4. Todo ocurre solo si el modo voz esta `enabled: true`.

## Comandos

- `/voz-on`  - activa el modo voz.
- `/voz-off` - desactiva y corta el audio en curso.
- `/voz`     - muestra el estado y la voz actual.

## Configuracion

El estado vive en `.claude/claudetalk.local.md` del workspace (por proyecto):

```yaml
---
enabled: true
voice: es-CO-GonzaloNeural  # voz neutra LATAM (cambia a gusto)
rate: "+0%"                 # velocidad: -20%, +10%, etc.
skip_code: true             # true = omite bloques de codigo al leer
---
```

Voces sugeridas (neutras LATAM): `es-CO-SalomeNeural`, `es-CO-GonzaloNeural`,
`es-MX-DaliaNeural`, `es-MX-JorgeNeural`. Lista completa: `edge-tts --list-voices`.

## Requisitos

- `edge-tts` (Python) en el PATH o en la ruta por defecto. Necesita internet.
- `ffmpeg`/`ffplay` en el PATH o en la ruta por defecto.

Si no estan en el PATH, agrega `edge_tts_path:` y `ffplay_path:` al frontmatter.

## Diagnostico

Errores y trazas se registran en `%TEMP%\claudetalk.log`.
