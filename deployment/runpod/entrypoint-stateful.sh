#!/bin/bash
set -e

echo "[KIOKU] Starting stateful services..."

# Ensure postgres user exists (minimal Ubuntu images may not create it)
if ! id -u postgres &>/dev/null; then
    echo "[KIOKU] Creating postgres user..."
    useradd -r -m -d /var/lib/postgresql -s /bin/bash postgres
fi

mkdir -p /var/run/postgresql && chown postgres:postgres /var/run/postgresql
mkdir -p /data/qdrant /data/minio
chown postgres:postgres /var/lib/postgresql

# Initialize PostgreSQL if not already done
if [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then
    echo "[KIOKU] Initializing PostgreSQL..."
    su - postgres -c "/usr/lib/postgresql/14/bin/initdb -D /var/lib/postgresql/data"

    # Configure for network access
    echo "host all all 0.0.0.0/0 trust" >> /var/lib/postgresql/data/pg_hba.conf
    echo "listen_addresses='*'" >> /var/lib/postgresql/data/postgresql.conf

    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data start"

    # Create database and schemas
    su - postgres -c "psql -c \"CREATE USER kioku WITH PASSWORD 'kioku';\""
    su - postgres -c "psql -c \"CREATE DATABASE kioku OWNER kioku;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS vector;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS \\\"uuid-ossp\\\";\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS hivemind;\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS vexa;\""

    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data stop"
    echo "[KIOKU] PostgreSQL initialized"
fi

echo "[KIOKU] Starting all services via supervisord..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf &

# Wait for Ollama to start, then pull the embedding model
sleep 5
echo "[KIOKU] Pulling nomic-embed-text-v2-moe embedding model..."
until ollama pull nomic-embed-text-v2-moe; do
    echo "[KIOKU] Retrying model pull in 10s..."
    sleep 10
done
echo "[KIOKU] Model pulled successfully"

# Keep supervisord in foreground
wait
