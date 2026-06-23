# Tests

## Structure

| Directory | Type | Description |
|---|---|---|
| `tests/run-tests.sh` | Runner | Runs all kioku test suites |
| `apps/cli/crates/cc-auth/tests/` | Unit | Auth file serialization tests |
| `apps/cli/crates/cc-kioku/tests/` | Unit | API type serialization/deserialization tests |
| `services/hivemind/tests/` | Integration | HTTP API tests (require running server) |

## Running Tests

### All tests (recommended)

```bash
./tests/run-tests.sh
```

This runs CLI unit tests, Hivemind compile check, and Docker stack health. Hivemind integration tests run automatically if the server is running on `:9100`.

### CLI unit tests only

```bash
cd apps/cli
cargo test -p cc-auth -p cc-kioku
```

### Hivemind integration tests

Requires a running Hivemind server:

```bash
cd deployment/docker && ./scripts/manage.sh start
cd ../../services/hivemind
HIVEMIND_URL=http://localhost:9100 cargo test
```

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