---
name: paperless
description: |
  Paperless-ngx document management via the pngx CLI and MCP server. Search,
  list, count, filter, download, upload, tag, and delete documents, tags,
  correspondents, document types, and storage paths.
when_to_use: |
  Triggers on: "paperless", "documents", "invoices", "receipts", "find",
  "search", "count", "download", "upload", "tag", "untag", "archive",
  "file this", "ablegen", "hochladen", "create tag", "create correspondent",
  "delete document", "inbox", "unprocessed", "document type", or any
  document-management query.
user-invocable: true
argument-hint: <search-query>
allowed-tools: Bash(pngx search:*), Bash(pngx inbox:*), Bash(pngx documents list:*), Bash(pngx documents get:*), Bash(pngx documents content:*), Bash(pngx documents open:*), Bash(pngx documents download:*), Bash(pngx documents notes:*), Bash(pngx tags), Bash(pngx tags list:*), Bash(pngx correspondents), Bash(pngx correspondents list:*), Bash(pngx document-types), Bash(pngx document-types list:*), Bash(pngx storage-paths), Bash(pngx storage-paths list:*), Bash(pngx version:*), Bash(pngx auth status:*), Bash(pngx mcp), Read
---

# Paperless Document Management

Search, list, filter, and retrieve documents from a Paperless-ngx instance.

## Interfaces

pngx provides two interfaces:

| Interface | Best for | When to use |
|-----------|----------|-------------|
| **CLI** (`pngx ...`) | Token-efficient agents, scripting, jq pipelines | Default choice for capable agents |
| **MCP** (`pngx mcp`) | Less capable agents, tool-calling workflows | When the agent supports MCP natively |

Both interfaces consume the same API client and return identical data structures.

## Prerequisites

The `pngx` CLI must be installed and configured:

```sh
pngx auth login
# Or:
export PNGX_URL=https://paperless.example.com
export PNGX_TOKEN=your-api-token
```

## Decision Tree

```
Need documents?
  ├─ Unprocessed / inbox → pngx inbox
  ├─ By keyword/content → pngx search "query"
  ├─ Count all / filter by metadata → pngx documents list -a -o json
  └─ By specific ID → pngx documents get ID
```

**`search` vs `documents list`:**

| Use case | Command | Why |
|----------|---------|-----|
| Find by content or keyword | `pngx search "invoice"` | Full-text search across content |
| Count total documents | `pngx documents list -a -o json` | Need the envelope's `total_count` |
| Filter by type, tag, date | `pngx documents list -a -o json` + `jq` | Search can't filter by metadata |
| Browse recent documents | `pngx documents list` | Default sort is by date |

## Limitations

**Flags that DO NOT exist** (do not hallucinate these):

- ~~`--document-type`~~ — no metadata filter flags
- ~~`--tag`~~ — no metadata filter flags
- ~~`--correspondent`~~ — no metadata filter flags
- ~~`--date-from`~~ / ~~`--date-to`~~ — no date range flags
- ~~`--sort`~~ — no sort flags

`inbox`, `search`, and `documents list` only accept `-n`/`--limit`, `-a`/`--all`,
`-o`/`--output`, and `-F`/`--fields`. For metadata filtering, pipe JSON output to `jq`.

## Common Pitfalls

| Wrong | Right | Why |
|-------|-------|-----|
| `pngx documents list -o json \| jq 'length'` | `pngx documents list -o json \| jq '.total_count'` | JSON output is an envelope with `results`, `total_count`, `showing`, `has_more`. `length` counts envelope keys, not documents. |
| `pngx search "Rechnung" --document-type Invoice` | `pngx search "Rechnung"` then filter with `jq` | `--document-type` does not exist. |
| `pngx search "invoice" -a` to count documents | `pngx documents list -a -o json \| jq '.total_count'` | `search` ranks by relevance. `documents list` gives the true count. |

## Workflows

### Search, refine, fetch

Start broad, narrow down, then fetch what you need.

```sh
# 1. Search broadly
pngx search "invoice"

# 2. Refine with a more specific query
pngx search "invoice 2024 energy"

# 3. Fetch details for matching documents
pngx documents get 57 73

# 4. Read the full text content
pngx documents content 57

# 5. Download the file
pngx documents download 57
```

### Browse metadata, then filter

Explore the taxonomy first. This is especially important for non-English
Paperless instances where tags, types, and correspondents may be in the local
language (e.g., German: "Rechnung" not "Invoice").

```sh
# See what tags, correspondents, and types exist
pngx tags
pngx correspondents
pngx document-types

# Search using the actual taxonomy terms
pngx search "Rechnung"
```

