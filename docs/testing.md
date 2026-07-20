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

**cc-auth** (3 tests, `tests/auth_test.rs`):
- Auth file roundtrip (write to disk, read back)
- Full-field serialization
- Deserialization with an omitted optional field (`active_workspace_id`)

**cc-cli** (24 tests, inline `#[cfg(test)]` modules — no `tests/` dir):
- Server URL resolution (CLI override, env var, default)
- CLI argument parsing
- Document delete target resolution (exact ID, prefix, ambiguous, unknown)
- Auth key delete target resolution (ID, prefix, unknown)
- MCP config/URL building (both servers present, port replacement, subdomain vs. path fallback)
- Bot-kill target resolution (exact ID, prefix, ambiguous, unknown) + segment-key selection
- Google Calendar date parsing (valid, invalid month, wrong format) + week-range calculation

**cc-kioku** (12 tests: 11 in `tests/types_test.rs`, 1 inline in `src/client.rs`):
- Auth session roundtrip, session defaults, workspace config/auth-key fields
- Knowledge search request defaults + result nesting
- Meeting ingest defaults, message roundtrip, content-part text, upload response fields
- API error format
- Document MIME type inference from extension

## Hivemind Unit Tests

Run the inline unit tests (knowledge chunking, PDF parsing):

```bash
cd services/hivemind
SQLX_OFFLINE=true cargo test --bin kioku-hivemind
```

## Hivemind Integration Tests

These test the full API stack. Requires a running Hivemind instance.

```bash
# Start the stack first (scripts/setup.sh and scripts/manage.sh predate the
# current single-container image and don't reflect it — use compose directly)
cd deployment/docker
cp .env.example .env   # fill in secrets
docker compose -f docker-compose.stateful.yml up -d

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

Triggered on push or PR to `main`/`master`, path-filtered to `services/dashboard/**`,
`services/mcp/**`, `services/hivemind/**`, and `services/cli/**` (not all of `services/**`
— changes to meeting-api, admin-api, agent-api, api-gateway, runtime-api, cookie,
transcription, or vexa-bot don't trigger this workflow):

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

### RunPod Integration Test (`test` job in `.github/workflows/ci.yml`)

Not a separate workflow file — it's the `test` job inside `ci.yml`, gated on the
`build-stateful`/`build-stateless` jobs completing (`needs:`), same workflow that builds
and pushes the images:

1. Deploys a stateful pod on RunPod from the just-built images
2. Runs `deployment/docker/scripts/ci-smoke-test.sh` (Hivemind `/health`, plus a rejected
   unauthenticated bot request and an authenticated bot-list call against Vexa)
3. Runs hivemind integration tests against the live pod
4. Runs CLI integration tests against the live pod
5. Destroys the test pod

`ci.yml` has no `image_sha` input — the only `workflow_dispatch` input is `skip_tests`. To
manually trigger a run (e.g. to rebuild images without a new commit, or skip the RunPod
test):

```bash
gh workflow run ci.yml -f skip_tests=false
```

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
