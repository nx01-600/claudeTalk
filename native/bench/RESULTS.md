# STT benchmark (phase C), 2026-09-26

Corpus: 20 Spanish dictation-style sentences with tech terms
(`corpus/sentences.txt`), 90.5 s of audio. The audio is synthesized with
five Edge voices (CO, MX, US) plus light noise by `corpus/to_wav.py`. The WAV
files are not committed. Every engine gets the same `initial_prompt` as the
daemon, greedy decoding and `language=es`. The machine is an RTX 5070 Ti
Laptop GPU.

| Engine | WER | Time per sentence (warm) | Peak VRAM |
|---|---|---|---|
| faster-whisper large-v3-turbo fp16, CUDA (v0.5 daemon) | 5.86 % | ~270 ms | ~2.1 GB |
| whisper-rs 0.16 large-v3-turbo **q8_0**, Vulkan | **5.44 %** | **~250 ms** | **1.38 GB** |
| whisper-rs 0.16 large-v3-turbo fp16, Vulkan | 5.44 % | ~245 ms | 2.10 GB |

**Decision: large-v3-turbo q8_0 on whisper.cpp with Vulkan.**
- It makes the same or fewer errors as today; the q8_0 and fp16 transcripts are identical word for word.
- It uses 35 % less VRAM.
- No CUDA runtime or wheels are needed.

The first use on a machine compiles the Vulkan pipelines, which took about
17 s once. The GPU driver caches them after that, so the daemon should warm up
at start (as v0.5 already does). The CUDA backend was not benchmarked: Vulkan
already beats the current engine, and CUDA would bring back the toolkit and
DLL problems.

Reproduce:

```
python corpus\to_wav.py        (needs edge-tts and PyAV; see the script)
native\build.cmd bench cargo build --release
C:\ctb\release\ct-bench.exe %LOCALAPPDATA%\claudeTalk\models\ggml-large-v3-turbo-q8_0.bin corpus
voice-input\.venv\Scripts\python native\bench\baseline.py native\bench\corpus
```
