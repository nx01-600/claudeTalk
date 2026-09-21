# claudeTalk

Voz para **Claude Code** en Windows, en dos direcciones:

- **Claude habla**: lee sus respuestas en voz alta (voces neuronales de Microsoft vía `edge-tts`).
- **Vos dictás**: apretás un acorde de teclas, hablás, y el texto aparece transcripto en la ventana que tenías enfocada. Todo local: Whisper corre en tu GPU (o CPU), el audio nunca sale de tu máquina.

El dictado viene con un overlay flotante estilo *liquid glass* (blanco y negro, claro u oscuro) con barras que siguen tu voz, y un panel de ajustes desde la tuerca.

> Estado: funcional y en uso diario. Es la etapa 2 de un proyecto cuyo norte es una conversación en vivo con Claude, interrumpible, tipo llamada.

---

## Qué hace

| | |
|---|---|
| **Dictado por voz** | Un toque de `Alt izq + Ctrl der` (configurable) arranca a grabar. Corta solo tras 2 s de silencio, o con otro toque. `Esc` cancela. |
| **Pegado inteligente** | El texto se pega en la ventana que tenía el foco al empezar a grabar; si cambiaste de ventana, no pega en cualquier lado. El texto **siempre** queda además en el portapapeles. |
| **Overlay liquid glass** | Píldora flotante con vidrio esmerilado, barras de volumen en vivo y tuerca de ajustes. Nunca roba el foco. |
| **Ajustes en vivo** | Teclas de activación (captura la combinación que apretes), corte por silencio, sonido, tema claro/oscuro, intensidad del vidrio, posición, idioma (español / inglés / auto). Apagar pide confirmación. |
| **Ciclo de vida** | Con el plugin instalado, el dictado arranca solo al abrir Claude Code y se cierra solo cuando no queda ninguno abierto. También podés lanzarlo a mano como app (queda en la bandeja). |
| **Claude habla (TTS)** | `/voz-on` y `/voz-off`. Lee la última respuesta, omite bloques de código, no bloquea la terminal. |

## Requisitos

