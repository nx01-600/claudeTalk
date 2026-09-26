"""Same corpus through the v0.5 engine (faster-whisper large-v3-turbo fp16 CUDA)."""
import sys, time, wave, re, unicodedata
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "voice-input"))
import stt  # sets up the CUDA DLL paths like the daemon

PROMPT = "Dictado en espanol para Claude Code: commit, repositorio, hook, pull request, branch, terminal, script, Elementor, Rails, TypeScript, Docker, WordPress."

def words(s):
    s = "".join(c for c in unicodedata.normalize("NFD", s.lower()) if not unicodedata.combining(c))
    return re.sub(r"[^\w]+", " ", s).split()

def edits(a, b):
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a):
        cur = [i + 1]
        for j, y in enumerate(b):
            cur.append(min(prev[j] + (x != y), prev[j + 1] + 1, cur[j] + 1))
        prev = cur
    return prev[-1]

from faster_whisper import WhisperModel
corpus = Path(sys.argv[1])
t = time.time(); m = WhisperModel("large-v3-turbo", device="cuda", compute_type=sys.argv[2] if len(sys.argv) > 2 else "float16"); print(f"load {time.time()-t:.2f}s")
refs = (corpus / "sentences.txt").read_text(encoding="utf-8").splitlines()
E = N = T = A = 0
for i, ref in enumerate(refs):
    with wave.open(str(corpus / f"{i:02}.wav")) as w:
        pcm = np.frombuffer(w.readframes(w.getnframes()), np.int16).astype(np.float32) / 32768
    t = time.time()
    segs, _ = m.transcribe(pcm, language="es", beam_size=1, vad_filter=True, initial_prompt=PROMPT)
    text = "".join(s.text for s in segs)
    dt = time.time() - t
    e = edits(words(ref), words(text)); E += e; N += len(words(ref)); T += dt; A += len(pcm) / 16000
    print(f"{i:02} {dt*1000:.0f}ms e={e} | {text.strip()}")
print(f"WER {100*E/N:.2f}% ({E}/{N}) | total {T:.2f}s for {A:.1f}s audio (RTF {T/A:.3f})")
