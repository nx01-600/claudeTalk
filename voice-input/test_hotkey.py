"""Diagnostico: confirma que Ctrl+Shift+Space dispara UNA vez por pulsacion
(sin auto-repeat) y que soltar la tecla se detecta por polling."""

import time

import hotkey


def on_press():
    print("HOTKEY DISPARADO (WM_HOTKEY)", flush=True)


gh = hotkey.GlobalHotkey(on_press=on_press)
gh.start()

print("Escuchando ctrl+shift+space. Mantene apretado 3s y sola. Ctrl+C para salir.", flush=True)
try:
    while True:
        if hotkey.is_key_down(hotkey.VK_SPACE):
            print("space: abajo", flush=True)
        time.sleep(0.2)
except KeyboardInterrupt:
    gh.stop()
