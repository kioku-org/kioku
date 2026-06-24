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
BOT_IMAGE="${BOT_IMAGE:-kyomoto/kioku-stateless:latest}"

echo "[KIOKU] Preparing stateful runtime..."

# ─── Detect public IP ─────────────────────────────────────────────────────────
PUBLIC_IP=$(curl -s --max-time 5 http://ifconfig.me 2>/dev/null || echo localhost)
echo "[KIOKU] Public IP: $PUBLIC_IP"

REDIS_LOCAL_URL="redis://:${REDIS_PASSWORD}@localhost:6379/0"
REDIS_BOT_URL="redis://:${REDIS_PASSWORD}@${PUBLIC_IP}:6379/0"
BOT_MEETING_API_URL="http://${PUBLIC_IP}:8080"
BOT_TTS_URL="http://${PUBLIC_IP}:8002"

# ─── Prepare directories ──────────────────────────────────────────────────────
mkdir -p \
    /data/postgresql /data/qdrant /data/redis /data/minio /data/ollama/models \
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
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS hivemind;"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS vexa;"
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

# ─── Ollama pull script ───────────────────────────────────────────────────────
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

# ─── Supervisord config ───────────────────────────────────────────────────────
cat > /etc/supervisor/conf.d/kioku.conf <<SUPERVISOR
[supervisord]
nodaemon=true
logfile=/var/log/supervisord.log

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

[program:sshd]
command=/usr/sbin/sshd -D -e
autostart=true
autorestart=true
stdout_logfile=/var/log/sshd.log
stderr_logfile=/var/log/sshd.err

[program:qdrant]
command=/usr/local/bin/qdrant --config-path /etc/qdrant/config.yaml
autostart=true
autorestart=true
stdout_logfile=/var/log/qdrant.log
stderr_logfile=/var/log/qdrant.err

[program:ollama]
command=/usr/local/bin/ollama serve
environment=OLLAMA_HOST="0.0.0.0:11434",OLLAMA_MODELS="/data/ollama/models"
autostart=true
autorestart=true
stdout_logfile=/var/log/ollama.log
stderr_logfile=/var/log/ollama.err

[program:minio]
command=/usr/local/bin/minio server /data/minio --address ":9000" --console-address ":9001"
environment=MINIO_ROOT_USER="${MINIO_ROOT_USER}",MINIO_ROOT_PASSWORD="${MINIO_ROOT_PASSWORD}"
autostart=true
autorestart=true
stdout_logfile=/var/log/minio.log
stderr_logfile=/var/log/minio.err

[program:ollama_pull]
command=/usr/local/bin/kioku-pull-models.sh
autostart=true
autorestart=false
startsecs=0
stdout_logfile=/var/log/ollama_pull.log
stderr_logfile=/var/log/ollama_pull.err

[program:hivemind]
command=/usr/local/bin/kioku-hivemind
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_MAX_CONNECTIONS="10",DB_SCHEMA="hivemind",JWT_SECRET="${HIVEMIND_JWT_SECRET}",JWT_TTL_SECONDS="2592000",ENCRYPTION_SECRET="${HIVEMIND_ENCRYPTION_SECRET}",VEXA_API_URL="http://localhost:8056",VEXA_ADMIN_API_URL="http://localhost:8001",VEXA_ADMIN_TOKEN="${VEXA_ADMIN_API_TOKEN}",HOST="0.0.0.0",PORT="9100",EMBEDDING_API_URL="http://localhost:11434",EMBEDDING_MODEL="nomic-embed-text-v2-moe",QDRANT_URL="http://localhost:6334",QDRANT_API_KEY="${QDRANT_API_KEY:-}"
autostart=true
autorestart=true
stdout_logfile=/var/log/hivemind.log
stderr_logfile=/var/log/hivemind.err

[program:vexa-api-gateway]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8056
directory=/opt/vexa/services/api-gateway
environment=ADMIN_API_URL="http://localhost:8001",MEETING_API_URL="http://localhost:8080",TRANSCRIPTION_COLLECTOR_URL="http://localhost:8080",MCP_URL="http://localhost:18888",AGENT_API_URL="http://localhost:8100",REDIS_URL="${REDIS_LOCAL_URL}",PUBLIC_BASE_URL="${VEXA_PUBLIC_URL:-http://localhost:8056}",TRANSCRIPT_SHARE_TTL_SECONDS="900",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",CORS_ORIGINS="${CORS_ORIGINS:-*}",LOG_LEVEL="${LOG_LEVEL:-INFO}",DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-api-gateway.log
stderr_logfile=/var/log/vexa-api-gateway.err

[program:vexa-admin-api]
command=/opt/venv/bin/uvicorn app.main:app --host 0.0.0.0 --port 8001
directory=/opt/vexa/services/admin-api
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable",DB_POOL_SIZE="5",DB_MAX_OVERFLOW="5",DB_POOL_TIMEOUT="30",ADMIN_API_TOKEN="${VEXA_ADMIN_API_TOKEN}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-admin-api.log
stderr_logfile=/var/log/vexa-admin-api.err

[program:vexa-meeting-api]
command=/opt/venv/bin/uvicorn meeting_api.main:app --host 0.0.0.0 --port 8080
directory=/opt/vexa/services/meeting-api
environment=DB_HOST="localhost",DB_PORT="5432",DB_NAME="${DB_NAME}",DB_USER="${DB_USER}",DB_PASSWORD="${DB_PASSWORD}",DB_SCHEMA="vexa",DB_SSL_MODE="disable",DB_POOL_SIZE="20",DB_MAX_OVERFLOW="20",DB_POOL_TIMEOUT="10",REDIS_URL="${REDIS_LOCAL_URL}",REDIS_HOST="localhost",REDIS_PORT="6379",REDIS_STREAM_NAME="transcription_segments",REDIS_CONSUMER_GROUP="collector_group",REDIS_STREAM_READ_COUNT="10",REDIS_STREAM_BLOCK_MS="2000",TRANSCRIPTION_COLLECTOR_URL="http://localhost:8080",TRANSCRIPTION_SERVICE_URL="http://localhost:80",REMOTE_TRANSCRIBER_URL="http://localhost:80/v1/audio/transcriptions",REMOTE_TRANSCRIBER_API_KEY="${VEXA_TRANSCRIBER_API_KEY:-}",TTS_SERVICE_URL="${BOT_TTS_URL}",RUNTIME_API_URL="http://localhost:8090",MEETING_API_URL="http://localhost:8080",BOT_IMAGE_NAME="${BOT_IMAGE}",BOT_REDIS_URL="${REDIS_BOT_URL}",BOT_MEETING_API_URL="${BOT_MEETING_API_URL}",ADMIN_TOKEN="${VEXA_ADMIN_API_TOKEN}",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",CORS_ORIGINS="${CORS_ORIGINS:-*}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}",ZOOM_CLIENT_ID="${ZOOM_CLIENT_ID:-}",ZOOM_CLIENT_SECRET="${ZOOM_CLIENT_SECRET:-}",STORAGE_BACKEND="${STORAGE_BACKEND:-minio}",MINIO_ENDPOINT="localhost:9000",MINIO_ACCESS_KEY="${MINIO_ACCESS_KEY:-vexa-access-key}",MINIO_SECRET_KEY="${MINIO_SECRET_KEY:-vexa-secret-key}",MINIO_BUCKET="${MINIO_BUCKET:-vexa-recordings}",MINIO_SECURE="false",RECORDING_ENABLED="${RECORDING_ENABLED:-false}",CAPTURE_MODES="audio"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-meeting-api.log
stderr_logfile=/var/log/vexa-meeting-api.err