**Always check the taxonomy before searching.** If your Paperless instance uses
German (or another language), searching in English will miss documents.

### Metadata filtering with jq

Use `documents list -a -o json` piped to `jq` for metadata-based queries.

```sh
# Count all documents
pngx documents list -a -o json | jq '.total_count'

# Filter by document type
pngx documents list -a -o json | jq '[.results[] | select(.document_type == "Invoice")]'

# Filter by date range (uses jiff ISO 8601 dates)
pngx documents list -a -o json | jq '[.results[] | select(.created >= "2025-01-01" and .created < "2025-02-01")]'

# Combined: type + date range
pngx documents list -a -o json | jq '[.results[] | select(.document_type == "Invoice" and .created >= "2025-01-01" and .created < "2025-02-01")]'

# Count results from a filter
pngx documents list -a -o json | jq '[.results[] | select(.document_type == "Invoice")] | length'
```

### Batch operations

Work with multiple documents at once.

```sh
# Get details for several documents
pngx documents get 42 43 44

# Read content of multiple documents
pngx documents content 42 43

# Open multiple documents in the web UI
pngx documents open 42 43

# Download multiple documents (auto-named from metadata)
pngx documents download 42 43 44

# Download a single document to a specific path
pngx documents download 42 --file invoice.pdf
```

## Commands

### Inbox

List documents that are still in the inbox (unprocessed).

```sh
pngx inbox
pngx inbox -n 5
pngx inbox --all
pngx inbox -o json
```

### Search documents

```sh
pngx search "invoice 2024"
pngx search "invoice 2024" -n 10
pngx search "invoice 2024" --all
```

### List documents

```sh
pngx documents list
pngx documents list -n 10
pngx documents list --all
```

### Get document details

```sh
pngx documents get 42
pngx documents get 42 43 44
```

### Get document content

```sh
pngx documents content 42
pngx documents content 42 43
```

### Open documents in browser

```sh
pngx documents open 42
pngx documents open 42 43
```

### Download documents

```sh
pngx documents download 42
pngx documents download 42 --file invoice.pdf
pngx documents download 42 --original
pngx documents download 42 43 44
```

`--file` can only be used with a single document ID. Multiple documents use
auto-naming from document metadata.

### Notes

List the notes attached to a document. Reading notes is auto-allowed; adding
and removing are permission-prompted writes.

```sh
pngx documents notes 42
pngx documents notes 42 -o json
```

### Browse metadata

```sh
pngx tags
pngx correspondents
pngx document-types
```

Metadata commands always show all items (no pagination flags).

## Pagination

