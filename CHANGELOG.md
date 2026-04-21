# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0] - 2026-04-21

### Added

- Ship `bin/pngx` shim so the Claude Code plugin works without a separately
  installed `pngx` binary. The shim prefers any user-installed `pngx` on
  `PATH`; otherwise it reuses (or lazily downloads) a plugin-managed copy
  from GitHub releases matching the plugin's declared version.
- Add `SessionStart` hook (`hooks/ensure-binary.sh`) that keeps the
  plugin-managed binary in sync in the background, never blocking session
  startup.
- Add `pngx tags create|update|delete`, `pngx correspondents create|update|delete`,
  `pngx document-types create|update|delete`, and a new `pngx storage-paths`
  category with `list|create|update|delete`. All accept `ID` or exact name
  as input; ambiguous names surface a candidate list.
- Add `ApiError::BadRequest` and `ApiError::ValidationError` variants and
  thread them through exit codes (both → 2) and `--json-errors` codes
  (`bad_request`, `validation_error`).
- Add `pngx documents upload FILE [--title] [--created] [--correspondent]
  [--document-type] [--tags] [--storage-path] [--asn] [--wait]
  [--wait-timeout]`. Without `--wait`, prints the consumption task UUID;
  with `--wait`, polls `/api/tasks/` until the task completes and prints
  the created document ID. The upload body streams directly from disk so
  multi-hundred-megabyte scans don't double in memory.
- Add `ApiError::TaskPending` (wait timed out) and `ApiError::TaskUnknown`
  (task not visible after repeated polls, likely reaped by Celery) with
  `--json-errors` codes `task_pending` and `task_unknown`; both map to
  exit code 4.
- Add `pngx documents update ID` with `--title`, `--created`,
  `--correspondent`, `--document-type`, `--storage-path`, `--tags`
  (replace-all), `--add-tag`/`--remove-tag` (atomic via `bulk_edit`), and
  `--asn`. Per-tag edits route through `bulk_edit` to avoid the
  read-modify-write race that `--tags` still carries.
- Add `pngx documents delete ID...` (with `--yes` for non-interactive
  contexts), `pngx documents tag IDS... TAGS...`,
  `pngx documents untag IDS... TAGS...`, and a generic
  `pngx documents bulk METHOD --ids ... --params '{...}'` escape hatch.

- Mirror every new CLI write command as an MCP tool: `documents_upload`,
  `documents_update`, `documents_delete`, `documents_tag`,
  `documents_untag`, `documents_bulk_edit`, plus `{tags,correspondents,
  document_types,storage_paths}_{create,update,delete}` and a
  `storage_paths` read tool. Writes carry `readOnlyHint: false`; every
  `*_delete` tool and `documents_bulk_edit` additionally carry
  `destructiveHint: true`.

### Changed

- Restructure the `tags`, `correspondents`, and `document-types` commands
  into subcommand groups. Bare `pngx tags`, `pngx correspondents`, and
  `pngx document-types` still list (default action); output flags (`-o`,
  `-F`) now live on the explicit `list` subcommand.
- Narrow the plugin's pre-allowed `settings.json` permissions to read
  verbs only. Every mutating `pngx` verb now appears in the `ask` list —
  Claude Code prompts for permission on each invocation rather than
  pre-approving. Users may still promote commands to `allow` in their own
  runtime settings.
- Tighten `skills/paperless/SKILL.md` `allowed-tools` frontmatter to an
  explicit list of read verbs. Writes are documented in prose and rely on
  the `ask` list for per-call approval.

## [0.7.1] - 2026-03-07

### Fixed

- Bump Claude Code plugin version to match release

## [0.7.0] - 2026-03-07

### Added

- Add `-F`/`--fields` flag for field filtering across all output commands
- Add `-o ndjson` output format for streamable newline-delimited JSON
- Add `--json-errors` flag and `PNGX_JSON_ERRORS` env var for structured error
  output on stderr
- Add `pngx mcp serve` command for MCP (Model Context Protocol) server over
  stdio with 9 read-only tools
- Add exit code 5 for configuration errors (missing URL or token)

### Fixed

- Emit structured JSON/NDJSON output for empty search and inbox results instead
  of a plain text message

## [0.6.3] - 2026-02-23

### Fixed

- Use session ID instead of parent PID for nudge hook deduplication marker

## [0.6.2] - 2026-02-20

### Added

- Auto-approve paperless skill invocation in plugin permissions

## [0.6.1] - 2026-02-20

### Fixed

- Fix plugin version not being picked up from marketplace cache

## [0.6.0] - 2026-02-20

### Added

- Plugin settings with default permissions for pngx commands
- Memory support for paperless skill

### Changed

- Migrate nudge hook from PreToolUse workaround to PostToolUse

## [0.5.1] - 2026-02-09

### Fixed

- Fix plugin hook using PostToolUse additionalContext which is not implemented for built-in tools

## [0.5.0] - 2026-02-09

### Added

- Plugin hook that nudges agents to use the paperless skill when running pngx commands directly

## [0.4.0] - 2026-02-08

### Added

- `auth status` shows authenticated user display name

### Fixed

- Fix `version` and `auth status` failing on Paperless-ngx 2.x (nested API response)

## [0.3.0] - 2026-02-08

### Added

- `inbox` command to list unprocessed documents
- `version` command shows Paperless-ngx server version when configured
- `auth status` verifies server connection and shows server version

### Changed

- `version` command errors (exit 4) when configured server is unreachable
- `-o`/`--output` flag only shown on commands that produce formatted output

## [0.2.0] - 2026-02-08

### Changed

- Bump MSRV to 1.93 (Rust edition 2024)
- Upgrade etcetera to 0.11

## [0.1.0] - 2026-02-07

### Added

- `pngx-client` crate: API client library for Paperless-ngx
- `pngx` crate: CLI binary with search, list, download commands
- Configuration via TOML file, environment variables, and CLI flags
- Multiple output formats: table, JSON, Markdown, plain
- CI workflow with fmt, clippy, test (stable + MSRV 1.85), and docs
- Release workflow with cross-compiled binaries
- Agent skill for Paperless-ngx document search

[Unreleased]: https://github.com/lukasmalkmus/pngx/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/lukasmalkmus/pngx/compare/v0.7.1...v0.8.0
[0.7.1]: https://github.com/lukasmalkmus/pngx/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/lukasmalkmus/pngx/compare/v0.6.3...v0.7.0
[0.6.3]: https://github.com/lukasmalkmus/pngx/compare/v0.6.2...v0.6.3
[0.6.2]: https://github.com/lukasmalkmus/pngx/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/lukasmalkmus/pngx/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/lukasmalkmus/pngx/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/lukasmalkmus/pngx/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/lukasmalkmus/pngx/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/lukasmalkmus/pngx/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/lukasmalkmus/pngx/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/lukasmalkmus/pngx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lukasmalkmus/pngx/releases/tag/v0.1.0
