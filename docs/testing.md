---
title: "Testing"
---
How to run and write tests for Kioku.

## Test Overview

Kioku has three test layers:

| Layer | Location | Requires Server | Run With |
|---|---|---|---|
| CLI unit tests | `services/cli/crates/*/tests/` | No | `cargo test` |
| Hivemind unit tests | `services/hivemind/` (inline) | No | `cargo test --bin kioku-hivemind` |
| Hivemind integration tests | `services/hivemind/tests/` | Yes | `cargo test` |
| CLI integration tests | `services/cli/crates/cc-kioku/tests/` | Yes | `cargo test --features integration` |

## CLI Unit Tests

These test serialization, auth file handling, and API types. No running server needed.

```bash
cd services/cli

# Run all unit tests
cargo test --lib --bins --tests

# Run with output
cargo test --lib --bins --tests -- --nocapture
```

### Test Coverage

**cc-auth** (3 tests):
- Auth serialization/deserialization
- Default credentials path
- Token expiry detection

**cc-cli** (11 tests):
- Server URL resolution (CLI override, env var, default)
- CLI argument parsing
- Auth key delete target resolution (prefix, ID, unknown)
- API key delete target resolution (ID, provider, ambiguous)

**cc-kioku** (7 tests):
- Auth session roundtrip
- Session defaults
- Knowledge search result format
- Meeting ingest defaults
- Company auth key optional fields
- Message roundtrip
- Upload response fields

## Hivemind Unit Tests

Run the inline unit tests (knowledge chunking, PDF parsing):

```bash
cd services/hivemind
SQLX_OFFLINE=true cargo test --bin kioku-hivemind
```

## Hivemind Integration Tests

These test the full API stack. Requires a running Hivemind instance.

```bash
# Start the stack first
cd deployment/docker
./scripts/setup.sh
./scripts/manage.sh start

# Run integration tests
cd ../../services/hivemind
cargo test
```

Tests cover:
- Auth flow (register, signin, signout, /auth/me)
- Company management (config, members, invites, auth keys)
- Session CRUD
- Message send and list
- Meeting ingest and list
- Knowledge search (requires Ollama embeddings)

### Embedding-Dependent Tests

Some knowledge tests require a running Ollama instance. These auto-skip in CI and run on RunPod:

```bash
# Tests check for Ollama via EMBEDDING_API_URL env var
# If unavailable, they print "SKIP: embedding service not available"
```

## CLI Integration Tests

Tests the `cc-kioku` client library against a live server:

```bash
cd services/cli
HIVEMIND_URL=http://localhost:9100 cargo test --test integration_test --features integration
```

Tests cover:
- Auth lifecycle (register, signin, whoami)
- Session CRUD
- Message send and list
- Meeting ingest and list
- Auth key CRUD
- Knowledge search

## CI Pipeline

GitHub Actions runs two workflows:

### Service Tests (`.github/workflows/service-tests.yml`)

Triggered on push to `master` when `services/**` changes:

| Job | What it tests |
|---|---|
| `hivemind-lint` | cargo fmt + clippy |
| `hivemind-build` | Build release binary + unit tests + upload artifact |
| `cli-lint` | cargo fmt + clippy |
| `cli-unit` | Unit tests (excluding integration) |
| `dashboard-unit` | vitest |
| `dashboard-build` | Next.js production build |
| `mcp-unit` | pytest (⚠ stale — `services/mcp` is now a Rust binary with no `requirements.txt`/`tests/`; this job currently fails on any PR touching `services/mcp/**`) |
| `mcp-lint` | ruff (⚠ same staleness as above) |
| `integration` | Full integration: Postgres + Qdrant + hivemind + CLI + MCP + dashboard |

### RunPod Integration Test (`.github/workflows/runpod-test.yml`)

Triggered on push to `master` (via `Build and Push Docker Images` completion):

1. Deploys a stateful pod with all services
2. Runs health checks (Redis, Ollama, Hivemind, meeting-api, vexa-gateway, runtime-api)
3. Runs hivemind integration tests (including embedding-dependent)
4. Runs CLI integration tests
5. Spawns a stateless bot pod
6. Cleans up

The workflow also supports manual reruns against a published image SHA:

```bash
gh workflow run "RunPod Integration Test" -f image_sha=<short-or-full-git-sha>
```

Use this when you need to validate a specific published image without waiting for
the `workflow_run` trigger from `build-images.yml`.

## Writing New Tests

### CLI Unit Tests

Add tests in `services/cli/crates/<crate>/tests/`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        let result = my_function();
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

Add tests in `services/hivemind/tests/`:

```rust
use reqwest::Client;
use serde_json::json;

#[tokio::test]
async fn test_new_endpoint() {
    let c = Client::new();
    let resp = c.get(format!("{}/health", base_url()))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
}
```
