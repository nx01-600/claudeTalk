"""Builds corpus/NN.wav from sentences.txt: Edge TTS (edge-tts CLI) with five
voices, decoded to 16 kHz mono with PyAV, plus light noise.
Run from this folder with a Python that has PyAV (the dictation venv) and
edge-tts on PATH."""
import glob, subprocess, wave
import av
import numpy as np

VOICES = ["es-CO-GonzaloNeural", "es-CO-SalomeNeural", "es-MX-JorgeNeural", "es-MX-DaliaNeural", "es-US-AlonsoNeural"]
for i, line in enumerate(open("sentences.txt", encoding="utf-8").read().splitlines()):
    subprocess.run(["edge-tts", "--voice", VOICES[i % 5], "--text", line, "--write-media", f"{i:02}.mp3"], check=True)
for mp3 in sorted(glob.glob("*.mp3")):
    c = av.open(mp3)
    r = av.AudioResampler(format="s16", layout="mono", rate=16000)
    pcm = [o.to_ndarray().reshape(-1) for f in c.decode(audio=0) for o in r.resample(f)]
    pcm += [o.to_ndarray().reshape(-1) for o in r.resample(None)]
    a = np.concatenate(pcm).astype(np.int16)
    rng = np.random.default_rng(len(a))  # a little room noise, not studio-clean
    a = np.clip(a + rng.normal(0, 120, a.shape), -32768, 32767).astype(np.int16)
    with wave.open(mp3[:-4] + ".wav", "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(16000)
        w.writeframes(a.tobytes())
    c.close()
