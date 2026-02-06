---
name: paperless-search
description: Search and retrieve documents from a Paperless-ngx instance using the pngx CLI.
user_invocable: true
---

# Paperless Search

Search and retrieve documents from a Paperless-ngx document management system.

## Prerequisites

The `pngx` CLI must be installed and configured:

```sh
pngx auth login
# Or:
export PNGX_URL=https://paperless.example.com
export PNGX_TOKEN=your-api-token
```

## Workflows

### Search, refine, fetch

The primary workflow. Start broad, narrow down, then fetch what you need.

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

Explore the organizational structure first, then search with that context.

```sh
# See what tags, correspondents, and types exist
pngx tags
pngx correspondents
pngx document-types

# Search with knowledge of the taxonomy
pngx search "ARAG Krankenversicherung"
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

Only `search` and `documents list` support pagination:

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

- `markdown` - Markdown tables (default)
- `json` - Structured JSON

Paginated commands (`search`, `documents list`) include a metadata envelope:

```json
{
  "results": [...],
  "total_count": 1523,
  "showing": 10,
  "has_more": true
}
```

Metadata commands and multi-ID commands return plain JSON arrays.

For agent workflows, prefer markdown (default). Use `-o json` when piping to
`jq` or processing programmatically:

```sh
pngx search "invoice" -o json | jq '.results[].title'
pngx documents get 42 -o json | jq '.title'
```
