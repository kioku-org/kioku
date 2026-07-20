---
title: "GPU vs CPU Modes"
description: "Choose the right hardware configuration for your Kioku deployment."
---

Kioku has two GPU-dependent subsystems, each independently configurable:

| Subsystem | GPU benefit | CPU fallback |
|---|---|---|
| **Ollama embeddings** | 5–20ms per embed | 50–200ms per embed |
| **Local transcription (whisper.cpp)** | Real-time at 1.5 GB VRAM | Usable but lags on long audio |

## Stateful Services (Ollama)

Ollama runs inside `kioku-stateful` for embedding documents and meeting transcripts.

**GPU** (recommended): Pass the GPU to the container:
```yaml
# docker-compose.stateful.yml
services:
  kioku-stateful:
    deploy:
      resources:
        reservations:
          devices:
            - driver: nvidia
              count: all
              capabilities: [gpu]
```

**CPU fallback**: use the provided override, which drops the GPU device reservations for
both `kioku-stateful` and the optional `kioku-whisper` shared service:
```bash
docker compose -f docker-compose.stateful.yml -f docker-compose.cpu.yml up -d
```
Embeddings still work, just slower. Acceptable for low-volume deployments (< 50 knowledge
searches/day).

## Bot Containers (Transcription)

Each `kioku-stateless` bot container runs a transcription service (Rust, on the
[kiku](https://crates.io/crates/kiku)/whisper.cpp engine) for live transcription. It has
two backend modes, set via `STT_BACKEND`:

- **`whisper`** (default) — local whisper.cpp, needs a GPU or CPU cycles as below.
- **`chirp`** / **`gpt4o`** — cloud STT (Google Chirp 3 / OpenAI gpt-4o-mini-transcribe)
  via OpenRouter. No GPU or local model at all — trades VRAM/CPU for an
  `OPENROUTER_API_KEY` and sending audio off your infrastructure (see
  [Data flow](/security/data-flow)). Everything below this point applies only to the
  local `whisper` backend.

**GPU**: Bot containers automatically use CUDA if the host has an NVIDIA GPU and `nvidia-container-toolkit` is installed. No config needed.

**CPU**: whisper.cpp runs on CPU automatically when no GPU is available (or when `libcuda` isn't found). A modern 8-core CPU can handle ~1–2 concurrent bots in real time.

### Whisper model vs. hardware

| Model | VRAM (int8) | WER | Relative speed (GPU) |
|---|---|---|---|
| `large-v3-turbo` | ~1.5 GB | Best | 8× realtime |
| `large-v3` | ~3.0 GB | Best (marginally) | 4× realtime |
| `medium` | ~1.0 GB | Good | 12× realtime |
| `small` | ~0.5 GB | Fair | 20× realtime |

Set via `BOT_WHISPER_MODEL` env var. `large-v3-turbo` is the default and recommended choice — nearly identical accuracy to `large-v3` at half the VRAM.

## Recommended Configurations

### Single GPU machine (8 GB VRAM)

```bash
# .env
USE_LOCAL_RESOURCE=true
LOCAL_BOT_THRESHOLD=4         # 4 × 1.5 GB = 6 GB VRAM for bots, 2 GB for Ollama
BOT_WHISPER_MODEL=large-v3-turbo
```

### CPU-only machine

```bash
USE_LOCAL_RESOURCE=true
LOCAL_BOT_THRESHOLD=1         # one bot at a time on local CPU whisper.cpp
BOT_WHISPER_MODEL=medium      # faster on CPU
# or skip local CPU transcription entirely:
# STT_BACKEND=chirp; OPENROUTER_API_KEY=sk-or-...
```

### GPU machine + RunPod overflow

```bash
USE_LOCAL_RESOURCE=true
LOCAL_BOT_THRESHOLD=4         # first 4 bots use local GPU
RUNPOD_API_KEY=your_key       # 5th+ overflow to RunPod
BOT_WHISPER_MODEL=large-v3-turbo
```

## Model Cache

whisper.cpp models are downloaded on first bot startup and cached in the `kioku-whisper-models` Docker volume. All subsequent bot containers reuse the cached model — no re-download per meeting. Not applicable when `STT_BACKEND=chirp`/`gpt4o` (no local model).

Ollama models are cached in `kioku-ollama-data` and pulled once on first startup.
