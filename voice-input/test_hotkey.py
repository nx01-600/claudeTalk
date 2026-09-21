"""Diagnostico: confirma que el acorde configurado dispara UNA vez por
pulsacion (sin auto-repeat) y muestra que teclas ve `pressed_keys()`,
util para verificar la captura de ajustes."""

import time

import config as cfg
import hotkey

config = cfg.Config()
keys = config.get("hotkey")


def on_press():
    print("ACORDE DISPARADO", flush=True)


chord = hotkey.ChordHotkey(on_press=on_press, keys=keys)
chord.start()

print(f"Escuchando {cfg.hotkey_label(keys)}. Ctrl+C para salir.", flush=True)
try:
    last = []
    while True:
        pressed = hotkey.pressed_keys()
        if pressed != last:
            print("teclas:", cfg.hotkey_label(pressed) or "-", flush=True)
            last = pressed
        time.sleep(0.05)
except KeyboardInterrupt:
    chord.stop()
