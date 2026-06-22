#!/bin/bash
set -x
set -e

trap 'echo "[KIOKU] FATAL ERROR on line $LINENO. Sleeping 1h for debugging..."; sleep 3600; exit 1' ERR

echo "[KIOKU] Installing dependencies..."
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y curl wget gnupg lsb-release sudo tzdata zstd openssh-server

# Set timezone non-interactively
ln -fs /usr/share/zoneinfo/UTC /etc/localtime
dpkg-reconfigure -f noninteractive tzdata

# Install PostgreSQL 14 + pgvector
echo "[KIOKU] Installing PostgreSQL 14..."
DISTRO_CODENAME=$(. /etc/os-release && echo "$VERSION_CODENAME")
echo "deb http://apt.postgresql.org/pub/repos/apt ${DISTRO_CODENAME}-pgdg main" > /etc/apt/sources.list.d/pgdg.list
curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc -o /tmp/postgresql.asc
gpg --batch --yes --dearmor -o /etc/apt/trusted.gpg.d/postgresql.gpg /tmp/postgresql.asc
rm -f /tmp/postgresql.asc
apt-get update
apt-get install -y postgresql-14 postgresql-14-pgvector

# Install Redis
echo "[KIOKU] Installing Redis..."
apt-get install -y redis-server

# Install supervisor
apt-get install -y supervisor

# Install Qdrant
echo "[KIOKU] Installing Qdrant..."
curl -fsSL https://github.com/qdrant/qdrant/releases/download/v1.10.1/qdrant-x86_64-unknown-linux-gnu.tar.gz \
    -o /tmp/qdrant.tar.gz
tar xzf /tmp/qdrant.tar.gz -C /usr/local/bin qdrant
rm -f /tmp/qdrant.tar.gz
chmod +x /usr/local/bin/qdrant

# Install Ollama
echo "[KIOKU] Installing Ollama..."
curl -fsSL https://ollama.ai/install.sh | sh

# Install MinIO
echo "[KIOKU] Installing MinIO..."
curl -fsSL https://dl.min.io/server/minio/release/linux-amd64/minio -o /usr/local/bin/minio
chmod +x /usr/local/bin/minio

echo "[KIOKU] Starting stateful services..."

# Ensure postgres user exists
if ! id -u postgres &>/dev/null; then
    echo "[KIOKU] Creating postgres user..."
    useradd -r -m -d /var/lib/postgresql -s /bin/bash postgres
fi

mkdir -p /var/run/postgresql /data/postgresql /data/qdrant /data/minio /data/redis /data/ollama
chown postgres:postgres /var/run/postgresql /data/postgresql
chown postgres:postgres /var/lib/postgresql
chown -R redis:redis /data/redis 2>/dev/null || true
mkdir -p /run/sshd /root/.ssh
chmod 700 /root/.ssh
if [ -n "${PUBLIC_KEY:-}" ]; then
    printf '%s\n' "$PUBLIC_KEY" > /root/.ssh/authorized_keys
    chmod 600 /root/.ssh/authorized_keys
fi
sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config
sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication no/' /etc/ssh/sshd_config

export OLLAMA_HOST=0.0.0.0:11434
export OLLAMA_MODELS=/data/ollama/models

# Initialize PostgreSQL if not already done
if [ ! -f /data/postgresql/PG_VERSION ]; then
    echo "[KIOKU] Initializing PostgreSQL..."
    su - postgres -c "/usr/lib/postgresql/14/bin/initdb -D /data/postgresql"

    # Configure for network access
    echo "host all all 0.0.0.0/0 trust" >> /data/postgresql/pg_hba.conf
    echo "listen_addresses='*'" >> /data/postgresql/postgresql.conf

    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /data/postgresql start"

    # Create database and schemas
    su - postgres -c "psql -c \"CREATE USER kioku WITH PASSWORD 'kioku';\""
    su - postgres -c "psql -c \"CREATE DATABASE kioku OWNER kioku;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS vector;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS \\\"uuid-ossp\\\";\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS hivemind;\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS vexa;\""

    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /data/postgresql stop"
    echo "[KIOKU] PostgreSQL initialized"
fi

# Configure Redis
echo "[KIOKU] Configuring Redis..."
sed -i 's/^bind 127.0.0.1.*/bind 0.0.0.0/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's/^protected-mode yes/protected-mode no/' /etc/redis/redis.conf 2>/dev/null || true
sed -i 's|^dir .*|dir /data/redis|' /etc/redis/redis.conf 2>/dev/null || true

# Configure Qdrant
echo "[KIOKU] Configuring Qdrant..."
mkdir -p /etc/qdrant
cat > /etc/qdrant/config.yaml << 'QDRANT'
service:
  host: 0.0.0.0
  http_port: 6334
storage:
  storage_path: /data/qdrant
QDRANT

# Create supervisord config
echo "[KIOKU] Creating supervisord config..."
cat > /etc/supervisor/conf.d/supervisord.conf << 'SUPERVISOR'
[supervisord]
nodaemon=true
logfile=/var/log/supervisord.log

[program:postgresql]
command=/usr/lib/postgresql/14/bin/postgres -D /data/postgresql
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

[program:ollama]
command=/usr/local/bin/ollama serve
environment=OLLAMA_HOST="0.0.0.0:11434",OLLAMA_MODELS="/data/ollama/models"
autostart=true
autorestart=true
stdout_logfile=/var/log/ollama.log
stderr_logfile=/var/log/ollama.err

[program:qdrant]
command=/usr/local/bin/qdrant --config-path /etc/qdrant/config.yaml
autostart=true
autorestart=true
stdout_logfile=/var/log/qdrant.log
stderr_logfile=/var/log/qdrant.err

[program:minio]
command=/usr/local/bin/minio server /data/minio --address ":9000" --console-address ":9001"
environment=MINIO_ROOT_USER="kioku",MINIO_ROOT_PASSWORD="kioku-minio-password"
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

# Create a background pull script that waits for ollama to be ready
cat > /usr/local/bin/kioku-pull-models.sh << 'PULLSCRIPT'
#!/bin/bash
set -x
set -e
echo "[KIOKU] Waiting for Ollama to be ready before pulling models..."
for i in {1..60}; do
    if curl -fsSL http://localhost:11434/api/tags > /dev/null 2>&1; then
        echo "[KIOKU] Ollama is ready, pulling nomic-embed-text-v2-moe..."
        ollama pull nomic-embed-text-v2-moe
        echo "[KIOKU] Model pulled successfully"
        exit 0
    fi
    echo "[KIOKU] Ollama not ready yet, retrying in 5s..."
    sleep 5
done
echo "[KIOKU] Ollama failed to become ready in time"
exit 1
PULLSCRIPT
chmod +x /usr/local/bin/kioku-pull-models.sh

echo "[KIOKU] Starting all services via supervisord..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf
