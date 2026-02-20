# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/lukasmalkmus/pngx/compare/v0.6.2...HEAD
[0.6.2]: https://github.com/lukasmalkmus/pngx/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/lukasmalkmus/pngx/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/lukasmalkmus/pngx/compare/v0.5.1...v0.6.0
[0.5.1]: https://github.com/lukasmalkmus/pngx/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/lukasmalkmus/pngx/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/lukasmalkmus/pngx/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/lukasmalkmus/pngx/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/lukasmalkmus/pngx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lukasmalkmus/pngx/releases/tag/v0.1.0
