---
title: "Kioku CLI"
---
The `kioku` CLI is a Rust binary that provides terminal access to the Hivemind API.

## Crates

| Crate | Purpose |
|---|---|
| `cc-cli` | Clap command dispatcher (binary: `kioku`) |
| `cc-kioku` | HTTP client over reqwest |
| `cc-auth` | Auth file management (`~/.config/kioku/auth.json`) |
| `cc-upgrade` | Self-update via GitHub releases |

## Installation

```bash
cargo install --path services/cli/crates/cc-cli
```

Or build from source:

```bash
cd services/cli
cargo build --release
# Binary at target/release/kioku
```

## Commands

### Authentication

```bash
kioku signin                    # email/password or --api-key
kioku signout                   # clear stored credentials
kioku whoami                    # show current user
kioku auth-token                # print stored JWT
kioku auth-key-create           # create long-lived API key
kioku auth-key-list             # list API keys
kioku auth-key-delete <prefix>  # delete an API key
```

### Sessions

```bash
kioku sessions-list             # list sessions
kioku sessions-create --title "Research"  # create a session
kioku sessions-get <id>          # get session details
kioku sessions-delete <id>       # delete a session
```

### Messages

```bash
kioku send <session_id> "What was discussed in the last standup?"
kioku messages <session_id>     # list messages in a session
```

### Knowledge

```bash
kioku knowledge-search "deployment strategy"
kioku knowledge-upload ./report.pdf
kioku knowledge-documents       # list uploaded documents
kioku knowledge-delete <id>     # delete a document
```

### Meetings

```bash
kioku meetings-list             # list all meetings
```

### MCP

```bash
kioku mcp                       # print MCP server config JSON for AI clients
```

### Self-Update

```bash
kioku upgrade-check             # check for new versions
kioku upgrade                   # upgrade to latest
```

## Configuration

| Env Variable | Default | Description |
|---|---|---|
| `KIOKU_SERVER` | `http://localhost:9100` | Hivemind API base URL |

Auth is stored at `~/.config/kioku/auth.json`.
