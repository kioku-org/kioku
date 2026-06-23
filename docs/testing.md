---
title: "Testing"
---
How to run and write tests for Kioku.

## Test Overview

Kioku has three test layers:

| Layer | Location | Requires Server | Run With |
|---|---|---|---|
| CLI unit tests | `apps/cli/crates/*/tests/` | No | `cargo test` |
| Hivemind integration tests | `services/hivemind/tests/` | Yes | `cargo test` |
| End-to-end (manual) | `tests/` | Yes | `./tests/run-tests.sh` |

## CLI Unit Tests

These test serialization, auth file handling, and API types. No running server needed.

```bash
cd apps/cli

# Run all unit tests
cargo test -p cc-auth -p cc-kioku

# Run with output
cargo test -p cc-auth -p cc-kioku -- --nocapture
```

### Test Coverage

**cc-auth** (3 tests):
- Auth serialization/deserialization
- Default credentials path
- Token expiry detection

**cc-kioku** (11 tests):
- Session types (Session, SessionConfig, CreateSessionRequest)
- Meeting types (Meeting, TranscriptSegment)
- Knowledge types (Document, SearchResult, SearchResponse)
- API request/response types
- Error handling

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
- Auth flow (signup, login, token refresh)
- Session CRUD
- Knowledge ingestion and search
- Meeting creation and retrieval
- Usage tracking

## End-to-End Tests

Full workflow tests using the CLI against a live stack.

```bash
# Run all e2e tests
cd tests
./run-tests.sh

# Or manually
kioku signup --email test@example.com --password testpass123
kioku login --email test@example.com --password testpass123
kioku create-session --name "Test Session"
kioku search --query "test query"
```

## Writing New Tests

### CLI Unit Tests

Add tests in `apps/cli/crates/<crate>/tests/`:

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
use hivemind::test_utils::TestServer;

#[tokio::test]
async fn test_new_endpoint() {
    let server = TestServer::new().await;
    let client = server.client();

    let response = client.get("/api/v1/new-endpoint").send().await;
    assert_eq!(response.status(), 200);
}
```

## Continuous Integration

GitHub Actions runs:
1. `build-images.yml` — Builds and pushes Docker images on push to master
2. `runpod-test.yml` — Deploys a test pod on RunPod and runs health checks

Tests must pass before merging to master.