- Windows 10/11.
- [Claude Code](https://claude.com/claude-code).
- Python 3.11 a 3.13 (para el dictado).
- GPU NVIDIA con CUDA 12 para transcripción en décimas de segundo. Sin GPU funciona en CPU (varios segundos por frase).
- Para la voz de Claude (TTS): `edge-tts` (`pip install edge-tts`, necesita internet) y `ffmpeg` (`winget install Gyan.FFmpeg`).

## Instalación

### 1. Plugin en Claude Code

```
git clone https://github.com/nx01-600/claudeTalk.git
```

Dentro de Claude Code:

```
/plugin marketplace add C:\ruta\a\claudeTalk
/plugin install claudeTalk@claudeTalk
```

### 2. Dictado por voz (una sola vez)

```powershell
powershell -ExecutionPolicy Bypass -File C:\ruta\a\claudeTalk\scripts\setup-voice.ps1 -Shortcut
```

Crea un entorno virtual en `%LOCALAPPDATA%\claudeTalk\venv`, instala las dependencias (`faster-whisper`, `PySide6`, `sounddevice`, runtime CUDA) y, con `-Shortcut`, deja un acceso directo **claudeTalk Dictado** en el escritorio. El modelo Whisper `large-v3-turbo` (~1,6 GB) se descarga solo la primera vez que dictás.

Listo. La próxima sesión de Claude Code ya arranca con el dictado activo.

## Uso

### Dictar

1. Enfocá la ventana donde querés el texto (la terminal de Claude Code, un editor, lo que sea).
2. Tocá **Alt izquierdo + Ctrl derecho**. Suena una nota suave y aparece la píldora.
3. Hablá. Al callarte 2 segundos (o al tocar el acorde de nuevo) transcribe y pega.

Si mientras tanto cambiaste de ventana, no pega nada: el texto te queda en el portapapeles para `Ctrl+V`.

### Ajustes

Clic en la tuerca de la píldora (o clic derecho en el ícono de la bandeja → **Ajustes**).

- **Teclas**: clic y apretá la combinación nueva; se guarda al soltar. `Esc` cancela.
- **Corte por silencio**: 1, 2 o 3 s.
- **Sonido al iniciar**: la nota al empezar a grabar.
- **Tema** claro u oscuro, **Vidrio** (cuánto blur y transparencia), **Posición** abajo o arriba.
- **Idioma**: español, inglés o detección automática.
- **Apagar dictado**: cierra el daemon por completo, con confirmación.

Los ajustes viven en `%APPDATA%\claudeTalk\dictado.json` y se aplican al instante.

### Arrancarlo y apagarlo

- Arranca solo con cada sesión de Claude Code (hook `SessionStart`) y se cierra solo cuando cerrás el último Claude Code.
- A mano: acceso directo **claudeTalk Dictado**, o `wscript scripts\dictado.vbs`, o `/dictado` dentro de Claude Code. Lanzado a mano, queda hasta que lo apagues.
- Apagar: tuerca → **Apagar dictado**, o bandeja → **Apagar dictado**. Siempre pide confirmación.

### Voz de Claude (TTS)

- `/voz-on` activa, `/voz-off` desactiva y corta el audio, `/voz` muestra el estado.
- Configuración por proyecto en `.claude/claudetalk.local.md`:

```yaml
---
enabled: true
voice: es-CO-GonzaloNeural  # cualquier voz de `edge-tts --list-voices`
rate: "+0%"
skip_code: true
---
```

## Cómo funciona

```
acorde de teclas ──► grabación (16 kHz, corte por silencio) ──► faster-whisper (GPU)
        │                       │                                       │
        │                  overlay.py                                   ▼
        │           píldora + barras de nivel               portapapeles + Ctrl+V simulado
        │                                                   (solo si el foco no cambió)
        └── hotkey.py: polling de GetAsyncKeyState cada 15 ms, sin hooks de teclado
```

Decisiones que vale la pena conocer (todas están explicadas en los docstrings):

- **Hotkey sin hooks.** `RegisterHotKey` no distingue Alt izquierdo de derecho ni acepta acordes de solo modificadores; los hooks de bajo nivel (`keyboard`, `pynput`) traen tormentas de auto-repeat. Polling de `GetAsyncKeyState` resuelve ambas cosas con costo despreciable.
- **Pegar, no teclear.** Escribir carácter por carácter con `SendInput` pierde tildes según la app y puede caer en la ventana equivocada a mitad de camino. Pegar por portapapeles es atómico y conserva el Unicode.
- **Vidrio sin APIs frágiles.** El backdrop nativo de Windows 11 (Mica/Acrylic) devuelve un panel sólido para ventanas con contenido pintado a mano. El overlay captura lo que hay detrás, lo desenfoca y lo usa de fondo.
- **Nunca roba el foco.** Overlay y panel usan `WS_EX_NOACTIVATE`; si se activaran, el pegado iría a parar al overlay.
- **Modelo residente.** Whisper se precarga al arrancar y se descarga tras 30 min sin uso para liberar VRAM.

## Estructura

```
.claude-plugin/     manifiesto del plugin y marketplace local
commands/           /voz-on /voz-off /voz /dictado
hooks/hooks.json    Stop → speak.ps1 (TTS)   SessionStart → voice-daemon-ensure.ps1
scripts/
  speak.ps1               TTS: transcript → edge-tts → ffplay
  voice-toggle.ps1        estado del modo voz
  setup-voice.ps1         instala el dictado (venv + dependencias + acceso directo)
  voice-daemon-ensure.ps1 lanza el daemon en modo --auto si no corre
  dictado.vbs             lanzador manual sin consola
voice-input/
  daemon_cli.py    orquestación, bandeja, modo --auto, única instancia
  hotkey.py        acorde por polling, captura de teclas
  audio.py         grabación con calibración de ruido y corte por silencio
  stt.py           faster-whisper residente (GPU, fallback CPU)
  inject.py        portapapeles + Ctrl+V, guardia de foco, diagnóstico
  overlay.py       píldora, panel de ajustes, vidrio, animaciones
  config.py        ajustes persistentes (%APPDATA%\claudeTalk\dictado.json)
  sounds.py        nota de inicio sintetizada
  test_*.py        diagnósticos manuales
```

## Diagnóstico

- Log del dictado (cuando corre sin consola): `%TEMP%\claudetalk-dictado.log`. Corriéndolo a mano en una terminal (`python voice-input\daemon_cli.py`) ves lo mismo en vivo, incluida una línea `[diag]` por cada pegado con la ventana destino, si corre elevada y cuántos eventos aceptó `SendInput`.
- Log del TTS: `%TEMP%\claudetalk.log`.
- "No pega en esa app pero sí en otras": si la app corre como administrador y el daemon no, Windows bloquea el `Ctrl+V` sintético (UIPI). Lanzá el daemon con el mismo nivel de privilegios.
- Dos instancias no pueden convivir: la segunda avisa y se cierra.

## Desarrollo

```powershell
cd voice-input
python -m venv .venv
.\.venv\Scripts\pip install -r requirements.txt
.\.venv\Scripts\python daemon_cli.py        # con consola y logs en vivo
.\.venv\Scripts\python test_hotkey.py       # ver qué teclas detecta
.\.venv\Scripts\python test_inject.py       # pegar en un Notepad de prueba
```

Si existe `voice-input\.venv`, tanto el hook como el lanzador lo prefieren sobre el venv de `%LOCALAPPDATA%`.

## Licencia

MIT. Ver [LICENSE](LICENSE).
