---
title: Document Notes (list, add, remove)
type: feat
status: completed
date: 2026-05-20
---

# Document Notes (list, add, remove)

## Overview

Add first-class support for Paperless-ngx document notes to pngx across all three
surfaces (client library, CLI, MCP server), plus the skill and plugin
permissions. Before this, notes were only reachable by hitting
`/api/documents/{id}/notes/` with raw `curl` and a token pulled from the config
file. This closes that gap and follows the read/write split established by the
v0.8.0 write commands.

Released as **0.9.0** (new feature, minor bump).

## Problem Statement / Motivation

The classification workflow that files documents into Paperless writes a note on
every miss ("correspondent X not in taxonomy"). Reading those notes to triage
inbox/TODO documents, and adding/removing notes by hand, required raw API calls
because pngx had no notes surface. Notes are a core per-document sub-resource and
belong in the CLI and MCP tool set alongside tags and metadata.

## Paperless API

```
GET    /api/documents/{id}/notes/            -> [ Note ]
POST   /api/documents/{id}/notes/   {note}   -> updated [ Note ]
DELETE /api/documents/{id}/notes/?id={note}  -> updated [ Note ]
```

`Note` shape (verified live on 2.20.15):

```json
{ "id": 6, "note": "text", "created": "2026-05-18T19:00:22+02:00",
  "user": { "id": 3, "username": "lukasmalkmus", "first_name": "Lukas", "last_name": "Malkmus" } }
```

POST and DELETE return the full updated notes list (200), not 204. Confirmed by a
live add+delete smoke test during implementation.

## Technical Approach

### pngx-client

- `Note` + nested `NoteUser` types in `types.rs` (`#[non_exhaustive]`, serde
  derive), exported from `lib.rs`.
- `document_notes(id) -> Vec<Note>` (reuses `get`).
- `add_note(id, &str) -> Vec<Note>` (reuses `post_json`, body `{"note": text}`).
- `delete_note(doc_id, note_id) -> Vec<Note>` (DELETE with `?id=`; decodes the
  returned list).
- wiremock unit tests: list, add (asserts request body), delete (asserts query
  param), not-found.

### CLI

Flat verbs under `documents`, matching the `tag`/`untag` style. Flat (not a
nested `notes {list,add,rm}` group) so the permission prefixes separate cleanly:
`Bash(pngx documents notes:*)` must match only the read path.

- `pngx documents notes <id>` — list, with `-o`/`-F`.
- `pngx documents add-note <id> "text"` — add.
- `pngx documents remove-note <id> <note-id>` — remove.
- `Tabular` + `FieldNames` for `Note` in `output.rs` (columns: ID, Note, Created,
  User). Integration tests in `tests/taxonomy_cli.rs`.

### MCP

Agent-native parity — every CLI write gets an MCP tool:

- `documents_notes` (`read_only_hint = true`)
- `documents_add_note` (`read_only_hint = false`)
- `documents_delete_note` (`read_only_hint = false, destructive_hint = true`)

### Skill / plugin / docs

- `skills/paperless/SKILL.md`: new `## Notes` section (read + write), read verb
  added to `allowed-tools`, MCP tools table updated.
- `.claude-plugin/settings.json`: `Bash(pngx documents notes:*)` -> `allow`;
  `add-note` / `remove-note` -> `ask`.
- `AGENTS.md`: CLI-shape tree + MCP description.

## Acceptance Criteria

- [ ] `pngx documents notes <id>` lists notes (markdown/json/ndjson, `-F`).
- [ ] `pngx documents add-note <id> "text"` adds a note and reports success.
- [ ] `pngx documents remove-note <id> <note-id>` removes a note.
- [ ] NotFound on unknown document/note exits 3.
- [ ] MCP exposes `documents_notes`, `documents_add_note`,
      `documents_delete_note` with correct hints.
- [ ] SKILL.md documents notes; read verb in `allowed-tools`; writes in `ask`.
- [ ] New wiremock + integration tests; all existing tests pass.
- [ ] `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass.

## Release

Minor bump to 0.9.0. Bump `crates/pngx/Cargo.toml`,
`crates/pngx-client/Cargo.toml`, `Cargo.lock` (both entries),
`.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`
(`plugins[0].version`), and move the CHANGELOG `[Unreleased]` entries into
`[0.9.0]`. Commit `release v0.9.0`, tag `v0.9.0`, push the tag. `plugin.json`
version must equal the tag minus `v` (the shim builds its download URL from it).

## Sources & References

- Existing patterns: `crates/pngx-client/src/client.rs` (CRUD helpers),
  `crates/pngx/src/commands/{documents,mcp}.rs`, `output.rs` (Tabular/FieldNames).
- Precedent: `docs/plans/2026-03-06-feat-agent-native-cli-plan.md` (v0.7.0/0.8.0).
