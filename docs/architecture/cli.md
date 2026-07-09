---
title: "Kioku CLI"
---
The `kioku` CLI is a Rust binary that provides terminal access to Hivemind, Vexa (meetings), and Google Calendar.

## Crates

| Crate | Purpose |
|---|---|
| `cc-cli` | Clap command dispatcher (binary: `kioku`) |
| `cc-kioku` | HTTP client over reqwest |
| `cc-auth` | Auth file management (`~/.config/kioku/auth.json`, `~/.config/kioku/google-calendar.json`) |
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

## Global flags

These apply to every command:

| Flag | Purpose |
|---|---|
| `--server <url>` | Override the server URL for this invocation |
| `--json` | Print machine-readable JSON instead of formatted text |
| `-t`, `--token` | Print the stored auth token and exit (can't be combined with a subcommand) |
| `-v`, `--verbose` | Increase log verbosity (repeatable) |

## Commands

### Auth

```bash
kioku signin                 # opens a browser: pick Google or GitHub with ← → / h l
kioku signin --api-key <key> # sign in with a long-lived API key instead of OAuth
kioku signout                # clear stored credentials
kioku whoami                 # show current user, role, and server
```

Signing in with Google also requests Google Calendar access in the same OAuth round trip
(used by `kioku cal`) — there's no separate connect step. GitHub sign-in can't grant
Calendar scope, so `kioku cal` will prompt to connect Google separately the first time
it's used after a GitHub sign-in.

### Knowledge

```bash
kioku search "deployment strategy"          # search your knowledge base
kioku search "deployment strategy" --limit 10

kioku docs                    # list documents
kioku docs ./report.pdf       # upload a document (PDF, DOCX, PPTX, TXT, or MD)
kioku docs --delete <id>      # delete a document (accepts an unambiguous id prefix)
```

### Meetings

```bash
kioku meet                          # list running bots
kioku meet <link>                   # join a meeting (Google Meet, Zoom, or Teams — auto-detected)
kioku meet --kill <bot-id-or-prefix>  # stop a running bot
kioku meet --transcript <meeting-id>  # print a meeting's transcript
```

### Google Calendar

```bash
kioku cal              # today's meetings
kioku cal --week       # the coming week
kioku cal --date DD/MM/YYYY
```

### API keys

Long-lived API keys for scripting/CI, scoped to your active workspace.

```bash
kioku keys                       # list your API keys
kioku keys --create              # create a key named "cli-key"
kioku keys --create --name ci    # create a key with a custom name
kioku keys --delete <id-or-prefix>
```

The raw key is only printed once, at creation time. Use it with `kioku signin --api-key <key>`.

### Workspaces

A Kioku user can belong to and switch between multiple workspaces.

```bash
kioku ws                          # list your workspaces (* marks the active one)
kioku ws <name-or-slug>           # switch the active workspace
kioku ws --create "New Team"      # create a workspace and switch to it
kioku ws <name> --invite <email>  # invite an email into that (non-active) workspace
```

### Teammates

```bash
kioku invite                 # list pending invites for the active workspace
kioku invite <email>         # invite a teammate (blocked on the free tier)
kioku invite --revoke <id>   # revoke a pending invite
```

### Tools

```bash
kioku mcp            # print MCP server config JSON for AI clients
kioku upgrade         # check for updates and upgrade if a newer version is available
kioku completions bash  # generate a shell completion script (bash/zsh/fish/...)
```

<Note>
  `kioku mcp` currently prints **two** server entries — `Kioku` (pointed at `{server}/mcp`)
  and `Kioku Meetings` (pointed at the dedicated meeting-MCP host/port). Since the backend
  MCP servers were consolidated into one service that hosts every tool (see
  [MCP overview](/mcp/overview)), the `Kioku` entry's URL is currently stale — use the
  `Kioku Meetings` entry (or hit the unified server directly) for both knowledge and
  meeting tools until this is fixed.
</Note>

### Hidden / power-user

Not shown in `--help`, but functional:

```bash
kioku register-admin --workspace-name "Acme" --email admin@acme.com --name "Admin" --password ...
# Bootstraps the first admin account on a self-hosted server. Refuses to run against
# api.kioku.chat — sign in at dashboard.kioku.chat instead.

kioku auth-token   # print the stored JWT (same as `kioku --token`)
```

## Configuration

| Env var | Default | Description |
|---|---|---|
| `KIOKU_SERVER` | `https://api.kioku.chat` | Server URL (overridden by `--server`) |
| `KIOKU_DASHBOARD` | derived from the server URL | Dashboard URL used for the `kioku signin` OAuth handoff |

Dashboard URL resolution (`resolve_dashboard_url`): if `KIOKU_DASHBOARD` is set, use it;
if the server is `api.kioku.chat`, use `https://dashboard.kioku.chat`; if the server is
`localhost:<port>`/`127.0.0.1:<port>`, use the same host on port `3001`; otherwise fall
back to `https://dashboard.kioku.chat`.

Auth is stored at `~/.config/kioku/auth.json` (server URL, token, user/workspace info,
and the workspace selected via `kioku ws`). Google Calendar tokens are stored separately
at `~/.config/kioku/google-calendar.json`.

## `kioku signin` OAuth flow

1. An animated provider selector (crossterm) lets you pick Google or GitHub with `← →`/`h l`.
2. The CLI opens a loopback TCP listener on a random port and opens your browser to the
   dashboard's `/cli-auth?port=<port>&state=<uuid>&provider=<google|github>`.
3. The dashboard checks your session (signs you in via that provider if needed), then
   redirects to `http://localhost:<port>/callback?token=...&state=...&user_id=...&email=...&name=...&workspace_id=...&role=...`
   — for Google, it also runs a Calendar-consent round trip first and appends
   `google_access_token`/`google_refresh_token`/`google_token_expires_at`.
4. The CLI validates the CSRF `state`, saves the `AuthFile` (and `GoogleCalendarAuth` if
   present), and shows a success page in the browser.

## `kioku upgrade`

Checks the latest GitHub release for `kioku-org/kioku`, compares against the running
binary's version, and — if newer — downloads the right platform asset, renames the
current executable to `.old`, writes the new binary in its place, and restores the
executable bit. Prints "Already up to date" if there's nothing to do.
