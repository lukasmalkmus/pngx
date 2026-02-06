# pngx

[![CI](https://github.com/lukasmalkmus/pngx/actions/workflows/ci.yaml/badge.svg)](https://github.com/lukasmalkmus/pngx/actions/workflows/ci.yaml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A command-line interface for [Paperless-ngx](https://docs.paperless-ngx.com).

## Features

- Search and browse documents, tags, correspondents, and document types
- View, download, and open one or more documents by ID
- Read document content as plain text
- Output as markdown tables or JSON
- Agent-friendly design for integration with AI assistants

## Installation

### From source

```sh
cargo install --git https://github.com/lukasmalkmus/pngx
```

### From GitHub releases

Download a prebuilt binary from the
[releases page](https://github.com/lukasmalkmus/pngx/releases).

## Quick start

Configure your Paperless-ngx instance:

```sh
pngx auth login
```

Or set environment variables:

```sh
export PNGX_URL=https://paperless.example.com
export PNGX_TOKEN=your-api-token
```

Verify the connection:

```sh
pngx auth status
```

Search for documents:

```sh
pngx search "invoice 2024"
```

## Workflows

### Search, refine, fetch

The primary workflow: search broadly, narrow down, then fetch what you need.

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
# See what tags exist
pngx tags

# See correspondents
pngx correspondents

# See document types
pngx document-types

# Now search with knowledge of the taxonomy
pngx search "ARAG Krankenversicherung"
```

### Batch operations

Work with multiple documents at once.

```sh
# Get details for several documents
pngx documents get 42 43 44

# Open multiple documents in the web UI
pngx documents open 42 43

# Download multiple documents (auto-named from metadata)
pngx documents download 42 43 44

# Download a single document to a specific path
pngx documents download 42 --file invoice.pdf
```

## Usage

```
pngx [--url URL] [--token TOKEN] [-o FORMAT] [-v...] COMMAND
```

### Commands

| Command | Description |
|---------|-------------|
| `auth login` | Save server URL and API token |
| `auth logout` | Remove saved credentials |
| `auth status` | Show current configuration |
| `search QUERY` | Search documents |
| `documents list` | List all documents |
| `documents get ID...` | View document details |
| `documents content ID...` | Show text content |
| `documents open ID...` | Open in the web UI |
| `documents download ID...` | Download document files |
| `tags` | List all tags |
| `correspondents` | List all correspondents |
| `document-types` | List all document types |
| `version` | Show version information |

### Global options

| Option | Env var | Description |
|--------|---------|-------------|
| `--url URL` | `PNGX_URL` | Paperless-ngx server URL |
| `--token TOKEN` | `PNGX_TOKEN` | API authentication token |
| `-o, --output FORMAT` | | Output format (`markdown`, `json`) |
| `-v, --verbose` | | Increase verbosity (-v, -vv, -vvv) |

### Pagination

`search` and `documents list` default to 25 results:

```sh
pngx search "tax" -n 10        # Limit to 10 results
pngx documents list --all      # Fetch all documents
```

Metadata commands (`tags`, `correspondents`, `document-types`) always show all
items — no pagination flags needed.

### Multi-ID support

`get`, `content`, `open`, and `download` accept one or more IDs:

```sh
pngx documents get 42 43 44
pngx documents content 42 43
pngx documents open 42 43
pngx documents download 42 43 44
```

`--file` is only valid with a single ID. Multiple downloads use auto-naming
from document metadata.

## Configuration

pngx reads configuration from (in order of precedence):

1. Command-line flags
2. Environment variables (`PNGX_URL`, `PNGX_TOKEN`)
3. Configuration file

The configuration file is located at:

| Platform | Path |
|----------|------|
| Linux | `~/.config/pngx/config.toml` |
| macOS | `~/Library/Application Support/pngx/config.toml` |

Example `config.toml`:

```toml
url = "https://paperless.example.com"
token = "your-api-token"
```

## Output formats

Default output is markdown tables. Use `-o json` for structured output.

Paginated commands (`search`, `documents list`) wrap JSON in an envelope:

```json
{
  "results": [...],
  "total_count": 1523,
  "showing": 10,
  "has_more": true
}
```

Metadata commands and multi-ID commands return plain JSON arrays.

## License

MIT - see [LICENSE](LICENSE) for details.
