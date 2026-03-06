---
title: Agent-Native CLI with MCP Server
type: feat
status: active
date: 2026-03-06
origin: docs/brainstorms/2026-03-06-agent-native-cli-brainstorm.md
---

# Agent-Native CLI with MCP Server

## Overview

Transition pngx from a human-first CLI to an agent-first CLI. The CLI remains
the core interface but optimizes for predictability over discoverability. An MCP
server (`pngx mcp serve`) provides a lower-barrier entry point for less capable
agents and non-technical users. Both surfaces consume the `pngx-client` library
directly (see brainstorm: library-first architecture decision).

Scope is read-only. No write operations, no `--dry-run`, no schema
introspection (see brainstorm: surface too small for schema commands).

## Problem Statement / Motivation

Agents are becoming the primary audience for pngx. The current CLI works for
agents via the skill file but has friction points:

- Full JSON responses waste tokens when agents need 2-3 fields
- Errors are unstructured stderr text that agents must regex-parse
- No streaming output for large result sets
- No native MCP integration; agents must shell out via Bash

## Proposed Solution

Four CLI improvements (field filtering, structured errors, NDJSON, exit code
stabilization) plus an MCP server subcommand. Implemented in phases so each
delivers standalone value.

## Technical Approach

### Phase 1: Field Filtering (`--fields`)

Add `--fields id,title,correspondent` to commands that produce structured
output. Reduces token consumption for agents.

**Field naming convention:** Use serde JSON key names (lowercase, underscored).
These are stable, unambiguous, and match what agents see in `-o json` output.
Valid field names per entity type:

| Entity | Valid fields |
|--------|-------------|
| Document | `id`, `title`, `correspondent`, `document_type`, `tags`, `created`, `added`, `archive_serial_number`, `original_file_name` |
| Tag | `id`, `name`, `slug`, `color`, `is_inbox_tag`, `document_count` |
| Correspondent | `id`, `name`, `slug`, `document_count` |
| DocumentType | `id`, `name`, `slug`, `document_count` |

Note: Document fields use display names (`correspondent`, `document_type`,
`tags`) not the raw API names (`correspondent_name`, etc). The resolver maps
these. This is simpler for agents to reason about.

**Behavior:**
- Invalid field names produce a usage error (exit 2) listing valid fields
- `--fields` applies to both JSON and markdown output
- For `DetailView` (single-item), filters which key-value rows appear
- For `Tabular` (list), filters which columns appear
- Scope: commands with `OutputArgs` only. Not `content`, `download`, `open`,
  `auth`, or `version`

**NameResolver optimization:** When `--fields` is specified and none of
`correspondent`, `document_type`, or `tags` are requested, skip the 3 resolver
API calls entirely. This is a significant latency win for agents.

**Implementation:**
- Add `fields: Option<String>` to `OutputArgs` (`-F` / `--fields`)
- Parse into `Vec<String>`, validate against entity-specific allowed lists
- Thread through `print_results` and `print_all` to filter output
- For JSON: use `serde_json::Value` manipulation to drop keys
- For markdown: filter `Tabular::headers()` and `Tabular::row()` by index
- Conditionally skip `NameResolver::fetch()` when resolved fields not needed

**Files:**
- `crates/pngx/src/output.rs` - Add field filtering to format helpers
- `crates/pngx/src/commands/mod.rs` - Thread fields through `print_results`/`print_all`
- `crates/pngx/src/main.rs` - Add `fields` to `OutputArgs`

### Phase 2: Structured JSON Errors

When JSON output is active, emit machine-parseable errors to stderr instead of
plain text.

**Error format:**
```json
{"error": "document not found", "code": "not_found"}
```

**Error code vocabulary** (mapping from `ApiError` variants):

| ApiError variant | Code string | Exit code |
|------------------|-------------|-----------|
| `Unauthorized` | `unauthorized` | 2 |
| `NotFound` | `not_found` | 3 |
| `InvalidUrl` | `invalid_url` | 4 |
| `Io` | `io_error` | 4 |
| `Network` | `network_error` | 4 |
| `Timeout` | `timeout` | 4 |
| `SchemeMismatch` | `scheme_mismatch` | 4 |
| `Server` | `server_error` | 1 |
| `Deserialization` | `deserialization_error` | 1 |
| Non-API (anyhow) | `internal_error` | 1 |
| Clap usage error | `usage_error` | 2 |
| Config missing | `config_error` | 5 |

