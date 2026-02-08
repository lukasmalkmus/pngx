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
pngx [--url URL] [--token TOKEN] [-o FORMAT] [-v...] COMMAND

├─ auth login [--url URL] [--token TOKEN]
├─ auth logout
├─ auth status
│
├─ search QUERY [-n LIMIT] [--all]
│
├─ documents list [-n LIMIT] [--all]
├─ documents get ID...
├─ documents content ID...
├─ documents open ID...
├─ documents download ID... [--original] [--file PATH]
│
├─ tags
├─ correspondents
├─ document-types
└─ version
```

**Pagination:** Only `search` and `documents list` accept `--limit`/`--all`.
Metadata commands (`tags`, `correspondents`, `document-types`) always show all
items.

**Multi-ID:** `get`, `content`, `open`, `download` accept one or more IDs.
`--file` is only valid with a single ID.

**Output:** `-o markdown` (default) or `-o json`. Paginated commands wrap JSON
in an envelope (`results`, `total_count`, `has_more`). Metadata and multi-ID
commands return plain JSON arrays.

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
- `rpassword` - secure password input
- `serde` / `serde_json` - serialization
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

## Skills

The `skills/paperless/` directory contains an agent skill for searching
Paperless-ngx documents via the pngx CLI.