Only `inbox`, `search`, and `documents list` support pagination:

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--limit` | `-n` | 25 | Max results (0 for unlimited) |
| `--all` | `-a` | false | Fetch all results |

For agent workflows, prefer `--limit` to avoid overwhelming context:

```sh
pngx search "query" -n 5
pngx documents list -n 10
```

## Output formats

Use `-o` / `--output` to control output:

- `markdown` — tables (default, best for human consumption)
- `json` — structured JSON (use when piping to `jq`)
- `ndjson` — newline-delimited JSON (one object per line, streamable)

**JSON envelope** (paginated commands only):

```json
{
  "results": [...],
  "total_count": 1523,
  "showing": 10,
  "has_more": true
}
```

**NDJSON format** (paginated commands):

```
{"_meta":true,"total_count":1523,"showing":25,"has_more":true}
{"id":42,"title":"Invoice 2026-01","correspondent":"ACME Corp",...}
{"id":43,"title":"Contract renewal",...}
```

The first line is a metadata header (identified by `_meta: true`). Subsequent
lines are data objects. NDJSON is ideal for streaming and line-by-line processing.

Metadata commands (`tags`, `correspondents`, `document-types`) and multi-ID
commands (`get 42 43`) return plain JSON arrays (or one NDJSON line per item).

## Field filtering

Use `-F` / `--fields` to select specific fields and reduce output size:

```sh
pngx documents list -o json -F id,title
pngx search "invoice" -F id,title,correspondent
pngx tags -o json -F id,name
```

**Valid fields per entity:**

| Entity | Fields |
|--------|--------|
| Documents | `id`, `title`, `correspondent`, `document_type`, `tags`, `created`, `added`, `archive_serial_number`, `original_file_name` |
| Tags | `id`, `name`, `slug`, `color`, `is_inbox_tag`, `document_count` |
| Correspondents | `id`, `name`, `slug`, `document_count` |
| Document Types | `id`, `name`, `slug`, `document_count` |

Field filtering reduces the JSON payload, saving tokens. It also skips metadata
API calls when resolved fields (correspondent, document_type, tags) are not
requested.

## Structured errors

Use `--json-errors` (or `PNGX_JSON_ERRORS=1`) to get machine-parseable errors
on stderr:

```json
{"error": "document not found", "code": "not_found"}
```

**Error codes:** `unauthorized`, `not_found`, `bad_request`,
`validation_error`, `task_pending`, `task_unknown`, `invalid_url`,
`io_error`, `network_error`, `timeout`, `scheme_mismatch`, `server_error`,
`deserialization_error`, `config_error`, `usage_error`, `internal_error`

**Exit codes:** 0 (success), 1 (server/deserialization), 2 (usage/unauthorized),
3 (not found), 4 (I/O/network/timeout/URL), 5 (config error)

## MCP Server

Start the MCP server for tool-calling agents:

```json
{
  "mcpServers": {
    "pngx": {
      "command": "pngx",
      "args": ["mcp"],
      "env": {
        "PNGX_URL": "https://paperless.example.com",
        "PNGX_TOKEN": "your-api-token"
      }
    }
  }
}
```

The server communicates over stdio using JSON-RPC (MCP protocol).

### MCP Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `search` | Search documents by query | `query` (required), `limit` (optional) |
| `inbox` | List inbox documents | `limit` (optional) |
| `documents_list` | List all documents | `limit` (optional) |
| `documents_get` | Get documents by ID | `ids` (required, array) |
| `documents_content` | Get document text content | `id` (required) |
| `documents_notes` | List a document's notes | `id` (required) |
| `tags` | List all tags | (none) |
| `correspondents` | List all correspondents | (none) |
| `document_types` | List all document types | (none) |
| `version` | Get server version | (none) |

Read tools carry `readOnlyHint: true`. Document metadata (correspondent,
type, tags) is resolved to human-readable names. The resolver is cached
for 5 minutes.

## Write operations (permission-prompted)

pngx also supports write operations. These are **not pre-allowed** in this
skill's `allowed-tools`; Claude Code prompts for permission on every call.
When a user says "upload", "tag this document", "create a correspondent",
"delete document X", etc., draft the command, confirm the parameters, and
run it. The permission prompt is the safety net — do not reply "I have
no tools for that."

### Documents

```sh
# Upload a file; with --wait, waits for consumption and prints the new
# document ID. Without --wait, prints the task UUID for async polling.
pngx documents upload invoice.pdf \
  --correspondent "Apple" \
  --document-type "Rechnung" \
  --tags "Steuer,Hardware" \
  --created 2026-01-15 \
  --wait

# Update metadata (title, correspondent, doctype, storage path, tags).
# --add-tag and --remove-tag route through bulk_edit for server-side
# atomicity (preferred over --tags for concurrent-safe edits).
pngx documents update 42 --correspondent "Apple" --add-tag Steuer

# Delete document(s) (requires --yes in non-interactive contexts).
pngx documents delete 42 --yes

# Bulk tag / untag across many documents (atomic per tag server-side).
pngx documents tag 1 2 3 Steuer Hardware
pngx documents untag 1 2 3 Old

# Add or remove a note (list notes with `pngx documents notes <id>`).
# remove-note takes the note ID shown by `pngx documents notes`.
pngx documents add-note 42 "Paid 2026-05-19"
pngx documents remove-note 42 7

# Escape hatch: any bulk_edit method.
pngx documents bulk set_correspondent --ids 1,2,3 --params '{"correspondent":42}'
```

### Taxonomy

```sh
# Tags
pngx tags create "Steuer" --color "#c02020" --matching-algorithm any
pngx tags update "Steuer" --color "#ff0000"
pngx tags delete "Steuer" --yes

# Correspondents
pngx correspondents create "Apple" --match "apple.com" --matching-algorithm any
pngx correspondents update "Apple" --match "*.apple.com"
pngx correspondents delete "Apple" --yes

# Document types
pngx document-types create "Rechnung"
pngx document-types delete 5 --yes

# Storage paths
pngx storage-paths create "Steuer" "{{ correspondent }}/{{ created_year }}"
```

Name-or-ID resolution: `--correspondent Apple` and `--correspondent 42`
both work. Ambiguous names produce a candidate list and exit 2; unknown
names hint at creating the entity first.

### MCP write tools

Every CLI write command has a matching MCP tool. Writes carry
`readOnlyHint: false`; deletes and `documents_bulk_edit` additionally
carry `destructiveHint: true`. Agents using the MCP interface see the
same permission prompt behavior as Bash commands.

Note tools: `documents_add_note` (`{id, note}`) and `documents_delete_note`
(`{id, note_id}`, carries `destructiveHint: true`), alongside the read tool
`documents_notes`.
