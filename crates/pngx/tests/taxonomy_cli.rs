//! Integration tests for taxonomy CRUD CLI commands.
//!
//! Each test boots a wiremock server, execs the `pngx` binary with `--url`
//! and `--token` pointing at it, and asserts on stdout/stderr/exit code.
//!
//! Unix-only: we `env_clear()` the child's environment, which on Windows
//! strips the Winsock DLL directory and other essentials, breaking
//! networking in the spawned `pngx` process (`WSAStartup` fails with
//! error 10106). The CLI dispatch logic these tests cover is
//! platform-agnostic; Linux and macOS runners exercise it on CI.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use serde_json::json;
use wiremock::matchers::{body_json_string, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn pngx_bin() -> PathBuf {
    // `tests/*.rs` integration tests get access to the built binary via
    // the CARGO_BIN_EXE_<name> env var.
    PathBuf::from(env!("CARGO_BIN_EXE_pngx"))
}

fn run_pngx(url: &str, args: &[&str]) -> std::process::Output {
    Command::new(pngx_bin())
        .env_clear()
        // Preserve PATH and a few HOME-ish things so the binary's own
        // dependencies (like SSL certs) resolve correctly.
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        )
        .env(
            "HOME",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
        )
        // Bypass the config-file lookup so these tests don't mutate the
        // user's real pngx config.
        .env("PNGX_CONFIG", "/dev/null")
        .arg("--url")
        .arg(url)
        .arg("--token")
        .arg("test-token")
        .args(args)
        .output()
        .expect("run pngx")
}

#[tokio::test]
async fn bare_tags_lists_and_preserves_backcompat() {
    // With no subcommand, `pngx tags` should still list tags (the default
    // action) — preserving the 0.7.x CLI surface. Output flags live on the
    // explicit `list` subcommand; bare `pngx tags` uses defaults.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 1,
                "name": "Steuer",
                "slug": "steuer",
                "color": "#c02020",
                "is_inbox_tag": false,
                "document_count": 3
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(&server.uri(), &["tags"]);
    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Default markdown output contains the tag name.
    assert!(
        stdout.contains("Steuer"),
        "expected 'Steuer' in stdout; got: {stdout}"
    );
}

#[tokio::test]
async fn tags_list_with_json_output() {
    // The modern form: `pngx tags list -o json`. Covers the flag placement
    // decision that output flags live on the List subcommand.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [{
                "id": 1,
                "name": "Steuer",
                "slug": "steuer",
                "color": null,
                "is_inbox_tag": false,
                "document_count": 0
            }]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(&server.uri(), &["tags", "list", "-o", "json"]);
    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed[0]["name"], "Steuer");
}

#[tokio::test]
async fn tags_create_happy_path() {
    let server = MockServer::start().await;
    let request_body = json!({"name": "Hardware", "color": "#3366cc"});
    let response_body = json!({
        "id": 42,
        "name": "Hardware",
        "slug": "hardware",
        "color": "#3366cc",
        "is_inbox_tag": false,
        "document_count": 0
    });
    Mock::given(method("POST"))
        .and(path("/api/tags/"))
        .and(body_json_string(request_body.to_string()))
        .respond_with(ResponseTemplate::new(201).set_body_json(&response_body))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(
        &server.uri(),
        &[
            "tags", "create", "Hardware", "--color", "#3366cc", "-o", "json",
        ],
    );
    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed[0]["id"], 42);
}

#[tokio::test]
async fn correspondents_update_resolves_name_to_id() {
    // `pngx correspondents update "Apple" --match "apple.com"` must look
    // up "Apple" in the list, find a unique match, and PATCH the right ID.
    let server = MockServer::start().await;
    // 1) Resolver call: list all correspondents.
    Mock::given(method("GET"))
        .and(path("/api/correspondents/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 2,
            "next": null,
            "previous": null,
            "results": [
                {"id": 1, "name": "Apple", "slug": "apple", "document_count": 5},
                {"id": 2, "name": "Google", "slug": "google", "document_count": 2}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    // 2) PATCH /api/correspondents/1/
    Mock::given(method("PATCH"))
        .and(path("/api/correspondents/1/"))
        .and(body_json_string(
            json!({"matches": "apple.com"}).to_string(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1,
            "name": "Apple",
            "slug": "apple",
            "document_count": 5
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(
        &server.uri(),
        &[
            "correspondents",
            "update",
            "Apple",
            "--match",
            "apple.com",
            "-o",
            "json",
        ],
    );
    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn document_types_delete_with_yes_in_non_tty_context() {
    let server = MockServer::start().await;
    // Resolver call when user passes a bare integer: none (we pass "9").
    Mock::given(method("DELETE"))
        .and(path("/api/document_types/9/"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(&server.uri(), &["document-types", "delete", "9", "--yes"]);
    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn storage_paths_delete_refuses_without_yes_in_non_tty() {
    // No mocks needed: the command should refuse before hitting the network
    // because stdin is not a TTY under `cargo test` and --yes was not passed.
    let server = MockServer::start().await;
    let out = run_pngx(&server.uri(), &["storage-paths", "delete", "42"]);
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("refusing to delete") || stderr.contains("--yes"),
        "unexpected stderr: {stderr}"
    );
}

#[tokio::test]
async fn tags_delete_unknown_name_exits_not_found_style() {
    // A name that doesn't match any tag should error with exit code 1
    // (anyhow::Error not classified as ApiError NotFound — this surfaces as
    // an anyhow message from the resolver).
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "count": 1,
            "next": null,
            "previous": null,
            "results": [
                {"id": 1, "name": "Steuer", "slug": "steuer", "color": null, "is_inbox_tag": false, "document_count": 0}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(&server.uri(), &["tags", "delete", "Nonexistent", "--yes"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no tag named 'Nonexistent'"),
        "unexpected stderr: {stderr}"
    );
}

#[tokio::test]
async fn documents_upload_without_wait_prints_task_uuid() {
    let server = MockServer::start().await;
    let task_uuid = "aaaaaaaa-1111-2222-3333-bbbbbbbbbbbb";

    Mock::given(method("POST"))
        .and(path("/api/documents/post_document/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(task_uuid))
        .expect(1)
        .mount(&server)
        .await;

    // Use a scratch PDF to upload.
    let tmp_dir = std::env::temp_dir().join(format!("pngx-upload-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let pdf = tmp_dir.join("invoice.pdf");
    std::fs::write(&pdf, b"%PDF-fake").unwrap();

    let out = run_pngx(
        &server.uri(),
        &[
            "documents",
            "upload",
            pdf.to_str().unwrap(),
            "--title",
            "Invoice Jan",
        ],
    );
    std::fs::remove_dir_all(&tmp_dir).ok();

    assert!(
        out.status.success(),
        "exit={:?} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.trim() == task_uuid,
        "expected task UUID on stdout; got: {stdout}"
    );
}

#[tokio::test]
async fn tags_create_duplicate_returns_validation_error_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/tags/"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({"name": ["tag with this name already exists"]})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let out = run_pngx(
        &server.uri(),
        &["--json-errors", "tags", "create", "Duplicate"],
    );
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    // Stderr may contain tracing warnings interleaved. Find the JSON error
    // line (starts with '{').
    let json_line = stderr
        .lines()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("expected a JSON error line in stderr");
    let parsed: serde_json::Value = serde_json::from_str(json_line.trim()).expect("JSON error");
    assert_eq!(parsed["code"], "validation_error");
}