**Activation:** Global `--json-errors` flag (not tied to `-o json`). Also
activated by `PNGX_JSON_ERRORS=1` env var. This allows agents to get structured
errors from any command, including `download` and `content` which don't accept
`-o`.

**Config errors get exit code 5** (new). Config validation failures ("server URL
not configured") have a different resolution path (run `auth login`) than server
errors. A distinct exit code helps agents branch.

**Multi-ID partial failure:** Maintain current fail-fast behavior in the CLI.
This is simpler, and agents can retry individual IDs. The MCP server will handle
partial failure differently (see Phase 4).

**Implementation:**
- Add `--json-errors` to `Cli` struct (global flag)
- Add `PNGX_JSON_ERRORS` env var support via figment
- Add `error_code()` method to map errors to code strings
- Modify `main()` error handler to check json-errors mode
- Add exit code 5 for config errors (requires distinguishing config errors from
  other anyhow errors, e.g., via a `ConfigError` type)

**Files:**
- `crates/pngx/src/main.rs` - Global flag, error handler
- `crates/pngx/src/config.rs` - `ConfigError` type for distinct exit code

### Phase 3: NDJSON Output

Add `-o ndjson` for paginated commands. Streams one JSON object per line,
reducing memory usage and enabling incremental processing.

**Metadata strategy:** Emit a metadata header line before data lines:
```
{"_meta":true,"total_count":1523,"showing":25,"has_more":true}
{"id":42,"title":"Invoice 2026-01","correspondent":"ACME Corp",...}
{"id":43,"title":"Contract renewal",...}
```

Agents check the first line for `_meta` to get pagination info, then process
data lines. This is a common NDJSON pattern.

**Scope:** NDJSON works for any command that outputs a list:
- Paginated: `inbox`, `search`, `documents list` (with metadata header)
- Non-paginated: `tags`, `correspondents`, `document-types` (metadata header
  with `has_more: false`)
- Single-item: `documents get 42` emits one line (no metadata header)
- Multi-ID: `documents get 42 43` emits one line per item (no metadata header)

**`--fields` interaction:** Field filtering applies per NDJSON line, same as
JSON mode.

**Error mid-stream:** If pagination fails mid-stream (e.g., network error on
page 3), emit the error to stderr (as structured JSON if `--json-errors` is
active). Items already written to stdout are valid. Exit code reflects the
error. Agents can detect incomplete output by comparing items received against
`total_count` from the metadata header.

**Streaming architecture change:** Currently `collect_*` methods buffer all
results before output. NDJSON requires a callback/iterator approach:
- Add `OutputFormat::Ndjson` variant
- For NDJSON, iterate pages from the client, resolve names, and write each item
  immediately
- The client library's pagination API (`paginate` method) already fetches page
  by page; expose this as an iterator or accept a callback

**Implementation:**
- Add `Ndjson` variant to `OutputFormat` enum
- Add `print_ndjson_header()` helper for metadata line
- Modify `print_results` to handle NDJSON streaming
- Consider adding a `for_each_page` or `pages()` iterator to `Client` for
  streaming pagination without buffering
- `NameResolver` fetched once before streaming starts (acceptable latency;
  consistent names across all lines)

**Files:**
- `crates/pngx/src/output.rs` - `Ndjson` variant, streaming helpers
- `crates/pngx/src/commands/mod.rs` - NDJSON path in `print_results`
- `crates/pngx-client/src/client.rs` - Page iterator or callback API (optional)

### Phase 4: MCP Server (`pngx mcp serve`)

Stdio JSON-RPC server using `rmcp` 1.1.0. Each supported command becomes an MCP
tool. Uses `pngx-client` directly.

**Dependency:** `rmcp = { version = "1.1", features = ["server", "macros", "schemars"] }`

Also requires `tokio` (async runtime) and `schemars` (JSON Schema generation
for tool parameters).

**MCP tool inclusion list:**

| Tool name | Maps to | Parameters |
|-----------|---------|------------|
| `search` | `collect_search` | `query` (required), `limit` (optional), `fields` (optional) |
| `inbox` | `collect_inbox_documents` | `limit` (optional), `fields` (optional) |
| `documents_list` | `collect_documents` | `limit` (optional), `fields` (optional) |
| `documents_get` | `document` | `ids` (required, array), `fields` (optional) |
| `documents_content` | `document_content` | `id` (required) |
| `tags` | `collect_tags` | `fields` (optional) |
| `correspondents` | `collect_correspondents` | `fields` (optional) |
| `document_types` | `collect_document_types` | `fields` (optional) |
| `version` | `server_version` | (none) |

**Excluded commands and rationale:**
- `auth login` - Interactive terminal input, inappropriate for MCP
- `auth logout` - Destructive (deletes config file)
- `auth status` - Reads local config, not useful for remote agents
- `documents open` - Launches local browser, inappropriate for MCP
- `documents download` - Writes binary to filesystem; could be added later
  with base64 encoding but out of scope for v1

**Tool naming:** No prefix. The MCP server name is `pngx` which provides
namespace. Tool names are short and match CLI subcommand structure: `search`,
`inbox`, `documents_list`, `documents_get`, etc.

**Tool output format:** JSON, matching the `-o json` CLI output structure.
Paginated tools return the envelope (`results`, `total_count`, `showing`,
`has_more`). `documents_content` returns `{"id": N, "content": "..."}`.
Multi-ID tools return an array of results with per-item errors:
```json
{
  "results": [
    {"id": 42, "title": "Invoice"},
    {"id": 43, "error": "not_found"}
  ]
}
```

**NameResolver caching:** Cache metadata with a 5-minute TTL. The MCP server is
long-running, so re-fetching tags/correspondents/types on every tool call is
wasteful. Cache is lazily initialized on first tool call that needs resolution.
No explicit cache refresh tool (agents can wait for TTL expiry).

**Async/sync boundary:** `pngx-client` uses `ureq` (synchronous). `rmcp` is
async (tokio). Use `tokio::task::spawn_blocking` for each tool handler. This
allows concurrent tool calls without blocking the tokio event loop.

**Auth:** `pngx mcp serve` accepts `--url` and `--token` flags (same as the
CLI global flags) and reads from figment config/env vars. The client is
constructed once at startup. If config is missing, the server fails to start
with a clear error message.

**Lifecycle:** Server runs until stdin EOF or SIGTERM. `rmcp` handles the
JSON-RPC lifecycle including `initialize`/`shutdown`. The server advertises
`tools` capability only (no resources or prompts for v1).

**Implementation:**
- Add `Mcp` variant to `Command` enum with `Serve` subcommand
- Create `crates/pngx/src/commands/mcp.rs` module
- Define a `PngxMcp` struct implementing `rmcp`'s server handler trait
- Each tool is a method with `#[tool]` macro, parameter struct with `schemars`
- `PngxMcp` holds a `Client`, a `RwLock<Option<CachedResolver>>`, and config
- Tool handlers call `spawn_blocking` to run sync client methods
- `main()` detects `Command::Mcp(Serve)` and enters tokio async runtime

**Files:**
- `crates/pngx/Cargo.toml` - Add `rmcp`, `tokio`, `schemars` dependencies
- `crates/pngx/src/commands/mcp.rs` - MCP server implementation
- `crates/pngx/src/commands/mod.rs` - Re-export mcp module
- `crates/pngx/src/main.rs` - `Mcp` command variant, async entrypoint

### Phase 5: Skill File Update

Update `skills/paperless/SKILL.md` to document both surfaces.

**Additions:**
- MCP tool names and parameter schemas
- Decision tree: CLI vs MCP (CLI for token efficiency and capable agents; MCP
  for simpler integration)
- `--fields` flag documentation with valid field names per entity
- `--json-errors` flag
- `-o ndjson` format
- Exit code table
- `allowed-tools` updated to include MCP tool patterns

### Phase 6: Exit Code Stabilization

Document the full exit code table. No code changes needed beyond Phase 2's
addition of exit code 5 for config errors.

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Server error or deserialization error |
| 2 | Usage error or unauthorized |
| 3 | Not found |
| 4 | I/O, network, timeout, or URL error |
| 5 | Configuration error |

Document in: skill file, `--help` epilog, and README.

## System-Wide Impact

- **Interaction graph:** CLI commands call `pngx-client` methods. MCP tools
  call the same methods via `spawn_blocking`. `NameResolver` is shared logic
  but has different lifecycle (per-command in CLI, cached in MCP).
- **Error propagation:** `ApiError` (thiserror) flows to CLI via anyhow, to MCP
  via JSON-RPC error responses. New `ConfigError` type for exit code 5.
- **State lifecycle risks:** MCP server holds a long-lived `Client` and cached
  `NameResolver`. If the Paperless-ngx server restarts or the token is
  rotated, the MCP server will get auth errors until restarted. Acceptable for
  v1.
- **API surface parity:** Field filtering and error codes must work identically
  in CLI and MCP. NDJSON is CLI-only.

## Acceptance Criteria

- [ ] `--fields id,title` on `documents list -o json` returns only those keys
- [ ] `--fields` with invalid field name prints valid fields and exits 2
- [ ] `--fields id,title` skips NameResolver API calls
- [ ] `--json-errors` produces `{"error":"...","code":"..."}` on stderr
- [ ] `PNGX_JSON_ERRORS=1` activates structured errors
- [ ] Config errors exit with code 5
- [ ] `-o ndjson` on `documents list` streams one object per line
- [ ] NDJSON metadata header includes `total_count` and `has_more`
- [ ] `pngx mcp serve` starts and responds to MCP `initialize`
- [ ] All 9 MCP tools are callable and return correct JSON
- [ ] MCP tool errors return proper JSON-RPC error responses
- [ ] MCP NameResolver caches with 5-minute TTL
- [ ] Skill file documents both CLI and MCP surfaces
- [ ] Exit code table documented in skill file and README
- [ ] All existing tests pass; new tests cover field filtering and NDJSON
- [ ] `cargo clippy --all-targets -- -D warnings` passes

## Dependencies & Risks

**Dependencies:**
- `rmcp` 1.1.0 (stable, official SDK, 4.6M downloads)
- `tokio` (new dependency for MCP server async runtime)
- `schemars` (JSON Schema generation for MCP tool parameters)

**Risks:**
- `rmcp` is async-only, requiring tokio. This adds ~300KB to the binary. Could
  be feature-gated (`mcp` feature flag) to keep the CLI lean when MCP is not
  needed. Decision: add the feature flag.
- `ureq` (sync) + `rmcp` (async) boundary requires `spawn_blocking`. This is
  well-understood but adds complexity. Alternative: switch to an async HTTP
  client. Decision: keep `ureq` + `spawn_blocking` for now; the client library
  is sync by design and switching would be a larger change.
- NDJSON streaming requires exposing pagination internals from `pngx-client`.
  The current `collect_*` API buffers everything. Adding a page iterator is a
  minor API addition, not a rewrite.

## Sources & References

- **Origin brainstorm:** [docs/brainstorms/2026-03-06-agent-native-cli-brainstorm.md](docs/brainstorms/2026-03-06-agent-native-cli-brainstorm.md)
  Key decisions: library-first architecture, single binary with `pngx mcp serve`,
  no schema introspection, read-only scope, agents as primary audience.
- **Article:** Justin Poehnelt, "You Need to Rewrite Your CLI for AI Agents"
  (https://justin.poehnelt.com/posts/rewrite-your-cli-for-ai-agents/)
- **MCP SDK:** `rmcp` 1.1.0 ([crates.io](https://crates.io/crates/rmcp),
  [GitHub](https://github.com/modelcontextprotocol/rust-sdk))
- **Existing output code:** `crates/pngx/src/output.rs` (Tabular/DetailView traits)
- **Existing error handling:** `crates/pngx-client/src/error.rs` (ApiError enum)
- **Existing config:** `crates/pngx/src/config.rs` (figment layering)
- **Existing skill:** `skills/paperless/SKILL.md`
