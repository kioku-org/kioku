# Contributing to Kioku

## Project Structure

```
kioku/
├── apps/
│   └── cli/                    # Kioku CLI (Rust)
│       ├── crates/
│       │   ├── cc-cli/         # Binary: kioku
│       │   ├── cc-kioku/       # HTTP client
│       │   ├── cc-auth/        # Auth file management
│       │   └── cc-upgrade/     # Self-update
│       └── AGENTS.md           # Rust agent guidelines
├── services/
│   ├── hivemind/               # Core API server (Rust/axum)
│   │   ├── src/
│   │   │   ├── handlers/      # HTTP handlers
│   │   │   ├── repos/          # Data access layer
│   │   │   ├── services/       # Business logic
│   │   │   └── mcp/            # MCP server
│   │   ├── migrations/         # SQL migrations
│   │   └── tests/              # Integration tests
│   └── vexa/                   # Vendored Vexa meeting-bot platform
├── deployment/
│   ├── docker/                 # Docker Compose deployment
│   └── runpod/                 # RunPod pod deployment
└── docs/                       # This documentation
```

## Development

### Hivemind (Rust)

```bash
cd services/hivemind
cargo check                    # Fast type check
cargo test                     # Run integration tests (needs running server)
cargo insta test --accept      # Update snapshots
```

Integration tests require a running Hivemind server. Start the Docker stack first:

```bash
cd deployment/docker
./scripts/manage.sh start
cd ../../services/hivemind
HIVEMIND_URL=http://localhost:9100 cargo test
```

See `apps/cli/AGENTS.md` for Rust coding guidelines (error handling, test style, domain types).

### CLI (Rust)

```bash
cd apps/cli
cargo check
cargo build                    # Debug build
./target/debug/kioku --help    # Run locally
```

### Vexa (Python)

Vexa services are vendored from [Vexa-ai/vexa](https://github.com/Vexa-ai/vexa). Changes should be made upstream unless kioku-specific.

---

## Testing

| Component | Test Type | Command |
|---|---|---|
| Hivemind | Integration (HTTP) | `cargo test` in `services/hivemind/` |
| CLI | Unit | `cargo test` in `apps/cli/` |
| Docker stack | Smoke test | `./deployment/docker/scripts/smoke-test.sh` |
| RunPod | Integration | GitHub Actions `runpod-test.yml` |

### Running Tests

```bash
# Start the stack
cd deployment/docker && ./scripts/manage.sh start

# Run hivemind integration tests
cd ../../services/hivemind && cargo test

# Run Docker smoke test
cd ../../deployment/docker && ./scripts/smoke-test.sh

# Health check
./scripts/healthcheck.sh
```

---

## Git Workflow

1. Create a feature branch: `feat/description` or `fix/description`
2. Make changes, commit with descriptive messages
3. Use `Co-Authored-By: Kioku <noreply@kioku.chat>` for AI-assisted commits
4. Open a PR to `master`
5. Ensure CI passes (Docker build + RunPod integration test)

---

## Deployment

Images are built and pushed to Docker Hub automatically on merge to master:

- `kyomoto/kioku-stateful:latest` — stateful pod (CPU, always-on)
- `kyomoto/kioku-stateless:latest` — stateless pod (GPU, ephemeral bot)

See [DEPLOYMENT.md](./DEPLOYMENT.md) for details.