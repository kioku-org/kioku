#!/bin/bash
set -euo pipefail

# Kioku Dashboard Deployment Script
# Usage: ./deploy.sh [build|start|stop|restart|logs|status]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_env() {
    if [ ! -f "$SCRIPT_DIR/.env" ]; then
        log_error ".env file not found. Copy .env.example to .env and configure it."
        exit 1
    fi

    # Source .env to check required variables
    set -a
    source "$SCRIPT_DIR/.env"
    set +a

    if [ -z "${VEXA_API_URL:-}" ]; then
        log_error "VEXA_API_URL is required in .env"
        exit 1
    fi

    if [ -z "${VEXA_ADMIN_API_KEY:-}" ]; then
        log_warn "VEXA_ADMIN_API_KEY is not set. User management may not work."
    fi
}

build() {
    log_info "Building dashboard image..."
    cd "$SCRIPT_DIR"
    docker compose build
    log_info "Build complete."
}

start() {
    log_info "Starting dashboard..."
    cd "$SCRIPT_DIR"
    docker compose up -d
    log_info "Dashboard started. Check status with: $0 status"
}

stop() {
    log_info "Stopping dashboard..."
    cd "$SCRIPT_DIR"
    docker compose down
    log_info "Dashboard stopped."
}

restart() {
    log_info "Restarting dashboard..."
    cd "$SCRIPT_DIR"
    docker compose restart
    log_info "Dashboard restarted."
}

logs() {
    cd "$SCRIPT_DIR"
    docker compose logs -f dashboard
}

status() {
    cd "$SCRIPT_DIR"
    docker compose ps
}

health_check() {
    log_info "Checking dashboard health..."
    if curl -sf http://localhost:3001/api/health > /dev/null 2>&1; then
        log_info "Dashboard is healthy."
        return 0
    else
        log_error "Dashboard health check failed."
        return 1
    fi
}

main() {
    local command=${1:-help}

    check_env

    case $command in
        build)
            build
            ;;
        start)
            start
            ;;
        stop)
            stop
            ;;
        restart)
            restart
            ;;
        logs)
            logs
            ;;
        status)
            status
            ;;
        health)
            health_check
            ;;
        help|*)
            echo "Usage: $0 {build|start|stop|restart|logs|status|health}"
            echo ""
            echo "Commands:"
            echo "  build    Build the dashboard Docker image"
            echo "  start    Start the dashboard container"
            echo "  stop     Stop the dashboard container"
            echo "  restart  Restart the dashboard container"
            echo "  logs     View dashboard logs"
            echo "  status   Show container status"
            echo "  health   Check dashboard health"
            ;;
    esac
}

main "$@"
