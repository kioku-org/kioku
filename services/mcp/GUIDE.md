# Kioku MCP refactor guide

This guide explains how to refactor `services/mcp` without changing its external MCP
behavior. Treat tool names, request schemas, credentials, response shapes, and HTTP
routes as compatibility contracts until a deliberate API change is approved.

## What the service does

`kioku-mcp` exposes one Streamable HTTP MCP endpoint at `/mcp`.

```text
MCP client
  -> StreamableHttpService
  -> KiokuMcpService implements ServerHandler
  -> tool dispatch
  -> Hivemind API or Vexa API gateway
```

The service has three tool categories:

- Meeting and bot tools proxy to the Vexa API gateway.
- Knowledge tools proxy to Hivemind.
- Meeting-link parsing is local, pure Rust logic.

Hivemind receives the caller's original Kioku credential. Vexa receives a per-user Vexa key
resolved through Hivemind's `/vexa/token` endpoint. Keep those authentication paths explicit.

## Rust concepts used here

### Structs and impl blocks

A `struct` holds related data. An `impl` block defines the behavior for that data.

```rust
pub struct VexaClient {
    http: reqwest::Client,
    base_url: Url,
}

impl VexaClient {
    pub async fn get_bot_status(&self, api_key: &str) -> Result<Value, McpError> {
        // Make one typed upstream request.
    }
}
```

### Traits

A trait is a contract. `rmcp::ServerHandler` is the protocol contract implemented by the
service. Keep that implementation thin. It should validate the MCP request and delegate to
application code.

### Ownership and Arc

`Arc<T>` gives several async MCP sessions shared ownership of immutable configuration or
clients. Do not put request-specific state in a shared `Arc`; pass that state as function
arguments instead.

### Result and error handling

Use `Result<T, E>` for operations that can fail. Do not return `String` errors from new code.
Use a typed error enum so callers can distinguish invalid input, missing credentials, upstream
timeouts, invalid upstream responses, and upstream HTTP failures.

### Serde

Use `Deserialize` for MCP input and `Serialize` for output. Prefer typed request structs when
the input shape is stable. Keep `serde_json::Map<String, Value>` only for deliberately
pass-through payloads such as metadata updates.

## Current refactor target

`src/handler.rs` currently owns:

- MCP tool and prompt definitions
- argument parsing
- credential extraction and Vexa-key resolution
- Hivemind and Vexa HTTP requests
- response conversion
- tool dispatch and meeting orchestration

Split these responsibilities without changing their behavior.

## Target module layout

```text
src/
  main.rs                 # Process setup and Axum router only
  config.rs               # Validated environment configuration
  transport.rs            # rmcp Streamable HTTP setup only
  app.rs                  # Thin ServerHandler implementation
  auth.rs                 # Extract and resolve caller credentials
  error.rs                # McpError and conversions to MCP results
  clients/
    mod.rs
    hivemind.rs           # Hivemind-specific HTTP API
    vexa.rs               # Vexa-specific HTTP API
  tools/
    mod.rs                # Registry and small dispatch match
    meetings.rs
    recordings.rs
    knowledge.rs
    prompts.rs
  meeting_links.rs        # Pure parser and its unit tests
```

Do not create a generic backend client that erases Hivemind and Vexa authentication
differences. Shared HTTP mechanics can live in a small helper, but each upstream client should
own its routes, headers, request types, and response handling.

## Refactor plan

### 1. Establish a baseline

Run these commands before moving code and after every small PR:

```bash
cd services/mcp
cargo fmt --check
cargo clippy -- -W clippy::all
cargo test
```

Record the tool list and prompt list from a running server. They are part of the public MCP
contract.

### 2. Add contract tests

The existing tests cover local parsing and URL rewriting. Add mock HTTP tests before extracting
clients. Cover:

- Authorization headers sent to Hivemind and Vexa
- Vexa-key resolution through `/vexa/token`
- method, path, query, and body for representative tools
- upstream timeout, non-JSON body, and non-success responses
- `request_meeting_bot` handling of an idempotency `409`

Use a local mock HTTP server. Tests should not depend on Docker, RunPod, or production
credentials.

### 3. Extract upstream clients

Move `hivemind()` into `clients/hivemind.rs` and `gateway()` into `clients/vexa.rs`.
Preserve the current timeout values and request behavior in this step. Return typed errors,
but keep the MCP-visible messages unchanged until tests cover the new mapping.

### 4. Extract authentication
