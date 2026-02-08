# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/lukasmalkmus/pngx/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/lukasmalkmus/pngx/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/lukasmalkmus/pngx/releases/tag/v0.1.0
