---
title: "Quickstart"
---
Get Kioku running locally in 5 minutes.

<Steps>
  <Step title="Prerequisites">
    - [Docker](https://docs.docker.com/get-docker/) with Compose v2
    - NVIDIA GPU + [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/) (for Ollama embeddings)
    - (Optional) Cloudflare Tunnel for public access
  </Step>

  <Step title="Bootstrap environment">
    ```bash
    git clone https://github.com/kioku-org/kioku.git
    cd kioku/deployment/docker

    # Copies .env.example, generates secure secrets, pulls base images
    ./scripts/setup.sh
    ```
  </Step>

  <Step title="Configure secrets">
    ```bash
    # Required — set your Vexa admin token
    $EDITOR .env

    # Optional — add API keys for integrations
    # OPENAI_API_KEY=...
    # ANTHROPIC_API_KEY=...
    # ZOOM_CLIENT_ID=...
    # ZOOM_CLIENT_SECRET=...
    ```
  </Step>

  <Step title="Start the stack">
    ```bash
    ./scripts/manage.sh start
    ```
    This starts stateful services (Postgres, Qdrant) first, then all stateless services (Hivemind, Vexa, Ollama, etc.).
  </Step>

  <Step title="Verify health">
    ```bash
    ./scripts/healthcheck.sh
    ```
    All services should show green checkmarks.
  </Step>

  <Step title="Install the CLI">
    ```bash
    cd ../../apps/cli
    cargo install --path crates/cc-cli

    # Sign in (create an admin account first via API)
    kioku signin
    ```
  </Step>

  <Step title="Optional: Cloudflare Tunnel">
    ```bash
    cp cloudflared.yml.example cloudflared.yml
    # Edit with your tunnel ID and domains
    # Set CLOUDFLARED_CREDENTIALS_DIR in .env
    ```
  </Step>
</Steps>

## Services

| Service | Port | Description |
|---------|------|-------------|
| Hivemind API | `9100` | Core API (auth, sessions, knowledge search, MCP) |
| Vexa API Gateway | `8056` | Meeting bot API |
| Vexa Admin API | `8057` | Admin operations |
| Vexa MCP | `18888` | Vexa MCP server |
| MinIO Console | `9001` | Object storage UI |
| Ollama | `11434` | Local embedding model server |
| Qdrant | `6333` | Vector DB REST API |

## Management

```bash
./scripts/manage.sh status          # running containers + resource usage
./scripts/manage.sh logs <service>  # tail logs (e.g. logs kioku-hivemind)
./scripts/manage.sh stop            # stop all (data preserved)
./scripts/manage.sh down            # stop and remove containers
./scripts/manage.sh down-volumes    # ⚠ destroy ALL data
./scripts/manage.sh backup          # dump databases to backups/
./scripts/manage.sh restore <file>  # restore from backup
```