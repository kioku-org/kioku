# Documentation project instructions

## About this project

- This is a documentation site built on [Mintlify](https://mintlify.com)
- Pages are MDX files with YAML frontmatter
- Configuration lives in `docs.json`

## Terminology

- **Hivemind** — the Rust API server
- **CLI** — the `kioku` binary (Rust, clap)
- **Knowledge** — searchable corpus from meetings + documents
- **Sessions** — conversation containers with messages and traces
- **Meetings** — ingested transcript data (from Vexa bots or manual upload)
- **MCP** — Model Context Protocol integration for AI clients

## Style preferences

- Use active voice and second person ("you")
- Keep sentences concise — one idea per sentence
- Use sentence case for headings
- Bold for UI elements: Click **Settings**
- Code formatting for file names, commands, paths, and code references

## Content boundaries

- Document the public API and CLI commands
- Don't document internal implementation details
- Don't document Vexa internals (separate docs in `services/vexa/docs/`)
