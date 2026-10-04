//! Public CLI contract: direct and Cargo external-subcommand invocation.
//!
//! The installed binary is `cargo-cleanme`, invocable directly or as
//! `cargo cleanme`. This integration test proves the documented external form
//! works without mutating user Cargo state: it places the built binary in a
//! temporary directory, prepends that directory to `PATH`, and runs
//! `cargo cleanme --help` through Cargo.

use std::{fs, path::PathBuf, process::Command};

fn staged_cargo_cleanme() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temporary PATH directory");
    let current = PathBuf::from(env!("CARGO_BIN_EXE_cargo-cleanme"));
    let file_name = current
        .file_name()
        .expect("binary has a file name")
        .to_owned();
    let staged = dir.path().join(file_name);
    fs::copy(&current, &staged).expect("stage cargo-cleanme binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&staged).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&staged, permissions).unwrap();
    }
    (dir, staged)
}

fn path_with(prefix: &std::path::Path) -> std::ffi::OsString {
    let mut paths = vec![prefix.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).expect("join PATH")
}

#[test]
fn cargo_external_subcommand_help_matches_direct_help() {
    let (_dir_guard, _staged) = staged_cargo_cleanme();
    // Direct invocation of the built binary.
    let direct = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .arg("--help")
        .output()
        .expect("run direct --help");
    assert!(direct.status.success());
    let direct_stdout = String::from_utf8_lossy(&direct.stdout).into_owned();

    // Cargo external-subcommand form: `cargo cleanme --help`.
    let external = Command::new("cargo")
        .args(["cleanme", "--help"])
        .env("PATH", path_with(_dir_guard.path()))
        // Isolate Cargo home so no user state is touched; the help path
        // performs no filesystem mutation anyway.
        .env("CARGO_HOME", _dir_guard.path().join("cargo-home"))
        .output()
        .expect("run cargo cleanme --help");
    assert!(
        external.status.success(),
        "cargo cleanme --help failed: {}",
        String::from_utf8_lossy(&external.stderr)
    );
    let external_stdout = String::from_utf8_lossy(&external.stdout).into_owned();
    assert_eq!(direct_stdout, external_stdout);
    assert!(external_stdout.contains("cargo-cleanme"));
}

#[test]
fn cargo_external_config_edit_help_matches_direct() {
    let (_dir_guard, _staged) = staged_cargo_cleanme();
    let direct = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args(["config", "edit", "--help"])
        .output()
        .expect("run direct config edit --help");
    assert!(direct.status.success());
    let direct_stdout = String::from_utf8_lossy(&direct.stdout).into_owned();
    assert!(direct_stdout.contains("edit"));

    let external = Command::new("cargo")
        .args(["cleanme", "config", "edit", "--help"])
        .env("PATH", path_with(_dir_guard.path()))
        .env("CARGO_HOME", _dir_guard.path().join("cargo-home"))
        .output()
        .expect("run cargo cleanme config edit --help");
    assert!(external.status.success());
    assert_eq!(direct_stdout, String::from_utf8_lossy(&external.stdout));
}

#[test]
fn json_scan_is_one_versioned_document_and_stats_stay_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let run = |stats: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args(["--config", config.to_str().unwrap(), "--no-progress"]);
        if stats {
            command.arg("--stats");
        }
        command.args(["--format", "json", "scan", dir.path().to_str().unwrap()]);
        command.output().unwrap()
    };
    let plain = run(false);
    assert!(
        plain.status.success(),
        "{}",
        String::from_utf8_lossy(&plain.stderr)
    );
    let with_stats = run(true);
    assert!(with_stats.status.success());
    assert_eq!(plain.stdout, with_stats.stdout);
    let output: serde_json::Value = serde_json::from_slice(&plain.stdout).unwrap();
    assert_eq!(output["schema_version"], 1);
    assert_eq!(output["operation"], "scan");
    assert_eq!(output["scope"], "explicit");
    assert!(String::from_utf8_lossy(&with_stats.stderr).contains("scan stats:"));
    assert_eq!(plain.stdout.iter().filter(|b| **b == b'\n').count(), 1);
}

#[test]
fn json_cleanup_emits_the_requested_mode_and_machine_summary() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"json-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "pub fn fixture() {}\n").unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            dir.path().to_str().unwrap(),
            "--dryrun",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["operation"], "clean");
    assert_eq!(json["mode"], "simulate");
    assert_eq!(json["result"]["summary"]["simulated"], 0);
    assert_eq!(output.stdout.iter().filter(|b| **b == b'\n').count(), 1);
}

#[test]
fn json_scope_block_is_emitted_with_nonzero_exit_status() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "this is not valid cargo manifest [\n",
    )
    .unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            dir.path().to_str().unwrap(),
            "--dryrun",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["scope_blocked"], true);
}

#[test]
fn config_edit_uses_fake_editor_process_and_keeps_invalid_edits() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config path with spaces.toml");
    let editor = dir.path().join(if cfg!(windows) {
        "fake editor.ps1"
    } else {
        "fake editor.sh"
    });
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&editor, "#!/bin/sh\nfor last do :; done\ncase \"$CLEANME_EDITOR_MODE\" in invalid) printf 'invalid = [' > \"$last\";; valid) printf '[scan]\\nrecency_seconds = 42\\n' > \"$last\";; fail) exit 17;; esac\nexit 0\n").unwrap();
        let mut mode = fs::metadata(&editor).unwrap().permissions();
        mode.set_mode(0o755);
        fs::set_permissions(&editor, mode).unwrap();
    }
    #[cfg(windows)]
    fs::write(
        &editor,
        r#"param([string]$ConfigPath)
$utf8 = [System.Text.UTF8Encoding]::new($false)
switch ($env:CLEANME_EDITOR_MODE) {
  'invalid' { [System.IO.File]::WriteAllText($ConfigPath, 'invalid = [', $utf8) }
  'valid' { [System.IO.File]::WriteAllText($ConfigPath, "[scan]`nrecency_seconds = 42`n", $utf8) }
  'fail' { exit 17 }
}
exit 0
"#,
    )
    .unwrap();
    let visual = if cfg!(windows) {
        format!(
            "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
            editor.display()
        )
    } else {
        format!("\"{}\" --wait", editor.display())
    };
    let run = |mode: &str| {
        Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
            .args(["--config", config.to_str().unwrap(), "config", "edit"])
            .env("VISUAL", &visual)
            .env("EDITOR", "editor-that-must-not-win")
            .env("CLEANME_EDITOR_MODE", mode)
            .output()
            .unwrap()
    };
    let failed = run("fail");
    assert_eq!(failed.status.code(), Some(17));
    let invalid = run("invalid");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("is invalid"));
    assert_eq!(fs::read_to_string(&config).unwrap(), "invalid = [");
    let valid = run("valid");
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    assert!(String::from_utf8_lossy(&valid.stdout).contains("edited"));
    assert_eq!(
        cargo_cleanme::config::load(&config)
            .unwrap()
            .scan
            .recency_seconds,
        42
    );
    let noop = run("noop");
    assert!(noop.status.success());
    assert_eq!(
        cargo_cleanme::config::load(&config)
            .unwrap()
            .scan
            .recency_seconds,
        42
    );
}
