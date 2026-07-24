# Transcription Service

OpenAI-Whisper-API-compatible transcription (`POST /v1/audio/transcriptions`,
multipart WAV in, verbose_json out). One Rust service
(`src/main.rs`, on the [kiku](https://crates.io/crates/kiku) engine), baked
into the stateless bot image (`deployment/docker/Dockerfile.stateless`,
built with `--features cuda`) and serving both roles from that image:

- **Shared `kioku-whisper` compose service** — the bot image with
  `WHISPER_ONLY=true`. `STT_BACKEND=whisper` for local whisper.cpp on GPU
  (CPU fallback), or `openrouter` with `OPENROUTER_MODEL` for cloud STT.
  `chirp` (`google/chirp-3`) and `gpt4o`
  (`openai/gpt-4o-mini-transcribe`) remain as legacy aliases.
- **In-pod transcriber** — the same binary started next to the bot for
  deploys that can't reach a shared service (RunPod bots). ggml model named
  by `MODEL_SIZE`, auto-downloaded to the pod's `/app/models` cache. Local
  segments carry word-level timestamps (Teams speaker attribution).

## Run (Rust)

```bash
# cloud, needs OPENROUTER_API_KEY
STT_BACKEND=openrouter OPENROUTER_MODEL=x-ai/grok-stt-1.0 \
  OPENROUTER_API_KEY=sk-or-... cargo run --release

curl -X POST localhost:8000/v1/audio/transcriptions \
  -F file=@tests/test_audio.wav -F model=whisper-1
```

In the stack it deploys via the `shared-whisper` compose profile — see
`deployment/docker/docker-compose.stateful.yml`.

## Behavior notes

- Auth: `X-API-Key` or `Authorization: Bearer` against `API_TOKEN`
  (allow-all when unset).
- Quiet audio is RMS-gained to `CHIRP_TARGET_RMS` (0.1) before upload;
  cloud responses are split into sentence-shaped segments with pro-rated
  timestamps and labeled `CHIRP_LANGUAGE_LABEL` (default `en`).
- Fail-fast load management: 503 + `Retry-After` when busy
  (`MAX_ACTIVE_REQUESTS`, default 20), 60s upstream timeout
  (`OPENROUTER_TIMEOUT_S`).

## Response format

```json
{
  "text": "transcribed text",
  "language": "en",
  "language_probability": 0.0,
  "duration": 5.95,
  "segments": [
    {"id": 0, "seek": 0, "start": 0.0, "end": 4.05, "text": "...",
     "tokens": [], "temperature": 0.0, "audio_start": 0.0, "audio_end": 4.05}
  ]
}
```

Local (whisper) segments also carry a `words` array
(`{word, start, end, probability}`). Dropped from the python era: VAD
filtering, temperature fallback, and per-segment confidence fields
(faster-whisper features kiku's whisper.cpp backend doesn't expose) — the
bot's phrase-based hallucination filter still applies.

Known limitation (both backends): whisper hallucinates on silence (bug #24) —
mitigated bot-side in `core/src/services/hallucinations/`.

## License

Apache-2.0