[program:vexa-agent-api]
command=/opt/venv/bin/uvicorn agent_api.main:app --host 0.0.0.0 --port 8100
directory=/opt/vexa/services/agent-api
environment=REDIS_URL="${REDIS_LOCAL_URL}",RUNTIME_API_URL="http://localhost:8090",ADMIN_API_URL="http://localhost:8001",ADMIN_API_TOKEN="${VEXA_ADMIN_API_TOKEN:-}",AGENT_API_INTERNAL_URL="http://localhost:8100",INTERNAL_API_SECRET="${INTERNAL_API_SECRET:-}",VEXA_ENV="${VEXA_ENV:-production}",CORS_ORIGINS="${CORS_ORIGINS:-*}",ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-agent-api.log
stderr_logfile=/var/log/vexa-agent-api.err

[program:vexa-mcp]
command=/opt/venv/bin/python main.py
directory=/opt/vexa/services/mcp
environment=API_GATEWAY_URL="http://localhost:8056",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-mcp.log
stderr_logfile=/var/log/vexa-mcp.err

[program:vexa-tts-service]
command=/opt/venv/bin/uvicorn main:app --host 0.0.0.0 --port 8002
directory=/opt/vexa/services/tts-service
environment=TTS_API_TOKEN="${TTS_API_TOKEN:-}",OPENAI_API_KEY="${OPENAI_API_KEY:-}",OPENAI_BASE_URL="${OPENAI_BASE_URL:-https://api.openai.com}",VEXA_ENV="${VEXA_ENV:-production}",LOG_LEVEL="${LOG_LEVEL:-INFO}",PIPER_VOICES_DIR="/app/voices"
autostart=true
autorestart=true
stdout_logfile=/var/log/vexa-tts.log
stderr_logfile=/var/log/vexa-tts.err

[program:runtime-api]
command=/opt/venv/bin/uvicorn runtime_api.main:app --host 0.0.0.0 --port 8090
directory=/opt/vexa/services/runtime-api
environment=ORCHESTRATOR_BACKEND="${ORCHESTRATOR_BACKEND:-runpod}",REDIS_URL="${REDIS_LOCAL_URL}",RUNPOD_ACCOUNT_API_KEY="${RUNPOD_ACCOUNT_API_KEY:-${RUNPOD_API_KEY:-}}",RUNPOD_GPU_TYPE="${RUNPOD_GPU_TYPE:-NVIDIA GeForce RTX 3090}",RUNPOD_GPU_TYPES="${RUNPOD_GPU_TYPES:-NVIDIA GeForce RTX 3090,NVIDIA GeForce RTX 5090,NVIDIA RTX A5000,NVIDIA RTX A4000}",RUNPOD_CLOUD_TYPE="${RUNPOD_CLOUD_TYPE:-COMMUNITY}",RUNPOD_CONTAINER_DISK_GB="${RUNPOD_CONTAINER_DISK_GB:-40}",RUNPOD_POLL_INTERVAL="${RUNPOD_POLL_INTERVAL:-15}",BROWSER_IMAGE="${BROWSER_IMAGE:-${BOT_IMAGE}}",HOST="0.0.0.0",PORT="8090",LOG_LEVEL="${LOG_LEVEL:-INFO}",VEXA_ENV="${VEXA_ENV:-production}"
autostart=true
autorestart=true
stdout_logfile=/var/log/runtime-api.log
stderr_logfile=/var/log/runtime-api.err
SUPERVISOR

# Add cloudflared if config exists
if [[ -f /etc/cloudflared/config.yml ]]; then
    cat >> /etc/supervisor/conf.d/kioku.conf <<CLOUDFLARED

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
echo "[KIOKU] Starting stateful services..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/kioku.conf
