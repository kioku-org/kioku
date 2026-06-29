---
title: "GPU vs CPU Modes"
description: "Choose the right hardware configuration for your Kioku deployment."
---

Kioku has two GPU-dependent subsystems, each independently configurable:

| Subsystem | GPU benefit | CPU fallback |
|---|---|---|
| **Ollama embeddings** | 5–20ms per embed | 50–200ms per embed |
| **Whisper transcription** | Real-time at 1.5 GB VRAM | Usable but lags on long audio |

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
              count: 1
              capabilities: [gpu]
```

**CPU fallback**: Remove the `deploy.resources` block. Embeddings still work, just slower.  
Acceptable for low-volume deployments (< 50 knowledge searches/day).

## Bot Containers (Whisper)

Each `kioku-stateless` bot container runs faster-whisper for live transcription.

**GPU**: Bot containers automatically use CUDA if the host has an NVIDIA GPU and `nvidia-container-toolkit` is installed. No config needed.

**CPU**: Set `COMPUTE_TYPE=int8` (already the default) and ensure enough CPU cores. A modern 8-core CPU can handle ~1–2 concurrent bots in real time.

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
COMPUTE_TYPE=int8
```

### CPU-only machine

```bash
USE_LOCAL_RESOURCE=true
LOCAL_BOT_THRESHOLD=1         # one bot at a time on CPU
BOT_WHISPER_MODEL=medium      # faster on CPU
COMPUTE_TYPE=int8
```

### GPU machine + RunPod overflow

```bash
USE_LOCAL_RESOURCE=true
LOCAL_BOT_THRESHOLD=4         # first 4 bots use local GPU
RUNPOD_API_KEY=your_key       # 5th+ overflow to RunPod
BOT_WHISPER_MODEL=large-v3-turbo
COMPUTE_TYPE=int8
```

## Model Cache

Whisper models are downloaded on first bot startup and cached in the `kioku-whisper-model` Docker volume. All subsequent bot containers reuse the cached model — no re-download per meeting.

Ollama models are cached in `kioku-ollama-data` and pulled once on first startup.
