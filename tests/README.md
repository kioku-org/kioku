# Tests

## Structure

| Directory | Type | Description |
|---|---|---|
| `services/cli/crates/cc-auth/tests/` | Unit | Auth file serialization tests |
| `services/cli/crates/cc-cli/src/main.rs` | Unit | CLI arg parsing + target resolution tests |
| `services/cli/crates/cc-kioku/tests/types_test.rs` | Unit | API type serialization tests |
| `services/cli/crates/cc-kioku/tests/integration_test.rs` | Integration | Client library tests (requires running server) |
| `services/hivemind/src/` | Unit | Knowledge chunking, PDF parsing tests |
| `services/hivemind/tests/` | Integration | HTTP API tests (require running server) |
| `services/dashboard/tests/` | Unit | vitest tests |
| `services/mcp/tests/` | Unit | pytest tests |

## Running Tests

### All unit tests (no server required)

```bash
# CLI
cd services/cli && cargo test --lib --bins --tests

# Hivemind
cd services/hivemind && SQLX_OFFLINE=true cargo test --bin kioku-hivemind

# Dashboard
cd services/dashboard && npx vitest run

# MCP
cd services/mcp && pytest tests/ -v
```

### Hivemind integration tests

Requires a running Hivemind server:

```bash
cd deployment/docker && ./scripts/manage.sh start
cd ../../services/hivemind
HIVEMIND_URL=http://localhost:9100 cargo test
```

### CLI integration tests

Requires a running Hivemind server:

```bash
cd services/cli
HIVEMIND_URL=http://localhost:9100 cargo test --test integration_test --features integration
```

### RunPod integration workflow

To validate the published RunPod images for a specific commit SHA:

```bash
gh workflow run "RunPod Integration Test" -f image_sha=<short-or-full-git-sha>
```

The workflow waits for the published stateful/stateless images, boots the
RunPod stateful pod, runs Hivemind and CLI integration tests against the live
pod, then verifies stateless pod creation through `runtime-api`.

### Docker smoke test

```bash
cd deployment/docker
./scripts/smoke-test.sh
```

### Docker health check

```bash
cd deployment/docker
./scripts/healthcheck.sh
```
