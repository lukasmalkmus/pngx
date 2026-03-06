# pngx

[![CI](https://github.com/lukasmalkmus/pngx/actions/workflows/ci.yaml/badge.svg)](https://github.com/lukasmalkmus/pngx/actions/workflows/ci.yaml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

An AI-native command-line interface for
[Paperless-ngx](https://docs.paperless-ngx.com). Designed for both human
operators and AI agents.

## Features

- Search and browse documents, tags, correspondents, and document types
- View, download, and open documents by ID
- Read document content as plain text
- Output as markdown tables, JSON, or NDJSON (streamable)
- Field filtering (`-F id,title`) to reduce output size
- MCP server (`pngx mcp serve`) for tool-calling agents
- Structured JSON errors (`--json-errors`) with machine-readable codes
- Agent-friendly: predictable commands, distinct exit codes, Claude Code plugin

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

Verify the connection:

```sh
pngx auth status
```

Search for documents:

```sh
pngx search "invoice 2024"
```

## Agent integration

pngx ships as a [Claude Code plugin](https://docs.anthropic.com/en/docs/claude-code/plugins)
with a `paperless` skill for document search workflows.

### Install the plugin

```sh
# Add the pngx marketplace
claude plugin marketplace add lukasmalkmus/pngx

# Install the plugin
claude plugin install pngx@pngx
```

Once installed, the skill activates automatically when your prompt mentions
Paperless-ngx documents:

```
Find all invoices from January 2025 in Paperless
```

You can also invoke it explicitly with `/paperless`.

## Usage

```
pngx [--url URL] [--token TOKEN] [-v...] COMMAND
```

| Command | Description |
|---------|-------------|
| `auth login` | Save server URL and API token |
| `auth logout` | Remove saved credentials |
| `auth status` | Show config and verify connection |
| `inbox` | List inbox documents |
| `search QUERY` | Search documents |
| `documents list` | List all documents |
| `documents get ID...` | View document details |
| `documents content ID...` | Show text content |
| `documents open ID...` | Open in the web UI |
| `documents download ID...` | Download document files |
| `tags` | List all tags |
| `correspondents` | List all correspondents |
| `document-types` | List all document types |
| `mcp serve` | Start MCP server over stdio |
| `version` | Show CLI and server version |

`inbox`, `search`, and `documents list` default to 25 results. Use `-n` to limit,
`-n 0` for unlimited, or `--all` to fetch everything. Metadata commands always
show all items.

Use `--url` and `--token` to override credentials per-call. Commands that
produce formatted output accept `-o markdown` (default), `-o json`, or `-o ndjson`.
Use `-F` / `--fields` to select specific fields (e.g., `-F id,title`).

Use `--json-errors` (or `PNGX_JSON_ERRORS=1`) to get structured error output
on stderr with machine-readable error codes.

### MCP server

For tool-calling agents, start the MCP server:

```json
{
  "mcpServers": {
    "pngx": {
      "command": "pngx",
      "args": ["mcp", "serve"],
      "env": {
        "PNGX_URL": "https://paperless.example.com",
        "PNGX_TOKEN": "your-api-token"
      }
    }
  }
}
```

### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Server or deserialization error |
| 2 | Usage error or unauthorized |
| 3 | Not found |
| 4 | I/O, network, timeout, or URL error |
| 5 | Configuration error |

## Configuration

`pngx auth login` writes credentials to `~/.config/pngx/config.toml`.

Alternatively, set environment variables:

```sh
export PNGX_URL=https://paperless.example.com
export PNGX_TOKEN=your-api-token
```

Precedence: flags > environment variables > config file.

## License

MIT - see [LICENSE](LICENSE) for details.
