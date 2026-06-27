#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEPLOY_DIR="$(dirname "$SCRIPT_DIR")"
cd "$DEPLOY_DIR"

STATEFUL_FILE="docker-compose.stateful.yml"
STATELESS_FILE="docker-compose.stateless.yml"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

compose_stateful()  { docker compose -f "$STATEFUL_FILE" "$@"; }
compose_stateless() { docker compose -f "$STATELESS_FILE" "$@"; }

# ─── Pre-flight checks ────────────────────────────────────────────────────────

check_prerequisites() {
    command -v docker >/dev/null 2>&1 || error "Docker is not installed"
    command -v docker compose >/dev/null 2>&1 || error "Docker Compose is not installed"

    if [[ ! -f .env ]]; then
        warn ".env not found. Copying from .env.example..."
        cp .env.example .env
        warn "Please edit .env with your secrets before starting."
        exit 1
    fi
}

# ─── Commands ─────────────────────────────────────────────────────────────────

cmd_start_stateful() {
    check_prerequisites
    info "Starting stateful services (postgres, qdrant)..."
    compose_stateful up -d
    info "Stateful services started."
}

cmd_stop_stateful() {
    info "Stopping stateful services..."
    compose_stateful stop
    info "Stateful services stopped."
}

cmd_down_stateful() {
    warn "This will remove stateful containers (data volumes preserved)."
    read -rp "Continue? (y/N) " confirm
    [[ "$confirm" =~ ^[Yy]$ ]] || { info "Aborted"; exit 0; }
    compose_stateful down
}

cmd_start() {
    check_prerequisites
    info "Starting stateful services (postgres, qdrant)..."
    compose_stateful up -d
    info "Waiting for stateful services to be healthy..."
    sleep 5
    info "Starting stateless services..."
    compose_stateless up -d --build
    info ""
    info "Platform is starting. Services will be available at:"
    info "  Hivemind API:   http://localhost:9100"
    info "  Vexa API:       http://localhost:8056"
    info "  Vexa Admin:     http://localhost:8057"
    info "  Qdrant:         http://localhost:6334"
    info "  Ollama:         http://localhost:11434"
    info "  Minio Console:  http://localhost:9001"
    info ""
    info "Check health: ./scripts/healthcheck.sh"
}

cmd_stop() {
    info "Stopping stateless services..."
    compose_stateless stop
    info "Stopping stateful services..."
    compose_stateful stop
    info "All services stopped."
}

cmd_down() {
    info "Shutting down stateless services..."
    compose_stateless down
    info "Shutting down stateful services..."
    compose_stateful down
    info "All services removed."
}

cmd_down_volumes() {
    warn "This will destroy ALL data including databases!"
    read -rp "Are you sure? (y/N) " confirm
    if [[ "$confirm" =~ ^[Yy]$ ]]; then
        info "Destroying stateless services and data..."
        compose_stateless down -v --remove-orphans
        info "Destroying stateful services and data..."
        compose_stateful down -v --remove-orphans
        info "All data destroyed."
    else
        info "Aborted"
    fi
}

cmd_restart() {
    info "Restarting all services..."
    compose_stateful restart
    compose_stateless restart
    info "Services restarted."
}

cmd_status() {
    info "Stateful services:"
    compose_stateful ps
    echo ""
    info "Stateless services:"
    compose_stateless ps
    echo ""
    info "Resource Usage:"
    docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}" \
        $(docker ps --format '{{.Names}}' | grep '^kioku-' | tr '\n' ' ') 2>/dev/null || warn "No running containers"
}

cmd_logs() {
    local service="${1:-}"
    if [[ -n "$service" ]]; then
        # Try stateless first, fall back to stateful
        compose_stateless logs -f "$service" 2>/dev/null || compose_stateful logs -f "$service"
    else
        compose_stateless logs -f
    fi
}

