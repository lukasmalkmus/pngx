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
- MCP server (`pngx mcp`) for tool-calling agents
- Structured JSON errors (`--json-errors`) with machine-readable codes
- Agent-friendly: predictable commands, distinct exit codes, portable agent plugin

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

pngx ships as an [Agent Plugins 1.0](https://agent-plugins.org) package with an [Agent Skills](https://agentskills.io)-compatible `paperless` skill. It also retains native Claude Code packaging.

### Portable clients and Codex

Install this repository through your client's plugin installer. The portable entry points are `plugin.json`, `mcp.json`, and `skills/paperless/SKILL.md`. The MCP descriptor launches `./bin/pngx`, so a separate CLI installation is optional. The launcher prefers an installed CLI, then a matching binary in the client's persistent `PLUGIN_DATA` directory, then downloads the release binary on first use. Background prefetch hooks are optional.

The bundled launcher supports macOS and Linux and needs Bash, curl, and tar. Windows users can install the native CLI separately and configure `pngx mcp` in their host. Configure credentials using `pngx auth login` before starting MCP. The portable descriptor does not inject URL or token placeholders and preserves the CLI's saved configuration. Custom configuration supplied only through environment variables depends on what the host inherits.

OpenAI-specific presentation and hook declarations live under `extensions.com.openai`. A host may require you to review and trust hooks, but MCP startup does not require those hooks.

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

You can also invoke it explicitly with `/pngx:paperless`.

On enable, Claude Code asks for your Paperless-ngx URL and API token and stores
the token in your system keychain. Leave both empty to keep using the
credentials from `pngx auth login`.

### Confirm before writes

The skill lists command-specific read-only preapprovals. This metadata does not restrict tool access or guarantee a permission prompt. Confirm write parameters with the user and follow the host's approval policy. In Claude Code, add explicit ask rules to `~/.claude/settings.json` when you want those commands to prompt:

```json
{
  "permissions": {
    "ask": [
      "Bash(pngx documents upload:*)",
      "Bash(pngx documents update:*)",
      "Bash(pngx documents delete:*)",
      "Bash(pngx documents tag:*)",
      "Bash(pngx documents untag:*)",
      "Bash(pngx documents bulk:*)",
      "Bash(pngx tags delete:*)",
      "Bash(pngx correspondents delete:*)",
      "Bash(pngx document-types delete:*)",
      "Bash(pngx storage-paths delete:*)"
    ]
  }
}
```

A plugin cannot ship permission rules, so these have to live in your own
settings.

### Validate and release plugin changes

Run `uv run --script scripts/validate_plugin.py` to check the pinned Agent Plugins schemas, skill metadata, and synchronized versions. Run `cargo build --bin pngx`, then `PLUGIN_TEST_BINARY=target/debug/pngx python3 scripts/test_plugin.py` for isolated launcher and real MCP handshake tests. These tests do not contact a Paperless server.

When preparing a release, update the crate, root plugin manifest, Claude manifest, and Claude marketplace versions together. Review the release commit and pass CI on its branch. After publication is authorized, tag that exact commit and let the release workflow publish every supported binary archive. Verify the assets before promoting the version-bumped package to marketplace `main`, then refresh installed plugins. Never overwrite a published tag or advertise a version before its binary downloads exist.

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
| `mcp` | Start MCP server over stdio |
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
      "args": ["mcp"],
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
