#!/usr/bin/env bash
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────
PG_MAJOR="${PG_MAJOR:-16}"
PG_BIN="/usr/lib/postgresql/${PG_MAJOR}/bin"
PGDATA="/data/postgresql"
DB_NAME="${DB_NAME:-kioku}"
DB_USER="${DB_USER:-kioku}"
DB_PASSWORD="${DB_PASSWORD:-kioku}"
REDIS_PASSWORD="${REDIS_PASSWORD:-kioku-redis}"
MINIO_ROOT_USER="${MINIO_ACCESS_KEY:-kioku}"
MINIO_ROOT_PASSWORD="${MINIO_SECRET_KEY:-kioku-minio-password}"
BOT_IMAGE="${BOT_IMAGE:-ghcr.io/kioku-org/kioku-stateless:latest}"

echo "[KIOKU] Preparing stateful runtime..."

# ─── Detect public IP (used for bot pod callbacks on RunPod) ──────────────────
PUBLIC_IP=$(curl -s --max-time 5 http://ifconfig.me 2>/dev/null || echo localhost)
echo "[KIOKU] Public IP: $PUBLIC_IP"

REDIS_LOCAL_URL="redis://:${REDIS_PASSWORD}@localhost:6379/0"
REDIS_BOT_URL="redis://:${REDIS_PASSWORD}@${PUBLIC_IP}:6379/0"
BOT_MEETING_API_URL="http://${PUBLIC_IP}:8080"
BOT_TTS_URL="http://${PUBLIC_IP}:8002"
BOT_COOKIE_URL="http://${PUBLIC_IP}:8099"

# ─── Prepare directories ──────────────────────────────────────────────────────
mkdir -p \
    /data/postgresql /data/qdrant /data/redis /data/minio /data/ollama/models \
    /data/cookie /data/recordings \
    /etc/qdrant /run/sshd /var/run/postgresql /root/.ssh /var/log/containers
chmod 700 /root/.ssh
chown postgres:postgres "$PGDATA" /var/run/postgresql 2>/dev/null || true
chown -R redis:redis /data/redis 2>/dev/null || true

# ─── SSH ──────────────────────────────────────────────────────────────────────
if [[ -n "${PUBLIC_KEY:-}" ]]; then
    printf '%s\n' "$PUBLIC_KEY" > /root/.ssh/authorized_keys
    chmod 600 /root/.ssh/authorized_keys
fi
sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config

# ─── PostgreSQL ───────────────────────────────────────────────────────────────
if [[ ! -f "$PGDATA/PG_VERSION" ]]; then
    echo "[KIOKU] Initializing PostgreSQL ${PG_MAJOR}..."
    sudo -u postgres "$PG_BIN/initdb" -D "$PGDATA"
    echo "listen_addresses='*'" >> "$PGDATA/postgresql.conf"
    echo "host all all 0.0.0.0/0 trust" >> "$PGDATA/pg_hba.conf"

    sudo -u postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -w start
    sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE USER ${DB_USER} WITH PASSWORD '${DB_PASSWORD}';" 2>/dev/null || true
    sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${DB_NAME} OWNER ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS vector;"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS hivemind AUTHORIZATION ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS vexa AUTHORIZATION ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "ALTER SCHEMA hivemind OWNER TO ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "ALTER SCHEMA vexa OWNER TO ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "GRANT ALL ON SCHEMA hivemind TO ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "GRANT ALL ON SCHEMA vexa TO ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "ALTER ROLE ${DB_USER} IN DATABASE ${DB_NAME} SET search_path TO hivemind,public;"
    sudo -u postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -m fast -w stop
fi

# ─── Redis ────────────────────────────────────────────────────────────────────
sed -i 's/^bind 127.0.0.1.*/bind 0.0.0.0/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's/^protected-mode yes/protected-mode no/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's|^dir .*|dir /data/redis|' /etc/redis/redis.conf 2>/dev/null || true
if grep -q '^requirepass ' /etc/redis/redis.conf 2>/dev/null; then
    sed -i "s/^requirepass .*/requirepass ${REDIS_PASSWORD}/" /etc/redis/redis.conf
else
    echo "requirepass ${REDIS_PASSWORD}" >> /etc/redis/redis.conf
fi
grep -q '^appendonly ' /etc/redis/redis.conf 2>/dev/null \
    && sed -i 's/^appendonly .*/appendonly yes/' /etc/redis/redis.conf \
    || echo 'appendonly yes' >> /etc/redis/redis.conf

# ─── Qdrant ───────────────────────────────────────────────────────────────────
cat > /etc/qdrant/config.yaml <<'QDRANT'
service:
  host: 0.0.0.0
  http_port: 6334
storage:
  storage_path: /data/qdrant
QDRANT

# ─── One-shot helper scripts ──────────────────────────────────────────────────
cat > /usr/local/bin/kioku-pull-models.sh <<'PULLSCRIPT'
#!/usr/bin/env bash
set -euo pipefail
echo "[KIOKU] Waiting for Ollama before pulling embedding model..."
for _ in $(seq 1 60); do
    if curl -fsS http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
        ollama pull nomic-embed-text-v2-moe
        exit 0
    fi
    sleep 5
done
echo "[KIOKU] Ollama did not become ready in time"
exit 1
PULLSCRIPT
chmod +x /usr/local/bin/kioku-pull-models.sh

cat > /usr/local/bin/kioku-init-minio.sh <<MINIO_INIT
#!/usr/bin/env bash
set -euo pipefail
echo "[KIOKU] Waiting for MinIO..."
for _ in \$(seq 1 60); do
    if mc alias set minio http://localhost:9000 "${MINIO_ROOT_USER}" "${MINIO_ROOT_PASSWORD}" >/dev/null 2>&1; then
        mc mb --ignore-existing "minio/${MINIO_BUCKET:-vexa-recordings}"
        echo "[KIOKU] MinIO bucket ready."
        exit 0
    fi
    sleep 5
done
echo "[KIOKU] MinIO did not become ready in time"
exit 1
MINIO_INIT
chmod +x /usr/local/bin/kioku-init-minio.sh

# ─── Supervisord config ───────────────────────────────────────────────────────
cat > /etc/supervisor/conf.d/kioku.conf <<SUPERVISOR
[supervisord]
nodaemon=true
logfile=/var/log/supervisord.log

# ── Infrastructure ────────────────────────────────────────────────────────────

[program:postgresql]
command=${PG_BIN}/postgres -D ${PGDATA}
user=postgres
autostart=true
autorestart=true
stdout_logfile=/var/log/postgres.log
stderr_logfile=/var/log/postgres.err

[program:redis]
command=/usr/bin/redis-server /etc/redis/redis.conf --daemonize no
autostart=true
autorestart=true
stdout_logfile=/var/log/redis.log
stderr_logfile=/var/log/redis.err

[program:qdrant]
command=/usr/local/bin/qdrant --config-path /etc/qdrant/config.yaml
autostart=true
autorestart=true
stdout_logfile=/var/log/qdrant.log
stderr_logfile=/var/log/qdrant.err

[program:minio]
command=/usr/local/bin/minio server /data/minio --address ":9000" --console-address ":9001"
environment=MINIO_ROOT_USER="${MINIO_ROOT_USER}",MINIO_ROOT_PASSWORD="${MINIO_ROOT_PASSWORD}"
autostart=true
autorestart=true
stdout_logfile=/var/log/minio.log
stderr_logfile=/var/log/minio.err

[program:ollama]
command=/usr/local/bin/ollama serve
environment=OLLAMA_HOST="0.0.0.0:11434",OLLAMA_MODELS="/data/ollama/models"
autostart=true
autorestart=true
stdout_logfile=/var/log/ollama.log
stderr_logfile=/var/log/ollama.err

[program:sshd]
command=/usr/sbin/sshd -D -e
autostart=true
autorestart=true
stdout_logfile=/var/log/sshd.log
stderr_logfile=/var/log/sshd.err

# ── One-shot init jobs ────────────────────────────────────────────────────────

[program:ollama-pull]
command=/usr/local/bin/kioku-pull-models.sh
autostart=true
autorestart=false
startsecs=0
stdout_logfile=/var/log/ollama-pull.log
stderr_logfile=/var/log/ollama-pull.err

[program:minio-init]
command=/usr/local/bin/kioku-init-minio.sh
autostart=true
autorestart=false
startsecs=0
stdout_logfile=/var/log/minio-init.log
stderr_logfile=/var/log/minio-init.err

# ── Vexa backends ─────────────────────────────────────────────────────────────

[program:api-gateway]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8056
directory=/opt/vexa/services/api-gateway
environment=ADMIN_API_URL="http://localhost:8001",MEETING_API_URL="http://localhost:8080",TRANSCRIPTION_COLLECTOR_URL="http://localhost:8080",MCP_URL="http://localhost:18888",AGENT_API_URL="http://localhost:8100",REDIS_URL="${REDIS_LOCAL_URL}",PUBLIC_BASE_URL="${VEXA_PUBLIC_URL:-http://localhost:8056}",TRANSCRIPT_SHARE_TTL_SECONDS="900",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",CORS_ORIGINS="${CORS_ORIGINS:-*}",LOG_LEVEL="${LOG_LEVEL:-INFO}",DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable"
autostart=true
autorestart=true
stdout_logfile=/var/log/api-gateway.log
stderr_logfile=/var/log/api-gateway.err

[program:admin-api]
command=/opt/venv/bin/uvicorn app.main:app --host 0.0.0.0 --port 8001
directory=/opt/vexa/services/admin-api
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable",DB_POOL_SIZE="5",DB_MAX_OVERFLOW="5",DB_POOL_TIMEOUT="30",ADMIN_API_TOKEN="${VEXA_ADMIN_API_TOKEN}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/admin-api.log
stderr_logfile=/var/log/admin-api.err

[program:meeting-api]
command=/opt/venv/bin/uvicorn meeting_api.main:app --host 0.0.0.0 --port 8080
directory=/opt/vexa/services/meeting-api
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable",DB_POOL_SIZE="20",DB_MAX_OVERFLOW="20",DB_POOL_TIMEOUT="10",REDIS_URL="${REDIS_LOCAL_URL}",REDIS_HOST="localhost",REDIS_PORT="6379",REDIS_STREAM_NAME="transcription_segments",REDIS_CONSUMER_GROUP="collector_group",REDIS_STREAM_READ_COUNT="10",REDIS_STREAM_BLOCK_MS="2000",TRANSCRIPTION_COLLECTOR_URL="http://localhost:8080",TRANSCRIPTION_SERVICE_URL="http://localhost:8000",REMOTE_TRANSCRIBER_URL="http://localhost:8000/v1/audio/transcriptions",REMOTE_TRANSCRIBER_API_KEY="${VEXA_TRANSCRIBER_API_KEY:-}",TTS_SERVICE_URL="${BOT_TTS_URL}",RUNTIME_API_URL="http://localhost:8090",MEETING_API_URL="http://localhost:8080",BOT_IMAGE_NAME="${BOT_IMAGE}",BOT_REDIS_URL="${REDIS_BOT_URL}",BOT_MEETING_API_URL="${BOT_MEETING_API_URL}",BOT_TTS_URL="${BOT_TTS_URL}",BOT_COOKIE_URL="${BOT_COOKIE_URL}",BOT_TRANSCRIPTION_SERVICE_URL="http://localhost:8000",ADMIN_TOKEN="${VEXA_ADMIN_API_TOKEN}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",CORS_ORIGINS="${CORS_ORIGINS:-*}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}",ZOOM_CLIENT_ID="${ZOOM_CLIENT_ID:-}",ZOOM_CLIENT_SECRET="${ZOOM_CLIENT_SECRET:-}",STORAGE_BACKEND="${STORAGE_BACKEND:-minio}",MINIO_ENDPOINT="localhost:9000",MINIO_ACCESS_KEY="${MINIO_ACCESS_KEY:-vexa-access-key}",MINIO_SECRET_KEY="${MINIO_SECRET_KEY:-vexa-secret-key}",MINIO_BUCKET="${MINIO_BUCKET:-vexa-recordings}",MINIO_SECURE="false",RECORDING_ENABLED="${RECORDING_ENABLED:-false}",CAPTURE_MODES="audio",COOKIE_STORAGE_BACKEND="http",COOKIE_SERVICE_URL="http://localhost:8099",COOKIE_SERVICE_TOKEN="${COOKIE_SERVICE_TOKEN:-}"
autostart=true
autorestart=true
stdout_logfile=/var/log/meeting-api.log
stderr_logfile=/var/log/meeting-api.err

[program:agent-api]
command=/opt/venv/bin/uvicorn agent_api.main:app --host 0.0.0.0 --port 8100
directory=/opt/vexa/services/agent-api
environment=REDIS_URL="${REDIS_LOCAL_URL}",RUNTIME_API_URL="http://localhost:8090",ADMIN_API_URL="http://localhost:8001",ADMIN_API_TOKEN="${VEXA_ADMIN_API_TOKEN:-}",AGENT_API_INTERNAL_URL="http://localhost:8100",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",CORS_ORIGINS="${CORS_ORIGINS:-*}",ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/agent-api.log
stderr_logfile=/var/log/agent-api.err

[program:tts]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8002
directory=/opt/vexa/services/tts-service
environment=TTS_API_TOKEN="${TTS_API_TOKEN:-}",OPENAI_API_KEY="${OPENAI_API_KEY:-}",OPENAI_BASE_URL="${OPENAI_BASE_URL:-https://api.openai.com}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}",PIPER_VOICES_DIR="/app/voices"
autostart=true
autorestart=true
stdout_logfile=/var/log/tts.log
stderr_logfile=/var/log/tts.err

[program:runtime-api-local]
command=/opt/venv/bin/uvicorn runtime_api.main:app --host 0.0.0.0 --port 8091
directory=/opt/vexa/services/runtime-api
environment=ORCHESTRATOR_BACKEND="docker",REDIS_URL="${REDIS_LOCAL_URL}",DOCKER_HOST="unix:///var/run/docker.sock",DOCKER_NETWORK="kioku-network",BROWSER_IMAGE="${BOT_IMAGE}",TRANSCRIPTION_SERVICE_URL="http://localhost:8000",TTS_SERVICE_URL="${BOT_TTS_URL}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",PROFILES_PATH="/app/profiles.yaml",LOG_LEVEL="${LOG_LEVEL:-INFO}",VEXA_ENV="${VEXA_ENV:-production}",HOST="0.0.0.0",PORT="8091",BOT_MODEL_CACHE_DIR="${BOT_MODEL_CACHE_DIR:-}",BOT_WHISPER_MODEL="${BOT_WHISPER_MODEL:-}"
autostart=true
autorestart=true
stdout_logfile=/var/log/runtime-api-local.log
stderr_logfile=/var/log/runtime-api-local.err

[program:runtime-api-runpod]
command=/opt/venv/bin/uvicorn runtime_api.main:app --host 0.0.0.0 --port 8092
directory=/opt/vexa/services/runtime-api
environment=ORCHESTRATOR_BACKEND="runpod",REDIS_URL="${REDIS_LOCAL_URL}",RUNPOD_ACCOUNT_API_KEY="${RUNPOD_API_KEY:-}",RUNPOD_GPU_TYPES="${RUNPOD_GPU_TYPES:-NVIDIA GeForce RTX 3090,NVIDIA RTX A5000,NVIDIA RTX A4000}",RUNPOD_CLOUD_TYPE="${RUNPOD_CLOUD_TYPE:-COMMUNITY}",TRANSCRIPTION_SERVICE_URL="http://localhost:8000",TTS_SERVICE_URL="${BOT_TTS_URL}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",PROFILES_PATH="/app/profiles.yaml",LOG_LEVEL="${LOG_LEVEL:-INFO}",VEXA_ENV="${VEXA_ENV:-production}",HOST="0.0.0.0",PORT="8092"
autostart=$([ -n "${RUNPOD_API_KEY:-}" ] && echo true || echo false)
autorestart=true
stdout_logfile=/var/log/runtime-api-runpod.log
stderr_logfile=/var/log/runtime-api-runpod.err

# ── Kioku-owned services ──────────────────────────────────────────────────────

[program:hivemind]
command=/usr/local/bin/kioku-hivemind
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_MAX_CONNECTIONS="10",DB_SCHEMA="hivemind",JWT_SECRET="${HIVEMIND_JWT_SECRET}",JWT_TTL_SECONDS="2592000",ENCRYPTION_SECRET="${HIVEMIND_ENCRYPTION_SECRET}",VEXA_API_URL="http://localhost:8056",VEXA_ADMIN_API_URL="http://localhost:8001",VEXA_ADMIN_TOKEN="${VEXA_ADMIN_API_TOKEN}",HOST="0.0.0.0",PORT="9100",EMBEDDING_API_URL="http://localhost:11434",EMBEDDING_MODEL="nomic-embed-text-v2-moe",QDRANT_URL="http://localhost:6334",QDRANT_API_KEY="${QDRANT_API_KEY:-}"
autostart=true
autorestart=true
stdout_logfile=/var/log/hivemind.log
stderr_logfile=/var/log/hivemind.err

[program:mcp]
command=/opt/venv/bin/python main.py
directory=/opt/vexa/services/mcp
environment=KIOKU_API_URL="http://localhost:8056",KIOKU_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/mcp.log
stderr_logfile=/var/log/mcp.err

[program:router]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8090
directory=/opt/vexa/services/router
environment=USE_LOCAL_RESOURCE="${USE_LOCAL_RESOURCE:-true}",LOCAL_BOT_THRESHOLD="${LOCAL_BOT_THRESHOLD:-3}",LOCAL_BACKEND_URL="http://localhost:8091",RUNPOD_BACKEND_URL="http://localhost:8092"
autostart=true
autorestart=true
stdout_logfile=/var/log/router.log
stderr_logfile=/var/log/router.err

[program:cookie]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8099
directory=/opt/vexa/services/cookie
environment=COOKIE_SERVICE_TOKEN="${COOKIE_SERVICE_TOKEN:-}",DATA_DIR="/data/cookie"
autostart=true
autorestart=true
stdout_logfile=/var/log/cookie.log
stderr_logfile=/var/log/cookie.err

[program:dashboard]
command=node server.js
directory=/opt/dashboard
environment=NODE_ENV="production",PORT="3001",HOSTNAME="0.0.0.0",VEXA_API_URL="http://localhost:8056",VEXA_ADMIN_API_KEY="${VEXA_ADMIN_API_TOKEN:-}",VEXA_ADMIN_API_URL="http://localhost:8001",VEXA_ALLOW_DIRECT_LOGIN="${VEXA_ALLOW_DIRECT_LOGIN:-true}",NEXTAUTH_URL="${NEXTAUTH_URL:-https://dashboard.kioku.chat}",NEXTAUTH_SECRET="${NEXTAUTH_SECRET:-}",GOOGLE_CLIENT_ID="${GOOGLE_CLIENT_ID:-}",GOOGLE_CLIENT_SECRET="${GOOGLE_CLIENT_SECRET:-}",AZURE_AD_CLIENT_ID="${AZURE_AD_CLIENT_ID:-}",AZURE_AD_CLIENT_SECRET="${AZURE_AD_CLIENT_SECRET:-}",AZURE_AD_TENANT_ID="${AZURE_AD_TENANT_ID:-}",SMTP_HOST="${SMTP_HOST:-}",SMTP_USER="${SMTP_USER:-}",SMTP_PASS="${SMTP_PASS:-}",NEXT_PUBLIC_DOCS_URL="${NEXT_PUBLIC_DOCS_URL:-https://docs.kioku.chat}"
autostart=true
autorestart=true
stdout_logfile=/var/log/dashboard.log
stderr_logfile=/var/log/dashboard.err

SUPERVISOR

# ── Optional: Cloudflare Tunnel ───────────────────────────────────────────────
if [[ -f /etc/cloudflared/config.yml ]]; then
    cat >> /etc/supervisor/conf.d/kioku.conf <<'CLOUDFLARED'

[program:cloudflared]
command=/usr/local/bin/cloudflared tunnel --config /etc/cloudflared/config.yml run
autostart=true
autorestart=true
stdout_logfile=/var/log/cloudflared.log
stderr_logfile=/var/log/cloudflared.err
CLOUDFLARED
    echo "[KIOKU] Cloudflared config found, enabling tunnel"
fi

# ─── Start ────────────────────────────────────────────────────────────────────
echo "[KIOKU] Starting stateful pod..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/kioku.conf
