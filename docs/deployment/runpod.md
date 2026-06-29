---
title: "RunPod"
---
Deploy Kioku on RunPod with a two-pod architecture: always-on CPU stateful pod + ephemeral GPU bot pods.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│         kioku-stateful pod (CPU, always-on)            │
│  supervisord                                           │
│  ├── postgres    ├── qdrant    ├── redis              │
│  ├── ollama      ├── minio     ├── hivemind           │
│  ├── vexa api-gateway  ├── vexa-meeting-api            │
│  ├── vexa-admin-api   ├── vexa-agent-api              │
│  ├── vexa-mcp          ├── vexa-tts-service            │
│  └── runtime-api (ORCHESTRATOR_BACKEND=runpod)        │
│  Exposed: 22, 6379, 8080, 8090, 9100, 8056, 11434     │
└────────────┬──────────────────────────────────────────┘
             │ RunPod REST API (create/stop pod)
             ▼
┌──────────────────────────────────┐
│   kioku-stateless pod (GPU)       │
│  ├── Whisper transcription (GPU)  │
│  └── Vexa bot (Playwright)        │
│  Lives ~1 meeting, then exits     │
└──────────────────────────────────┘
```

## Images

| Image | Registry | Pod Type |
|---|---|---|
| `kyomoto/kioku-stateful:latest` | Docker Hub | CPU, always-on |
| `kyomoto/kioku-stateless:latest` | Docker Hub | GPU, ephemeral |

Images are built automatically by GitHub Actions on push to master.
The RunPod validation workflow can also be dispatched manually for a specific
published image SHA when you want to re-run the end-to-end pod test without
waiting on a new push.

## Deploy

```bash
cd deployment/runpod
cp .env.example .env
# Fill in RUNPOD_API_KEY, secrets, domain
$EDITOR .env

./deploy.sh
```


The script creates a CPU pod with all stateful services via `runpodctl pod create --compute-type cpu`.

The deployed stateful pod should receive:

- `STATEFUL_RUNPOD_CLOUD_TYPE=COMMUNITY` so the CPU pod gets a public IP when using `runpodctl pod create`
- `RUNPOD_ACCOUNT_API_KEY` for runtime-api orchestration
- `BROWSER_IMAGE` / `BOT_IMAGE` pointing at `kyomoto/kioku-stateless:latest`
- `RUNPOD_GPU_TYPES` as an ordered fallback list for stateless GPU allocation

## Security

- **Redis** — AUTH password required (`REDIS_PASSWORD`), port 6379 exposed publicly
- **Postgres** — NOT exposed publicly (internal only)
- **MinIO/Qdrant** — NOT exposed publicly (internal only)
- **meeting-api** — Uses `INTERNAL_API_SECRET` for internal callbacks
- **Cloudflared** — Optional, tunnels public traffic to hivemind/vexa-gateway

## Cost

| Resource | Rate | Monthly (24/7) |
|---|---|---|
| Stateful CPU pod | ~$0.10-0.20/hr | ~$72-144 |
| Bot GPU pod (per meeting) | ~$0.27-0.46/hr | per-meeting |
| Container disk (20GB) | $0.10/GB/mo | ~$2 |

Bot pods only cost money while a meeting is in progress. A 1-hour meeting costs ~$0.27-0.46 in GPU compute.

## Bot Pod Lifecycle

1. **Spawn**: `POST /vexa/bots` → runtime-api calls RunPod REST API → GPU pod created
2. **Boot**: Pod pulls image (~30-60s), starts Whisper + bot
3. **Meeting**: Bot joins Google Meet/Zoom/Teams, transcribes
4. **Exit**: Bot exits → reaper detects (15s poll) → pod deleted

<Note>
  Bot pod startup latency is ~30-60s vs ~2s for Docker Compose. Plan accordingly for time-sensitive meetings.
</Note>

## CI Validation

The GitHub Actions `RunPod Integration Test` workflow validates the published
RunPod images by:

1. Waiting for the stateful and stateless Docker Hub images for a target SHA
2. Starting a stateful CPU pod on RunPod
3. Verifying service health, including the Ollama embedding endpoint on `11434`
4. Running the Hivemind and CLI integration suites against the live pod
5. Spawning and deleting a stateless bot pod via `runtime-api`

To run that workflow manually for a specific published SHA:

```bash
gh workflow run "RunPod Integration Test" -f image_sha=<short-or-full-git-sha>
```
