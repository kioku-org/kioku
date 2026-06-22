#!/usr/bin/env bash
set -euo pipefail

PG_MAJOR="${PG_MAJOR:-16}"
PG_BIN="/usr/lib/postgresql/${PG_MAJOR}/bin"
PGDATA="/data/postgresql"
DB_NAME="${DB_NAME:-kioku}"
DB_USER="${DB_USER:-kioku}"
DB_PASSWORD="${DB_PASSWORD:-kioku}"
MINIO_ROOT_USER="${MINIO_ROOT_USER:-kioku}"
MINIO_ROOT_PASSWORD="${MINIO_ROOT_PASSWORD:-kioku-minio-password}"

echo "[KIOKU] Preparing stateful runtime..."

mkdir -p \
    /data/postgresql \
    /data/qdrant \
    /data/minio \
    /data/redis \
    /data/ollama/models \
    /etc/qdrant \
    /run/sshd \
    /var/run/postgresql \
    /root/.ssh

chmod 700 /root/.ssh
chown postgres:postgres "$PGDATA" /var/run/postgresql
chown -R redis:redis /data/redis 2>/dev/null || true

if [[ -n "${PUBLIC_KEY:-}" ]]; then
    printf '%s\n' "$PUBLIC_KEY" > /root/.ssh/authorized_keys
    chmod 600 /root/.ssh/authorized_keys
fi

sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config

if [[ ! -f "$PGDATA/PG_VERSION" ]]; then
    echo "[KIOKU] Initializing PostgreSQL ${PG_MAJOR}..."
    sudo -u postgres "$PG_BIN/initdb" -D "$PGDATA"
    {
        echo "listen_addresses='*'"
        echo "host all all 0.0.0.0/0 trust"
    } >> "$PGDATA/postgresql.conf"
    echo "host all all 0.0.0.0/0 trust" >> "$PGDATA/pg_hba.conf"

    sudo -u postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -w start
    sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE USER ${DB_USER} WITH PASSWORD '${DB_PASSWORD}';"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${DB_NAME} OWNER ${DB_USER};"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS vector;"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS hivemind;"
    sudo -u postgres psql -v ON_ERROR_STOP=1 -d "$DB_NAME" -c "CREATE SCHEMA IF NOT EXISTS vexa;"
    sudo -u postgres "$PG_BIN/pg_ctl" -D "$PGDATA" -m fast -w stop
fi

sed -i 's/^bind 127.0.0.1.*/bind 0.0.0.0/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's/^protected-mode yes/protected-mode no/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's|^dir .*|dir /data/redis|' /etc/redis/redis.conf 2>/dev/null || true
grep -q '^appendonly ' /etc/redis/redis.conf \
    && sed -i 's/^appendonly .*/appendonly yes/' /etc/redis/redis.conf \
    || echo 'appendonly yes' >> /etc/redis/redis.conf

cat > /etc/qdrant/config.yaml <<'QDRANT'
service:
  host: 0.0.0.0
  http_port: 6334
storage:
  storage_path: /data/qdrant
QDRANT

cat > /etc/supervisor/conf.d/kioku-stateful.conf <<SUPERVISOR
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
SUPERVISOR

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

echo "[KIOKU] Starting stateful services..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/kioku-stateful.conf
