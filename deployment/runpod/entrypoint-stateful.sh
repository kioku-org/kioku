#!/bin/bash
set -e

echo "[KIOKU] Starting stateful services..."

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

# Create data directories
mkdir -p /data/qdrant /data/minio

echo "[KIOKU] Starting all services via supervisord..."
exec /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf
