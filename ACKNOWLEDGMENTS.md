# Acknowledgments

Kioku stands on the shoulders of a lot of open-source work. This file credits
the projects whose code, binaries, or libraries are included in or adapted by
this repository, and notes their licenses.

If you believe something here is mis-attributed or missing, please open an
issue — it will be corrected promptly.

---

## Adapted / borrowed code

Portions of this project were adapted from other open-source repositories.
Their original authors retain copyright over the adapted portions, under the
licenses noted below.

- **[Vexa](https://github.com/Vexa-ai/vexa)** by **Vexa-ai** — the open-source
  meeting bot and transcription platform this project was originally built on
  top of. Copyright © Vexa-ai. **Apache License 2.0.** Kioku started as a
  clone of Vexa (`services/vexa`), later flattened directly into `services/`.
  Vexa-derived code makes up the bot/meeting/dashboard layer:
  `services/vexa-bot/`, `services/modules/`, `services/dashboard/`,
  `services/mcp/`, `services/meeting-api/`, `services/admin-api/`,
  `services/agent-api/`, `services/api-gateway/`, `services/router/`,
  `services/cookie/`, `services/runtime-api/`, `services/transcription-service/`,
  and `services/tts-service/`. It has since been extended and rebranded
  (dashboard UI, auth flow, MCP tools) but the architecture and a large share
  of the implementation is Vexa's. The `@vexaai/transcript-rendering` npm
  package and internal `@vexa/*` workspace modules are used as-is from
  upstream. **`services/hivemind/` (the Rust knowledge/vector layer) and
  `services/cli/` are Kioku-original, not derived from Vexa.**
- **[langchain-rust](https://github.com/Abraxas-365/langchain-rust)** by
  **Abraxas-365** — Rust LLM/vectorstore framework. Copyright © Abraxas-365.
  **MIT License.** Hivemind depends on a fork,
  [`coolcmyk/langchain-rust-openrouter`](https://github.com/coolcmyk/langchain-rust-openrouter),
  which adds OpenRouter support on top of the upstream crate (`Cargo.toml`
  in `services/hivemind/`).

---

## Bundled inside the stateful image

`deployment/docker/Dockerfile.stateful` builds a single supervisord-managed
image that downloads and runs these directly (not just composed via Docker
Hub images):

| Component | Source | Purpose | License |
|---|---|---|---|
| [PostgreSQL 16](https://www.postgresql.org/) + [pgvector](https://github.com/pgvector/pgvector) | apt.postgresql.org | Primary datastore + vector column support | PostgreSQL License / PostgreSQL License |
| [Qdrant](https://github.com/qdrant/qdrant) 1.10.1 | GitHub release binary | Vector store for meeting/document knowledge search | Apache-2.0 |
| [Ollama](https://github.com/ollama/ollama) | `ollama.com/install.sh` | Local embedding + LLM serving (`nomic-embed-text-v2-moe`) | MIT |
| [Redis](https://redis.io/) | apt (`redis-server`) | Live transcript streams (XADD/PUBLISH), caching | BSD-3-Clause (Debian-packaged version) |
| [MinIO](https://github.com/minio/minio) | `dl.min.io` binary | S3-compatible object storage for recordings | AGPL-3.0 — see license note below |
| [cloudflared](https://github.com/cloudflare/cloudflared) | GitHub release binary | Cloudflare Tunnel client for exposing services | Apache-2.0 |
| [nginx](https://nginx.org/) (`nginx:alpine`) | Docker Hub image | Reverse proxy in `transcription-service` compose | BSD-2-Clause |

## Rust dependencies

Core crates used by `services/hivemind/` and `services/cli/` (both
MIT-licensed workspaces):

| Crate | Purpose | License |
|---|---|---|
| [axum](https://github.com/tokio-rs/axum) | HTTP server framework | MIT |
| [tokio](https://github.com/tokio-rs/tokio) | Async runtime | MIT |
| [sqlx](https://github.com/launchbadge/sqlx) | Async Postgres client | MIT / Apache-2.0 |
| [qdrant-client](https://github.com/qdrant/rust-client) | Qdrant Rust client | Apache-2.0 |
| [ollama-rs](https://github.com/pepperoni21/ollama-rs) | Ollama Rust client | MIT |
| [rmcp](https://github.com/modelcontextprotocol/rust-sdk) | Official Rust MCP SDK | MIT |
| [jsonwebtoken](https://github.com/Keats/jsonwebtoken) | JWT auth | MIT |
| [bcrypt](https://github.com/Keats/rust-bcrypt) | Password hashing | MIT |
| [pdf-extract](https://github.com/jrmuizel/pdf-extract) | PDF text extraction for document memory | MIT |
| [tower](https://github.com/tower-rs/tower) / [tower-http](https://github.com/tower-rs/tower-http) | Middleware (CORS, tracing) | MIT |
| [reqwest](https://github.com/seanmonstar/reqwest) | HTTP client | MIT / Apache-2.0 |
| [serde](https://github.com/serde-rs/serde) / [serde_json](https://github.com/serde-rs/json) | Serialization | MIT / Apache-2.0 |
| [uuid](https://github.com/uuid-rs/uuid) | UUID generation | MIT / Apache-2.0 |
| [clap](https://github.com/clap-rs/clap) | CLI argument parsing (`services/cli/`) | MIT / Apache-2.0 |
| [crossterm](https://github.com/crossterm-rs/crossterm) | Terminal UI for the CLI's provider selector | MIT |
| [tracing](https://github.com/tokio-rs/tracing) | Structured logging | MIT |

## Python dependencies

Shared across `meeting-api`, `mcp`, `admin-api`, `agent-api`, `api-gateway`,
`router`, `transcription-service`, and `tts-service`:

| Package | Purpose | License |
|---|---|---|
| [FastAPI](https://github.com/tiangolo/fastapi) | Web framework for all Python services | MIT |
| [Uvicorn](https://github.com/encode/uvicorn) | ASGI server | BSD-3-Clause |
| [Starlette](https://github.com/encode/starlette) | ASGI toolkit (FastAPI dependency) | BSD-3-Clause |
| [Pydantic](https://github.com/pydantic/pydantic) | Data validation | MIT |
| [HTTPX](https://github.com/encode/httpx) | Async HTTP client | BSD-3-Clause |
| [SQLAlchemy](https://github.com/sqlalchemy/sqlalchemy) + [asyncpg](https://github.com/MagicStack/asyncpg) | ORM / async Postgres driver | MIT / Apache-2.0 |
| [Alembic](https://github.com/sqlalchemy/alembic) | DB migrations | MIT |
| [redis-py](https://github.com/redis/redis-py) | Redis client | MIT |
| [boto3](https://github.com/boto/boto3) | S3/MinIO client | Apache-2.0 |
| [fastapi-mcp](https://github.com/tadata-org/fastapi_mcp) | Exposes FastAPI routes as MCP tools | MIT |
| [faster-whisper](https://github.com/SYSTRAN/faster-whisper) | Speech-to-text transcription engine | MIT |
| [piper-tts](https://github.com/rhasspy/piper) | Local text-to-speech | MIT |
| [langdetect](https://github.com/Mimino666/langdetect) | Language detection for TTS | MIT |
| [croniter](https://github.com/kiorky/croniter) | Cron schedule parsing (`agent-api`) | MIT |
| [python-multipart](https://github.com/Kludex/python-multipart) | Multipart/form-data parsing | Apache-2.0 |
| [NumPy](https://github.com/numpy/numpy) | Numeric arrays for audio processing | BSD-3-Clause |
| [soundfile](https://github.com/bastibe/python-soundfile) | Audio I/O | BSD-3-Clause |

## Node / dashboard dependencies

`services/dashboard/` (Next.js) and `services/vexa-bot/core/`:

| Package | Purpose | License |
|---|---|---|
| [Next.js](https://github.com/vercel/next.js) | Dashboard framework | MIT |
| [React](https://github.com/facebook/react) | UI library | MIT |
| [NextAuth.js](https://github.com/nextauthjs/next-auth) | Google OAuth sign-in | ISC |
| [Vercel AI SDK](https://github.com/vercel/ai) (`ai`, `@ai-sdk/*`) | LLM chat/agent UI plumbing | Apache-2.0 |
| [Radix UI](https://github.com/radix-ui/primitives) | Accessible UI primitives | MIT |
| [Tiptap](https://github.com/ueberdosis/tiptap) | Rich text editor (notes) | MIT |
| [Zustand](https://github.com/pmndrs/zustand) | Client state management | MIT |
| [Zod](https://github.com/colinhacks/zod) | Schema validation | MIT |
| [Tailwind CSS](https://github.com/tailwindlabs/tailwindcss) | Styling | MIT |
| [Playwright](https://github.com/microsoft/playwright) | Browser automation the bot uses to join Google Meet / Teams | Apache-2.0 |
| [Puppeteer](https://github.com/puppeteer/puppeteer) + [puppeteer-extra-plugin-stealth](https://github.com/berstend/puppeteer-extra) | Browser automation fallback / stealth patches | Apache-2.0 / MIT |
| [@huggingface/transformers](https://github.com/huggingface/transformers.js) | In-process ML (VAD, embeddings) in the bot core | Apache-2.0 |
| [ws](https://github.com/websockets/ws) | WebSocket client/server | MIT |

## Companion services (interoperated with, not bundled)

Kioku talks to these over the network/API. They are **not** distributed with
this project, but deserve credit:

- Meeting platforms: **Google Meet**, **Microsoft Teams**, **Zoom** — bot
  join/capture is built against their web/SDK surfaces.
- Model/API providers, selectable via the dashboard and CLI: **Anthropic**,
  **OpenAI**, **OpenRouter**.
- [Cloudflare Tunnel](https://www.cloudflare.com/products/tunnel/) — used in
  self-hosted deployments to expose `dashboard.*` / `api.*` without opening
  inbound ports.

---

### License-compatibility notes

Kioku's own code is **MIT** (see [`LICENSE`](LICENSE)). Two things are worth
flagging for anyone redistributing a build:

- **Vexa-derived services are Apache-2.0.** Per Apache-2.0 §4, any
  redistribution of `services/vexa-bot/`, `services/dashboard/`,
  `services/mcp/`, and the other Vexa-originated services listed above must
  retain Vexa's copyright and license notices (see
  [`services/vexa-bot/LICENSE`](services/vexa-bot/LICENSE)) and note that
  those files were changed. This does not affect `services/hivemind/` or
  `services/cli/`, which are MIT and Kioku-original.
- **MinIO is AGPL-3.0.** It is bundled as an unmodified binary in the
  stateful image purely as infrastructure (S3-compatible storage for
  recordings), the same way Postgres or Redis are — Kioku's own code does not
  link against it. If you self-host and modify Kioku while keeping MinIO
  bundled, AGPL's network-use clause applies to *MinIO itself*, not to the
  rest of the stack. Swapping in another S3-compatible backend
  (`STORAGE_BACKEND` env var) avoids this entirely.

---

## Thanks to

Large parts of Kioku were written *with* AI models, not just by a human —
credit where it's due:

- **Claude** (Anthropic) — Sonnet 4.6, including 1M-context sessions, paired
  on the bulk of the Vexa integration, Hivemind, and CLI work.
