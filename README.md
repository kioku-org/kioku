```⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡀⠀⡀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢠⣰⣜⣽⣦⡄⣎
⠀⡀⣦⡀⢀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⢦⣻⣿⣿⣿⣿⡇
⡀⠹⡪⣿⣾⣷⣄⠀⠀⠀⠀⠀⠀⠀⠀⠐⢦⣝⣿⣿⣿⣿⠁
⠘⠷⣾⣿⣿⣿⣿⣿⣦⡀⠀⠀⠀⠀⠀⢐⣿⣾⣿⣿⣿⠏⠀
⠠⢥⣴⣾⣿⣿⣿⣿⣿⣿⣷⡄⠀⠀⢀⣲⣿⣿⣿⣿⠟⠀⠀
⠀⠀⢂⣩⣵⢿⣿⣿⣿⣿⣿⣿⣄⢠⣾⣿⣿⣿⣯⣥⡀⠀⠀
⠀⠀⠀⠠⠤⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡀⠀
⠀⠀⠀⠀⠀⠺⢯⣿⣿⢿⣿⣿⣿⣿⣿⣿⣿⣿⠃⠀⠱⠑⠀
⠀⠀⠀⠀⠀⠀⠀⠀⢀⣴⣾⣿⣿⣿⣿⡿⠛⠁⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⢀⣴⡿⣿⣿⡿⠻⣯⠀⠧⢀⣀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠉⠀⠈⠈⠀⠀⠀⣵⢦⠈⠿⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⠀⠀⠀⠀⠀⠀⠀⠀
```

kioku: save your context, wherever and whenever you are.

## Quick Start

### Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with Compose v2
- NVIDIA GPU + [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/) (for Ollama embeddings)
- (Optional) A Cloudflare Tunnel for public access

### Run it

```bash
cd deployment/docker

# 1. Bootstrap .env (copies template, generates secure secrets, pulls images)
./scripts/setup.sh

# 2. Fill in your API keys and domain
#    Required: VEXA_ADMIN_API_TOKEN
#    Optional: OPENAI_API_KEY, ANTHROPIC_API_KEY, ZOOM_CLIENT_ID/SECRET, ...
$EDITOR .env

# 3. (Optional) Configure Cloudflare Tunnel
cp cloudflared.yml.example cloudflared.yml
# Edit cloudflared.yml with your tunnel ID + domains
# Set CLOUDFLARED_CREDENTIALS_DIR in .env to your credentials folder

# 4. Start everything (stateful first, then stateless)
./scripts/manage.sh start

# 5. Verify all services are healthy
./scripts/healthcheck.sh
```

### Services

| Service | Port | Description |
|---|---|---|
| Hivemind API | `9100` | Core API (auth, sessions, knowledge search) |
| Vexa API Gateway | `8056` | Meeting bot API |
| Vexa Admin API | `8057` | Admin operations |
| Vexa MCP | `18888` | Model Context Protocol server |
| MinIO Console | `9001` | Object storage UI |
| Ollama | `11434` | Local embedding model server |
| Qdrant | `6333` | Vector DB REST API |

### Manage

```bash
./scripts/manage.sh status          # show running containers + resource usage
./scripts/manage.sh logs <service>  # tail logs (e.g. logs kioku-hivemind)
./scripts/manage.sh stop            # stop all services (data preserved)
./scripts/manage.sh down            # stop and remove containers
./scripts/manage.sh down-volumes    # ⚠ destroy ALL data
./scripts/manage.sh backup          # dump databases to backups/
./scripts/manage.sh restore <file>  # restore from a backup file
```

Run `./scripts/manage.sh help` for the full command list.

## License

MIT License
