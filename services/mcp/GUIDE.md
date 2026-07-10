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

Move `bearer_token_from_context()` and `resolve_vexa_key()` to `auth.rs`. Model credentials
explicitly so they cannot be mixed accidentally:

```rust
pub struct KiokuToken(String);
pub struct VexaApiKey(String);

pub struct CallerCredentials {
    pub kioku_token: KiokuToken,
}
```

The resolver should accept `&KiokuToken` and return `Result<VexaApiKey, McpError>`. Hivemind
calls receive `KiokuToken`; Vexa calls receive `VexaApiKey`. Do not hide this distinction in a
generic string parameter.

### 5. Split tool handlers by domain

Move handlers in this order:

1. Knowledge tools: `search`, `meetings`, `transcript`, `documents`, `session`, and `meeting`.
2. Simple meeting tools: status, list, stop, and update.
3. Recording tools.
4. Composite orchestration: `request_meeting_bot` and `get_meeting_bundle`.

Keep a small `match tool_name` in `tools/mod.rs`. The match selects a handler. It must not
contain HTTP construction, authentication resolution, or composite orchestration.

### 6. Co-locate schemas and input models

The current manual schemas can drift from their `Deserialize` structs. Put each tool's schema,
input type, and handler in the same module. Add a contract test that verifies required fields
and defaults for every public tool.

### 7. Improve configuration last

After behavior is covered, make `Config::from_env()` return `Result<Config>`, parse URLs once,
and fail at startup with actionable messages for invalid configuration. Avoid changing default
URLs without a deployment review.

## Frozen boundary: meeting-link parsing

Do not change `src/parse_meeting_link.rs` or the behavior of `parse_meeting_url()` as part of
this refactor. It contains user-maintained platform-specific rules and already has focused unit
tests. Treat it as a stable dependency of the `parse_meeting_link` tool.

You may move the MCP adapter that calls the parser into `tools/parse_links.rs`, but only when
the following remain identical:

- accepted URLs and rejected URLs
- returned `platform`, `native_meeting_id`, `passcode`, and warnings
- Teams URL hashing and warning behavior
- error messages relied on by MCP clients

Any parser behavior change belongs in its own reviewed change with new tests.

## How a tool should work after the refactor

Each tool handler should follow the same sequence:

1. Deserialize and validate MCP arguments.
2. Obtain the correct credential for its upstream service.
3. Call one typed client method.
4. Convert the result into an MCP `CallToolResult`.

For a simple knowledge tool, the handler should be close to this shape:

```rust
pub async fn search(
    ctx: &ToolContext,
    token: &KiokuToken,
    input: SearchInput,
) -> Result<CallToolResult, McpError> {
    let query = input.query.trim();
    if query.is_empty() {
        return Err(McpError::invalid_input("query cannot be empty"));
    }

    let result = ctx.hivemind.search(token, query, input.limit).await?;
    Ok(json_result(result))
}
```

The handler owns input validation and domain behavior. `HivemindClient::search()` owns the
HTTP method, route, authorization header, timeout, and JSON decoding.

## Recommended types

Use a shared immutable context instead of passing unrelated dependencies separately:

```rust
#[derive(Clone)]
pub struct ToolContext {
    pub hivemind: HivemindClient,
    pub vexa: VexaClient,
    pub credentials: CredentialResolver,
}
```

Keep `reqwest::Client` inside the upstream clients. It is cheap to clone and shares its
connection pool. Do not create a new HTTP client for every tool call.

For errors, begin with a small application enum. Adding `thiserror` is reasonable once the
error cases stabilize.

```rust
pub enum McpError {
    MissingCredentials,
    InvalidInput(String),
    UpstreamTimeout { service: &'static str },
    UpstreamHttp { service: &'static str, status: u16, body: String },
    InvalidUpstreamResponse { service: &'static str },
}
```

Never include a bearer token, API key, cookie, or full internal URL in an error sent to the MCP
client or a log message.

## Testing strategy

### Unit tests

Unit test pure logic with normal `#[test]` tests:

