---
name: paperless
description: |
  Paperless-ngx document management via the pngx CLI. Use when searching,
  listing, counting, filtering, downloading, or retrieving documents from
  Paperless. Triggers on: "paperless", "documents", "invoices", "receipts",
  "find document", "search documents", "how many documents", "download document",
  "document type", "correspondent", "tag", "inbox", "unprocessed", or any
  document management query.
user-invocable: true
argument-hint: <search-query>
allowed-tools: Bash(pngx *)
memory: user
---

# Paperless Document Management

Search, list, filter, and retrieve documents from a Paperless-ngx instance.

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
and `-o`/`--output`. For metadata filtering, pipe JSON output to `jq`.

## Common Pitfalls

| Wrong | Right | Why |
|-------|-------|-----|
| `pngx documents list -o json \| jq 'length'` → `4` | `pngx documents list -o json \| jq '.total_count'` | JSON output is an envelope with `results`, `total_count`, `showing`, `has_more`. `length` counts envelope keys, not documents. |
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

- `markdown` — tables (default, best for agent consumption)
- `json` — structured JSON (use when piping to `jq`)

**JSON envelope** (paginated commands only):

```json
{
  "results": [...],
  "total_count": 1523,
  "showing": 10,
  "has_more": true
}
```

Metadata commands (`tags`, `correspondents`, `document-types`) and multi-ID
commands (`get 42 43`) return plain JSON arrays.
