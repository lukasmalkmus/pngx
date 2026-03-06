---
date: 2026-03-06
topic: agent-native-cli
---

# Agent-Native pngx

## What We're Building

Transition pngx from a human-first CLI to an agent-first CLI, informed by
Justin Poehnelt's "Rewrite Your CLI for AI Agents" recommendations. Agents
become the primary audience. The CLI remains the core interface, but an MCP
server (`pngx mcp serve`) provides a lower-barrier entry point for less capable
agents and non-technical users.

The scope is read-only for now. Write operations (upload, tag, delete) are
future work but the architecture should not preclude them.

## Why This Approach

**Library-first, both surfaces are consumers.** The `pngx-client` crate is
already a standalone library. Both the CLI and the MCP server import it directly
as Rust dependencies. Neither shells out to the other. This avoids process
spawning overhead, string-based error parsing, and double serialization.

Alternatives considered:

- **CLI-first, MCP as shell wrapper:** Simple but fragile. MCP would parse CLI
  stdout, losing typed errors. Two serialization round-trips.
- **MCP-only, deprecate CLI for agents:** Loses the token efficiency of CLI
  output. Capable agents (Claude Code) actually work better with CLIs.

## Key Decisions

- **Single binary:** MCP server lives in the `pngx` crate as `pngx mcp serve`,
  not a separate binary. One artifact to distribute. Can be feature-gated later
  if binary size matters.
- **No schema introspection command:** The CLI surface is small (~12 commands,
  simple flags). A schema command would be a verbose duplicate of `--help`. The
  skill file is the right abstraction for agent discovery.
- **Read-only scope:** No `--dry-run` or write-operation safety rails needed
  yet. Design the MCP tool definitions to accommodate writes later.
- **Agents are the primary audience:** Default output, error formatting, and
  flag design should optimize for predictability over discoverability.

## Changes

### CLI improvements (agent-focused)

- **Field filtering:** `--fields id,title,correspondent` on commands that
  produce structured output. Reduces token consumption for agents that only
  need a subset.
- **Structured JSON errors:** Errors currently go to stderr as human text.
  Add `--output json` awareness to errors so agents get
  `{"error": "...", "code": "..."}` instead of unstructured text.
- **NDJSON option:** For paginated commands, `--output ndjson` streams one
  JSON object per line. Agents can process results incrementally without
  buffering the full array.
- **Exit codes:** Formalize exit codes (0 = success, 1 = application error,
  2 = usage error, 3 = auth error) so agents can branch without parsing
  stderr.

### MCP server (`pngx mcp serve`)

- Stdio JSON-RPC transport (standard MCP protocol).
- Each CLI command maps to an MCP tool (e.g., `search`, `documents_list`,
  `documents_get`, `inbox`, `tags`, `correspondents`, `document_types`).
- Tool definitions include parameter schemas with types, descriptions,
  and constraints.
- Responses are structured JSON (no markdown formatting).
- Auth reuses the existing figment config (TOML file + env vars).

### Skill file updates

- Update the existing skill in `skills/paperless/` to document both
  surfaces (CLI and MCP).
- Add guidance on when to prefer CLI (token efficiency, capable agents)
  vs MCP (simpler integration, less capable agents).

## Open Questions

- Which Rust MCP SDK to use? Evaluate `rmcp` and `mcp-rs` for maturity,
  stdio transport support, and maintenance activity.
- Should field filtering apply to JSON output only, or also affect markdown
  table columns?
- Should NDJSON be a separate format (`-o ndjson`) or a flag (`--stream`)
  combined with `-o json`?

## Next Steps

1. Evaluate Rust MCP SDKs
2. Implement field filtering on existing commands
3. Add structured JSON errors
4. Add NDJSON output option
5. Build MCP server subcommand
6. Update skill file
