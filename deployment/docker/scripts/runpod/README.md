# RunPod deploy

Deploys the always-on Kioku services on RunPod using `runpodctl`. Meeting bots
are spawned automatically by runtime-api when a meeting is requested.

## CPU production with OpenRouter

Create a startup template from the checked-out deployment files:

```bash
python3 create_cpu_template.py
```

Put the returned `TEMPLATE_ID` in the ignored `.env` file, alongside the required
application secrets and RunPod/OpenRouter account keys. Configure:

```dotenv
STATEFUL_COMPUTE_TYPE=CPU
STATEFUL_RUNPOD_CLOUD_TYPE=SECURE
BOT_COMPUTE_TYPE=CPU
STT_BACKEND=openrouter
OPENROUTER_MODEL=x-ai/grok-stt-1.0
EMBEDDING_BASE_MODEL=nomic-embed-text
EMBEDDING_MODEL=nomic-embed-text-cpu
CPU_EMBEDDING_THREADS=2
USE_LOCAL_RESOURCE=false
MIN_BOT_POOL=0
VEXA_ALLOW_DIRECT_LOGIN=false
```

The template installs the current entrypoint and runtime profiles at each start.
CPU bot profiles pass the OpenRouter key and model to the in-pod transcription
service. Embedding inference uses two CPU threads. No local Whisper model or GPU
bot pool is needed.

Keep unverified email login disabled in production. Admin access at `/admin/users`
uses `VEXA_ADMIN_API_TOKEN` from the local `.env`. Configure OAuth or SMTP before
enabling ordinary user sign-in.

To deploy a rebuilt dashboard independently of the published image, store its
Next.js standalone output (including `public` and `.next/static`) on the network
volume and set `DASHBOARD_RELEASE_DIR` to that directory. For an existing pod
whose template still uses `/opt/dashboard`, copy `dashboard-release.sh` to the
volume and set `BASH_ENV` to its absolute path. The startup hook selects that
release before starting the services and preserves it across container resets.
When replacing an older dashboard, use a fresh query parameter on the initial
admin link (for example `/admin/users?admin_login=1`) to bypass HTML that the
RunPod proxy already cached. The updated admin layout disables future caching.

For durable production data, create a network volume and add its ID and location
to `.env`:

```bash
runpodctl network-volume create --name kioku-prod-data --size 50 --data-center-id US-CA-2
```

```dotenv
NETWORK_VOLUME_ID=<returned-id>
STATEFUL_DATA_CENTER_ID=US-CA-2
VOLUME_GB=0
PGDATA=/var/lib/kioku/postgresql
POSTGRES_BACKUP_DIR=/data/postgresql-backups
POSTGRES_BACKUP_INTERVAL_SECONDS=60
```

The network volume mounts at `/data` and retains Qdrant, Redis, MinIO, Ollama,
and PostgreSQL backups independently of the container. RunPod network storage
may reject the ownership and permissions required by PostgreSQL, so this setup
keeps its live data on the container disk and publishes a consistent compressed
dump every 60 seconds. Startup restores the latest completed dump after
container replacement. An abrupt failure can lose writes since the last
completed backup; use a managed PostgreSQL service when that recovery gap is
unacceptable. Verify the mount and restore before serving production traffic.
A CPU pod may report `volumeInGb=0` even with a network volume.

Then deploy:

```bash
./deploy.sh
runpodctl pod get <pod-id>
```

The startup script derives dashboard and API URLs from the assigned pod ID.
Override `NEXTAUTH_URL`, `VEXA_PUBLIC_URL`, and `VEXA_PUBLIC_API_URL` when using
a custom domain. Recreate the startup template when changing its source files.

The published stateful and stateless GHCR images are publicly pullable. Deployment
output prints pod metadata and omits environment variables containing secrets.