cmd_backup() {
    local timestamp
    timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_dir="$DEPLOY_DIR/backups"
    mkdir -p "$backup_dir"

    info "Backing up full database to $backup_dir/kioku_$timestamp.sql..."
    compose_stateful exec -T postgres pg_dump -U kioku -d kioku --no-owner --no-privileges \
        > "$backup_dir/kioku_$timestamp.sql"

    info "Backing up hivemind schema..."
    compose_stateful exec -T postgres pg_dump -U kioku -d kioku \
        --schema=hivemind --no-owner --no-privileges \
        > "$backup_dir/hivemind_$timestamp.sql"

    info "Backing up vexa schema..."
    compose_stateful exec -T postgres pg_dump -U kioku -d kioku \
        --schema=vexa --no-owner --no-privileges \
        > "$backup_dir/vexa_$timestamp.sql"

    info "Backup complete:"
    ls -lh "$backup_dir/"*"$timestamp"*
}

cmd_restore() {
    local backup_file="${1:-}"
    if [[ -z "$backup_file" ]]; then
        error "Usage: $0 restore <backup-file.sql>"
    fi
    if [[ ! -f "$backup_file" ]]; then
        error "Backup file not found: $backup_file"
    fi

    warn "This will overwrite the current database!"
    read -rp "Are you sure? (y/N) " confirm
    if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
        info "Aborted"
        exit 0
    fi

    info "Restoring from $backup_file..."
    compose_stateful exec -T postgres psql -U kioku -d kioku < "$backup_file"
    info "Restore complete."
}

cmd_shell() {
    local service="${1:-postgres}"
    info "Opening shell in $service..."
    if [[ "$service" == "postgres" || "$service" == "qdrant" ]]; then
        compose_stateful exec "$service" /bin/sh || compose_stateful exec "$service" /bin/bash
    else
        compose_stateless exec "$service" /bin/sh || compose_stateless exec "$service" /bin/bash
    fi
}

cmd_db_shell() {
    info "Opening psql shell..."
    compose_stateful exec postgres psql -U kioku -d kioku
}

# ─── Main ─────────────────────────────────────────────────────────────────────

usage() {
    cat <<EOF
Kioku Platform Manager

Usage: $0 <command> [args]

Full-stack commands (stateful + stateless):
  start              Start all services
  stop               Stop all services (keep data)
  down               Stop and remove all containers
  down-volumes       Destroy ALL data (databases, volumes, etc.)
  restart            Restart all services
  status             Show service status and resource usage
  logs [service]     Follow logs (optionally for a specific service)
  backup             Backup all databases to backups/
  restore <file>     Restore database from backup file
  shell [service]    Open shell in a service container (default: postgres)
  db-shell           Open psql shell to the database

Stateful-only commands (postgres, qdrant):
  start-stateful     Start only postgres and qdrant
  stop-stateful      Stop only postgres and qdrant
  down-stateful      Remove stateful containers (preserves volumes)

Examples:
  $0 start
  $0 logs kioku-hivemind
  $0 backup
  $0 restore backups/kioku_20260401_120000.sql
  $0 start-stateful          # bring up just postgres+qdrant
EOF
}

case "${1:-help}" in
    start)            cmd_start ;;
    stop)             cmd_stop ;;
    down)             cmd_down ;;
    down-volumes)     cmd_down_volumes ;;
    restart)          cmd_restart ;;
    status)           cmd_status ;;
    logs)             cmd_logs "${2:-}" ;;
    backup)           cmd_backup ;;
    restore)          cmd_restore "${2:-}" ;;
    shell)            cmd_shell "${2:-}" ;;
    db-shell)         cmd_db_shell ;;
    start-stateful)   cmd_start_stateful ;;
    stop-stateful)    cmd_stop_stateful ;;
    down-stateful)    cmd_down_stateful ;;
    help|--help|-h)   usage ;;
    *)                error "Unknown command: $1. Run '$0 help' for usage." ;;
esac
