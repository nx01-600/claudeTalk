import os
import time
import sys

nvidia_base = os.path.join(os.path.dirname(sys.executable), "..", "Lib", "site-packages", "nvidia")
nvidia_base = os.path.abspath(nvidia_base)
dll_dirs = []
for pkg in ("cudnn", "cublas"):
    bin_dir = os.path.join(nvidia_base, pkg, "bin")
    if os.path.isdir(bin_dir):
        os.add_dll_directory(bin_dir)
        dll_dirs.append(bin_dir)
os.environ["PATH"] = os.pathsep.join(dll_dirs) + os.pathsep + os.environ["PATH"]

import sounddevice as sd
import numpy as np
from faster_whisper import WhisperModel

SR = 16000
DURATION = 6

print(f"Grabando {DURATION}s... habla ahora en espanol.")
audio = sd.rec(int(DURATION * SR), samplerate=SR, channels=1, dtype="float32")
sd.wait()
print("Grabacion terminada. Cargando modelo...")

t0 = time.time()
model = WhisperModel("large-v3-turbo", device="cuda", compute_type="float16")
t1 = time.time()
print(f"Modelo cargado en {t1-t0:.2f}s")

audio_flat = audio.flatten()
t2 = time.time()
segments, info = model.transcribe(audio_flat, language="es", beam_size=1)
text = "".join(s.text for s in segments)
t3 = time.time()

print(f"Transcripcion: {text!r}")
print(f"Latencia transcripcion (beam_size=1): {t3-t2:.2f}s")
print(f"Idioma detectado: {info.language} (prob {info.language_probability:.2f})")

import subprocess
print(subprocess.run(["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv"], capture_output=True, text=True).stdout)
print("PID actual:", os.getpid())
