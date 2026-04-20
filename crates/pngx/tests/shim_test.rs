//! Integration tests for `bin/pngx`, the Claude Code plugin shim.
//!
//! The shim is a bash script, so these tests spawn it as a subprocess with a
//! carefully controlled environment (empty PATH plus only what each scenario
//! needs) and assert on stdout/stderr/exit code.
//!
//! Only run on Unix; the shim uses `#!/usr/bin/env bash` and is not intended
//! to run on Windows directly (users there install `pngx` via cargo/scoop).

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary directory that cleans itself up on drop.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let base =
            std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = base.join(format!("pngx-shim-test-{name}-{nanos}-{counter}"));
        fs::create_dir_all(&dir).expect("create scratch dir");
        Self { root: dir }
    }

    fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write script");
    let mut perms = fs::metadata(path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod");
}

fn shim_path() -> PathBuf {
    // bin/pngx lives at workspace-root/bin/pngx. This test file lives in
    // crates/pngx/tests/, so go two dirs up from CARGO_MANIFEST_DIR.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .join("bin/pngx")
}

fn setup_plugin_root(scratch: &Scratch, version: Option<&str>) -> PathBuf {
    let plugin_root = scratch.path().join("plugin");
    fs::create_dir_all(plugin_root.join(".claude-plugin")).expect("mkdir claude-plugin");
    let manifest = match version {
        Some(v) => format!(r#"{{"name":"pngx","version":"{v}"}}"#),
        None => r#"{"name":"pngx"}"#.to_string(),
    };
    fs::write(plugin_root.join(".claude-plugin/plugin.json"), manifest).expect("write manifest");
    plugin_root
}

/// Build a clean PATH that includes only the core system bins plus any extras
/// the test needs, and excludes any directory that already contains `pngx`
/// (so the shim's "user-installed" branch doesn't accidentally fire).
fn clean_path(extras: &[&Path]) -> String {
    let mut parts: Vec<String> = extras.iter().map(|p| p.display().to_string()).collect();
    for core in ["/usr/bin", "/bin"] {
        let core_path = Path::new(core);
        if !core_path.join("pngx").exists() {
            parts.push(core.to_string());
        }
    }
    parts.join(":")
}

// =============================================================================
// Scenarios
// =============================================================================

#[test]
fn user_installed_pngx_wins() {
    // With a user-installed pngx on PATH, the shim must exec it rather than
    // falling into the plugin-managed cache or download paths.
    let scratch = Scratch::new("user-win");
    let plugin_root = setup_plugin_root(&scratch, Some("1.2.3"));
    let plugin_data = scratch.path().join("data");

    let user_bin = scratch.path().join("user-bin");
    fs::create_dir_all(&user_bin).unwrap();
    write_executable(
        &user_bin.join("pngx"),
        "#!/usr/bin/env bash\necho FAKE_USER_PNGX\nexit 0\n",
    );

    let output = Command::new(shim_path())
        .env_clear()
        .env("PATH", clean_path(&[user_bin.as_path()]))
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "exit={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("FAKE_USER_PNGX"),
        "expected FAKE_USER_PNGX in stdout; got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn shim_dir_in_path_does_not_self_recurse() {
    // If the user happens to have the plugin's bin/ dir on PATH, the shim
    // must strip it before the user-install lookup so it doesn't re-exec
    // itself forever.
    let scratch = Scratch::new("self-dir-path");
    let plugin_root = setup_plugin_root(&scratch, Some("1.2.3"));
    let plugin_data = scratch.path().join("data");

    let user_bin = scratch.path().join("user-bin");
    fs::create_dir_all(&user_bin).unwrap();
    write_executable(
        &user_bin.join("pngx"),
        "#!/usr/bin/env bash\necho FAKE_USER_PNGX\nexit 0\n",
    );

    // Put the shim's own directory on PATH ahead of the user install.
    let shim_dir = shim_path().parent().expect("shim has parent").to_path_buf();
    let output = Command::new(shim_path())
        .env_clear()
        .env(
            "PATH",
            clean_path(&[shim_dir.as_path(), user_bin.as_path()]),
        )
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "exit={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("FAKE_USER_PNGX"),
        "expected FAKE_USER_PNGX in stdout; got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn missing_plugin_json_fails_fast() {
    // No plugin.json at all: the shim should print a clear error and exit
    // 127 without hanging on a download.
    let scratch = Scratch::new("no-manifest");
    let plugin_root = scratch.path().join("plugin");
    fs::create_dir_all(&plugin_root).unwrap();
    let plugin_data = scratch.path().join("data");

    let output = Command::new(shim_path())
        .env_clear()
        .env("PATH", clean_path(&[]))
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("plugin.json not found"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn missing_version_field_fails_with_clear_error() {
    // plugin.json exists but has no `version` field: must not silently try to
    // download an empty version tag.
    let scratch = Scratch::new("no-version");
    let plugin_root = setup_plugin_root(&scratch, None);
    let plugin_data = scratch.path().join("data");

    let output = Command::new(shim_path())
        .env_clear()
        .env("PATH", clean_path(&[]))
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("could not read version"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cached_binary_is_reused_when_version_matches() {
    // With a matching plugin-managed binary at $CLAUDE_PLUGIN_DATA/bin/pngx,
    // the shim must exec it rather than re-downloading.
    let scratch = Scratch::new("cache-hit");
    let plugin_root = setup_plugin_root(&scratch, Some("9.9.9"));
    let plugin_data = scratch.path().join("data");
    fs::create_dir_all(plugin_data.join("bin")).unwrap();
    write_executable(
        &plugin_data.join("bin/pngx"),
        "#!/usr/bin/env bash\necho FAKE_CACHED_PNGX\nexit 0\n",
    );
    fs::write(plugin_data.join("bin/pngx.version"), "9.9.9").unwrap();

    let output = Command::new(shim_path())
        .env_clear()
        .env("PATH", clean_path(&[]))
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "exit={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("FAKE_CACHED_PNGX"),
        "expected FAKE_CACHED_PNGX in stdout; got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn sed_fallback_works_when_jq_is_unavailable() {
    // Mask `jq` off PATH by excluding any directory that contains it; the
    // shim's sed-based extractor should still parse plugin.json and execute
    // the cached binary.
    let scratch = Scratch::new("no-jq");
    let plugin_root = setup_plugin_root(&scratch, Some("7.7.7"));
    let plugin_data = scratch.path().join("data");
    fs::create_dir_all(plugin_data.join("bin")).unwrap();
    write_executable(
        &plugin_data.join("bin/pngx"),
        "#!/usr/bin/env bash\necho FAKE_CACHED_NOJQ\nexit 0\n",
    );
    fs::write(plugin_data.join("bin/pngx.version"), "7.7.7").unwrap();

    let system_path = std::env::var("PATH").unwrap_or_default();
    let filtered: Vec<String> = system_path
        .split(':')
        .filter(|dir| {
            let p = Path::new(dir);
            !p.join("jq").exists() && !p.join("pngx").exists()
        })
        .map(String::from)
        .collect();
    // Guarantee sed/bash/cat/etc. are still available by unioning in the
    // core system bins (they're already likely in `filtered`, but be defensive).
    let mut path = filtered.join(":");
    for core in ["/usr/bin", "/bin"] {
        if !Path::new(core).join("pngx").exists() && !path.split(':').any(|p| p == core) {
            path = format!("{path}:{core}");
        }
    }

    let output = Command::new(shim_path())
        .env_clear()
        .env("PATH", path)
        .env("HOME", scratch.path())
        .env("CLAUDE_PLUGIN_ROOT", &plugin_root)
        .env("CLAUDE_PLUGIN_DATA", &plugin_data)
        .arg("--version")
        .output()
        .expect("run shim");

    assert!(
        output.status.success(),
        "exit={:?} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("FAKE_CACHED_NOJQ"),
        "expected FAKE_CACHED_NOJQ in stdout; got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}