- argument defaults and validation
- URL-rewrite behavior for recording downloads
- conversion from `McpError` to a client-safe result
- tool schema required fields and documented defaults
- meeting-link parsing, without changing its behavior

### HTTP client tests

Use a local mock server such as `wiremock` or `httpmock` as a development dependency. For each
client method, test:

- expected HTTP method and path
- authorization header type and value
- query parameters and JSON body
- success response decoding
- 401, 404, 409, and 500 responses
- timeout and malformed JSON behavior

### MCP contract tests

Start an in-process router and issue MCP requests to `/mcp`. Assert that:

- `tools/list` returns the exact public tool names
- each tool retains its input schema
- missing credentials produce an MCP error result, not a server panic
- known upstream failures remain client-readable

Do not require Docker, RunPod, or real credentials for unit and client tests. Keep full-stack
tests separate from the crate's fast test suite.

## Compatibility checklist

Before merging any refactor PR, compare the old and new service for every public tool:

| Contract | Must remain stable during refactor |
|---|---|
| MCP endpoint | `/mcp` and streamable HTTP behavior |
| Tool and prompt names | Exact names and availability |
| Input schemas | Required fields, defaults, and field names |
| Credential handling | Kioku token for Hivemind; resolved Vexa key for Vexa |
| Upstream routes | Method, path, query, and body |
| Output | JSON fields, idempotency behavior, and useful errors |
| Timeouts | Explicit and equivalent until intentionally changed |

The `request_meeting_bot` `409` response is a compatibility case: it is converted into a soft
success. Preserve that behavior and test it directly.

## Suggested pull requests

### PR 1: establish safety

- Add mock-server tests for one Hivemind tool and one Vexa tool.
- Add a tool-list snapshot or contract assertion.
- Make no production code moves other than test seams.

### PR 2: extract clients

- Add `clients/hivemind.rs` and `clients/vexa.rs`.
- Move only HTTP request construction and response decoding.
- Keep `KiokuMcpService` and its tool dispatch unchanged.

### PR 3: extract authentication and errors

- Add `auth.rs` and `error.rs`.
- Replace stringly typed internal credentials with wrapper types.
- Keep external error text stable where clients may depend on it.

### PR 4: move knowledge tools

- Add `tools/knowledge.rs`.
- Move one tool at a time, with tests after each move.
- Leave meeting-link parsing untouched.

### PR 5: move Vexa tools

- Split simple meeting tools first.
- Move recordings next.
- Move `request_meeting_bot` and `get_meeting_bundle` last because they contain orchestration.

### PR 6: clean the old handler

- Turn `handler.rs` into `app.rs`, or remove it after all callers use the new modules.
- Delete only code proven unreachable by compilation and tests.
- Do not combine this cleanup with new tool features.

## Working method for each change

1. State the behavior you will preserve.
2. Add or update the smallest test that proves it.
3. Move one responsibility.
4. Run format, clippy, and tests.
5. Review the MCP-visible diff: tools, prompts, schemas, and errors.
6. Commit the narrow change.

When a refactor becomes hard to test, the abstraction boundary is usually wrong. Move the
upstream call behind a client boundary rather than adding more generic helper functions.

## Common mistakes to avoid

- Creating a new `reqwest::Client` for every request.
- Passing raw `String` credentials through every layer.
- Combining Hivemind and Vexa clients because both use HTTP.
- Replacing all `Value` payloads with types before knowing their real API contract.
- Changing tool schemas while moving code.
- Returning raw upstream bodies that contain credentials or internal deployment details.
- Making CORS, timeout, or URL-default changes in a mechanical file move.
- Refactoring `parse_meeting_link.rs` together with unrelated tool dispatch work.

## Definition of done

The refactor is complete when:

- `ServerHandler` only handles MCP protocol concerns and delegates to `tools`.
- Hivemind, Vexa, and credential-resolution code have separate tested modules.
- Every public tool has a typed input model or a documented pass-through payload reason.
- The tool and prompt contracts are tested.
- The meeting-link parser is unchanged and its tests still pass.
- `cargo fmt --check`, `cargo clippy -- -W clippy::all`, and `cargo test` pass.
- A diff review shows no unintended API, schema, credential, or deployment changes.
