# Transcription Service

OpenAI-Whisper-API-compatible transcription (`POST /v1/audio/transcriptions`,
multipart WAV in, verbose_json out). Two implementations live here:

- **Rust (`src/main.rs`, `Dockerfile.kiku`)** — the shared `kioku-whisper`
  compose service, built on the [kiku](https://crates.io/crates/kiku) engine.
  Cloud STT via OpenRouter (`STT_BACKEND=chirp` → `google/chirp-3`, `gpt4o` →
  `openai/gpt-4o-mini-transcribe`); local whisper.cpp (CPU) behind the
  `local-whisper` cargo feature. No GPU needed.
- **Python (`main.py`, `chirp.py`)** — in-pod GPU faster-whisper, shipped into
  stateless bot pods by `deployment/docker/Dockerfile.stateless` for deploys
  that can't reach a shared service (RunPod bots).

## Run (Rust)

```bash
# cloud, needs OPENROUTER_API_KEY
STT_BACKEND=chirp OPENROUTER_API_KEY=sk-or-... cargo run --release

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

The python in-pod service additionally supports faster-whisper tuning
(VAD knobs, temperature fallback, `timestamp_granularities=word` for
word-level timing used in Teams speaker attribution) — see `.env.example`
and `docs/models.md`. Its test suite lives in `tests/` (`pytest tests/ -v`).

Known limitation (both backends): whisper hallucinates on silence (bug #24) —
mitigated bot-side in `core/src/services/hallucinations/`.

## License

Apache-2.0
