"""Diagnostic: confirms that the configured chord fires ONCE per press
(no auto-repeat) and shows what keys `pressed_keys()` sees, useful to
verify settings capture."""

import time

import config as cfg
import hotkey

config = cfg.Config()
keys = config.get("hotkey")


def on_press():
    print("CHORD FIRED", flush=True)


chord = hotkey.ChordHotkey(on_press=on_press, keys=keys)
chord.start()

print(f"Listening for {cfg.hotkey_label(keys)}. Ctrl+C to exit.", flush=True)
try:
    last = []
    while True:
        pressed = hotkey.pressed_keys()
        if pressed != last:
            print("keys:", cfg.hotkey_label(pressed) or "-", flush=True)
            last = pressed
        time.sleep(0.05)
except KeyboardInterrupt:
    chord.stop()
