#!/usr/bin/env bash
set -e

echo "[KIOKU] Kioku Platform starting on RunPod..."

# ─── 1. Initialize PostgreSQL ────────────────────────────────────────────────
echo "[KIOKU] Initializing PostgreSQL..."
if [ ! -f /var/lib/postgresql/data/PG_VERSION ]; then
    su - postgres -c "/usr/lib/postgresql/14/bin/initdb -D /var/lib/postgresql/data"
    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data -l /var/log/pg-init.log start"
    
    # Create database and user
    su - postgres -c "psql -c \"CREATE USER kioku WITH PASSWORD 'kioku';\""
    su - postgres -c "psql -c \"CREATE DATABASE kioku OWNER kioku;\""
    su - postgres -c "psql -c \"GRANT ALL PRIVILEGES ON DATABASE kioku TO kioku;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS vector;\""
    su - postgres -c "psql -d kioku -c \"CREATE EXTENSION IF NOT EXISTS \\\"uuid-ossp\\\";\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS hivemind;\""
    su - postgres -c "psql -d kioku -c \"CREATE SCHEMA IF NOT EXISTS vexa;\""
    
    su - postgres -c "/usr/lib/postgresql/14/bin/pg_ctl -D /var/lib/postgresql/data stop"
    echo "[KIOKU] PostgreSQL initialized"
fi

# ─── 2. Generate secrets ─────────────────────────────────────────────────────
JWT_SECRET=$(openssl rand -hex 32)
ENCRYPTION_SECRET=$(openssl rand -hex 32)
export JWT_SECRET
export ENCRYPTION_SECRET

echo "[KIOKU] Secrets generated"

# ─── 3. Start all services via supervisord ───────────────────────────────────
echo "[KIOKU] Starting all services..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf
