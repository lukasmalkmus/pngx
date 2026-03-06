# AGENTS.md

## Project

**pngx** is a CLI for [Paperless-ngx](https://docs.paperless-ngx.com), written
in Rust.

**Repository:** https://github.com/lukasmalkmus/pngx

## Architecture

Cargo workspace with two crates:

| Crate | Path | Description |
|-------|------|-------------|
| `pngx` | `crates/pngx` | CLI binary (clap, figment, comfy-table) |
| `pngx-client` | `crates/pngx-client` | API client library (ureq, serde) |

The CLI depends on the client library. The client is a standalone crate with no
CLI-specific dependencies.

## CLI shape

```
pngx [--url URL] [--token TOKEN] [--json-errors] [-v...] COMMAND

├─ auth login [--url URL] [--token TOKEN]
├─ auth logout
├─ auth status
│
├─ inbox [-n LIMIT] [--all] [-o FORMAT] [-F FIELDS]
├─ search QUERY [-n LIMIT] [--all] [-o FORMAT] [-F FIELDS]
│
├─ documents list [-n LIMIT] [--all] [-o FORMAT] [-F FIELDS]
├─ documents get ID... [-o FORMAT] [-F FIELDS]
├─ documents content ID...
├─ documents open ID...
├─ documents download ID... [--original] [--file PATH]
│
├─ tags [-o FORMAT] [-F FIELDS]
├─ correspondents [-o FORMAT] [-F FIELDS]
├─ document-types [-o FORMAT] [-F FIELDS]
│
├─ mcp serve
└─ version
```

**Pagination:** Only `inbox`, `search`, and `documents list` accept `--limit`/`--all`.
Metadata commands (`tags`, `correspondents`, `document-types`) always show all
items.

**Multi-ID:** `get`, `content`, `open`, `download` accept one or more IDs.
`--file` is only valid with a single ID.

**Output:** `-o markdown` (default), `-o json`, or `-o ndjson`, available on
commands that produce formatted output. Paginated commands wrap JSON in an
envelope (`results`, `total_count`, `showing`, `has_more`). NDJSON emits a
`_meta` header line followed by one JSON object per line. Metadata and multi-ID
commands return plain JSON arrays.

**Field filtering:** `-F`/`--fields` selects specific fields (e.g.,
`-F id,title`). Reduces output size and skips unnecessary API calls when
resolved fields (correspondent, document_type, tags) are not requested.

**Structured errors:** `--json-errors` (or `PNGX_JSON_ERRORS=1`) emits
machine-readable errors to stderr as `{"error":"...","code":"..."}`.

**Exit codes:**

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Server or deserialization error |
| 2 | Usage error or unauthorized |
| 3 | Not found |
| 4 | I/O, network, timeout, or URL error |
| 5 | Configuration error |

**MCP server:** `pngx mcp serve` starts an MCP server over stdio for
tool-calling agents. Provides 9 read-only tools matching the CLI commands.

## Build commands

```sh
cargo build                                     # Build all crates
cargo test --all                                # Run all tests
cargo clippy --all-targets -- -D warnings       # Lint
cargo fmt --all --check                         # Check formatting
cargo doc --no-deps --all                       # Build docs
```

### MSRV

Minimum supported Rust version is **1.93** (edition 2024).

## Commit format

```
crates/pngx: the change
crates/pngx-client: the change
crates/{pngx,pngx-client}: shared change
```

Messages should read as: "Modify `path:` to `the change`."

For root-level files:

```
ci: update workflow
readme: add usage section
```

## Dependencies

### pngx (CLI)

- `anyhow` - application error handling
- `clap` - argument parsing with derive macros
- `comfy-table` - terminal table rendering
- `etcetera` - platform config paths
- `figment` - layered configuration (TOML + env)
- `jiff` - date/time formatting
- `open` - open documents in default application
- `rmcp` - MCP server (JSON-RPC over stdio)
- `rpassword` - secure password input
- `schemars` - JSON Schema generation for MCP tool parameters
- `serde` / `serde_json` - serialization
- `tokio` - async runtime for MCP server
- `tracing` / `tracing-subscriber` - structured logging
- `url` - URL parsing

### pngx-client (library)

- `jiff` - date/time types with serde support
- `serde` / `serde_json` - JSON serialization
- `thiserror` - library error types
- `ureq` - synchronous HTTP client
- `url` - URL parsing

### Dev dependencies

- `wiremock` - HTTP mocking for client tests
- `tokio` - async runtime for wiremock

## Changelog

The project maintains a `CHANGELOG.md` following
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) format.

### Format

Entries go under the `[Unreleased]` heading grouped by category. At release
time, unreleased entries move into a versioned section. Use only these
categories (omit empty ones):

- **Added** - new commands, flags, or features
- **Changed** - changes to existing behavior or output
- **Deprecated** - features marked for future removal
- **Removed** - removed commands, flags, or features
- **Fixed** - bug fixes
- **Security** - vulnerability fixes

### Writing entries

- One line per change, concise and descriptive
- Write from the user's perspective, not the developer's
- Use imperative mood (Add, Fix, Remove)

### What to document

- New commands, flags, or features
- Changes to existing command behavior or output
- Breaking changes (flag renames, removed commands, config format changes)
- Bug fixes that affected users
- Security fixes
- Dependency upgrades that change behavior or bump the MSRV

### What NOT to document

- Internal refactors with no user-visible effect
- Code style or formatting changes
- CI/CD pipeline tweaks
- Dependency bumps that don't affect behavior or MSRV
- Documentation-only changes
- Test-only changes

## Skills

The `skills/paperless/` directory contains an agent skill for searching
Paperless-ngx documents via the pngx CLI.
