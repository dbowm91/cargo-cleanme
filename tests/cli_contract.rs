//! Public CLI contract: direct and Cargo external-subcommand invocation.
//!
//! The installed binary is `cargo-cleanme`, invocable directly or as
//! `cargo cleanme`. This integration test proves the documented external form
//! works without mutating user Cargo state: it places the built binary in a
//! temporary directory, prepends that directory to `PATH`, and runs
//! `cargo cleanme --help` through Cargo.

use std::{fs, path::PathBuf, process::Command};

mod common;

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

/// The first `cargo-cleanme[.exe]` on `path_value`, resolved the way Cargo
/// resolves an external subcommand.
///
/// This exists so the external-subcommand cases can assert their own premise.
/// A staged binary that is never the one Cargo picks produces a test that
/// passes or fails for reasons that have nothing to do with the product: the
/// Windows installer case in C012 was green for an entire release cycle that
/// way. `EXE_SUFFIX` is what makes the probe correct on Windows, where an
/// extensionless staged file is invisible to the lookup.
///
/// The path is passed in rather than read from this process's environment on
/// purpose. The child is the thing whose lookup matters, and reading the
/// parent's `PATH` would assert against a different environment than the one
/// Cargo actually searches -- which is how a real installed `cargo-cleanme`
/// can be sitting on `PATH` while the staged fixture is what really runs.
fn resolve_external_subcommand(path_value: &std::ffi::OsStr) -> Option<PathBuf> {
    let file_name = format!("cargo-cleanme{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(path_value)
        .map(|dir| dir.join(&file_name))
        .find(|candidate| candidate.is_file())
}

/// Fail unless the staged binary is provably the one Cargo will execute.
fn assert_staged_is_resolved(path_value: &std::ffi::OsStr, staged: &std::path::Path) {
    let resolved = resolve_external_subcommand(path_value).unwrap_or_else(|| {
        panic!(
            "no cargo-cleanme{} on the staged PATH; the external-subcommand \
             cases would be exercising something else",
            std::env::consts::EXE_SUFFIX
        )
    });
    assert_eq!(
        resolved, staged,
        "Cargo would resolve {resolved:?}, not the staged {staged:?}; \
         the external-subcommand assertions below would prove nothing about \
         this build"
    );
}

#[test]
fn the_staged_binary_is_the_external_subcommand_cargo_will_run() {
    // The premise of the two cases below, asserted as its own case: a
    // successful `cargo cleanme --version` proves the staged bytes ran, and
    // resolving the file name proves they are the ones Cargo selects. Without
    // both, an unrelated `cargo-cleanme` on PATH would satisfy every equality
    // assertion while the product under test was never invoked.
    let (dir, staged) = staged_cargo_cleanme();
    let env_path = path_with(dir.path());
    let search_order = std::env::split_paths(&env_path).collect::<Vec<_>>();
    assert_eq!(
        search_order.first().map(|p| p.as_path()),
        Some(dir.path()),
        "the staged directory must come first on PATH"
    );
    assert_staged_is_resolved(&env_path, &staged);

    let direct = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .arg("--version")
        .output()
        .expect("run direct --version");
    assert!(direct.status.success());
    let external = Command::new("cargo")
        .args(["cleanme", "--version"])
        .env("PATH", &env_path)
        .env("CARGO_HOME", dir.path().join("cargo-home"))
        .output()
        .expect("run cargo cleanme --version");
    assert!(
        external.status.success(),
        "cargo cleanme --version failed: {}",
        String::from_utf8_lossy(&external.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&external.stdout),
        String::from_utf8_lossy(&direct.stdout),
        "cargo ran something other than the staged binary"
    );
}

#[test]
fn cargo_external_subcommand_help_matches_direct_help() {
    let (dir, staged) = staged_cargo_cleanme();
    // Premise first: the staged binary must be the one Cargo resolves, or the
    // stdout comparison below proves nothing about this build.
    let env_path = path_with(dir.path());
    assert_staged_is_resolved(&env_path, &staged);
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
        .env("PATH", &env_path)
        // Isolate Cargo home so no user state is touched; the help path
        // performs no filesystem mutation anyway.
        .env("CARGO_HOME", dir.path().join("cargo-home"))
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
    let (dir, staged) = staged_cargo_cleanme();
    let env_path = path_with(dir.path());
    assert_staged_is_resolved(&env_path, &staged);
    let direct = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args(["config", "edit", "--help"])
        .output()
        .expect("run direct config edit --help");
    assert!(direct.status.success());
    let direct_stdout = String::from_utf8_lossy(&direct.stdout).into_owned();
    assert!(direct_stdout.contains("edit"));

    let external = Command::new("cargo")
        .args(["cleanme", "config", "edit", "--help"])
        .env("PATH", &env_path)
        .env("CARGO_HOME", dir.path().join("cargo-home"))
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

// Platform scope: this case substitutes a POSIX `/bin/sh` Cargo stub, which is
// deliberately Unix-only.
//
// A `#!/bin/sh` file named `cargo` is not a Windows executable: PowerShell and
// `CreateProcess` resolve through PATHEXT and will not run it, so a Windows
// counterpart would have to be a compiled stub rather than the same fixture
// with a different extension. Rather than claim Windows coverage that does not
// exist, the case is scoped to the platform whose executable semantics it
// actually matches. The Unix-only assertions inside it are load-bearing, not
// incidental: the stub is what makes the `clean` invocation observable, and the
// cases assert the stub's own log and argument file.
#[cfg(unix)]
#[test]
fn json_unattended_yes_executes_through_cargo_and_emits_typed_result() {
    use std::{
        os::unix::fs::PermissionsExt,
        time::{Duration, SystemTime},
    };
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    let target = root.join("target");
    let bin = temp.path().join("bin");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(target.join("debug")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        target.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    let artifact = target.join("artifact.bin");
    fs::write(&artifact, vec![7u8; 4096]).unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    for file in [
        root.join("src/main.rs"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        target.join("CACHEDIR.TAG"),
        artifact.clone(),
    ] {
        common::backdate_file(&file, old);
    }
    for directory in [target.clone(), target.join("debug")] {
        common::backdate_file(&directory, old);
    }
    // The workspace directory's own mtime is source activity too, so it has to
    // be backdated for the fixture to look inactive under a real recency window.
    // Directory mtimes need a platform-correct handle on Windows (C009).
    for directory in [root.clone(), root.join("src")] {
        common::backdate_file(&directory, old);
    }
    let log = temp.path().join("cargo.log");
    let args_log = temp.path().join("cargo-args.log");
    let fake = bin.join("cargo");
    fs::write(&fake, r##"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'cargo %s (fixture)\n' "${CARGO_VERSION:-1.99.0}"; exit 0; fi
printf '%s\n' "$1" >> "$CARGO_LOG"
case "$1" in
  locate-project) printf '{"root":"%s/Cargo.toml"}\n' "$FIXTURE_ROOT" ;;
  metadata) printf '{"packages":[{"id":"fixture 0.1.0","name":"fixture","version":"0.1.0","manifest_path":"%s/Cargo.toml"}],"workspace_members":["fixture 0.1.0"],"workspace_root":"%s","target_directory":"%s"}\n' "$FIXTURE_ROOT" "$FIXTURE_ROOT" "$FIXTURE_TARGET" ;;
  clean) printf '%s\n' "$@" >> "$CARGO_ARGS_LOG"; rm -f "$FIXTURE_TARGET/artifact.bin" ;;
  *) exit 2 ;;
esac
"##).unwrap();
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake, permissions).unwrap();
    let config = temp.path().join("config.toml");
    // The fixture is backdated by an hour, so the default recency window keeps
    // it eligible; `recency_seconds = 0` is no longer accepted (it would
    // disable the inactivity guard).
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--yes",
            "--profile",
            "dev",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .env("CARGO_ARGS_LOG", &args_log)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["summary"]["cleaned"], 1, "{json}");
    assert_eq!(json["result"]["units"][0]["reason_code"], "cleaned");
    assert_eq!(json["result"]["selector_kind"], "profile");
    assert_eq!(json["result"]["selector_value"], "dev");
    assert_eq!(
        json["result"]["units"][0]["before_bytes"],
        serde_json::Value::Null
    );
    assert_eq!(
        json["result"]["units"][0]["selector_estimate_bytes"],
        serde_json::Value::Null
    );
    assert!(
        json["result"]["units"][0]["output_union_before_bytes"]
            .as_u64()
            .unwrap()
            >= 4096
    );
    assert!(!artifact.exists());
    let calls = fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().filter(|line| *line == "clean").count(), 1);
    assert!(
        fs::read_to_string(&args_log)
            .unwrap()
            .lines()
            .any(|line| line == "--profile")
    );

    let simulate_output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--profile",
            "dev",
            "--dryrun",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .output()
        .unwrap();
    let simulate_json: serde_json::Value = serde_json::from_slice(&simulate_output.stdout).unwrap();
    assert_eq!(simulate_json["result"]["units"][0]["outcome"], "simulated");
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| *line == "clean")
            .count(),
        1
    );

    let policy_output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--profile",
            "dev",
            "--min-reclaimable-bytes",
            "1",
            "--yes",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .output()
        .unwrap();
    let policy_json: serde_json::Value = serde_json::from_slice(&policy_output.stdout).unwrap();
    assert_eq!(
        policy_json["result"]["units"][0]["reason_code"],
        "selector_unsupported"
    );
    assert_eq!(
        policy_json["result"]["units"][0]["policy_disposition"],
        "selector_estimate_unavailable"
    );
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| *line == "clean")
            .count(),
        1
    );

    let invalid_package = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--package",
            "missing-package",
            "--yes",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .env("CARGO_ARGS_LOG", &args_log)
        .output()
        .unwrap();
    let invalid_json: serde_json::Value = serde_json::from_slice(&invalid_package.stdout).unwrap();
    assert_eq!(
        invalid_json["result"]["units"][0]["reason_code"],
        "selector_invalid"
    );
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| *line == "clean")
            .count(),
        1
    );

    let unsupported_package = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--package",
            "fixture",
            "--yes",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("CARGO_VERSION", "1.97.0")
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .output()
        .unwrap();
    let unsupported_json: serde_json::Value =
        serde_json::from_slice(&unsupported_package.stdout).unwrap();
    assert_eq!(
        unsupported_json["result"]["units"][0]["reason_code"],
        "selector_unsupported"
    );
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| *line == "clean")
            .count(),
        1
    );

    let package_output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "clean",
            root.to_str().unwrap(),
            "--package",
            "fixture@0.1.0",
            "--yes",
            "--format",
            "json",
        ])
        .env("PATH", path_with(&bin))
        .env("FIXTURE_ROOT", &root)
        .env("FIXTURE_TARGET", &target)
        .env("CARGO_LOG", &log)
        .env("CARGO_ARGS_LOG", &args_log)
        .output()
        .unwrap();
    let package_json: serde_json::Value = serde_json::from_slice(&package_output.stdout).unwrap();
    assert_eq!(package_json["result"]["units"][0]["reason_code"], "cleaned");
    assert_eq!(package_json["result"]["selector_kind"], "package");
    assert_eq!(package_json["result"]["selector_value"], "fixture@0.1.0");
    assert_eq!(
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .filter(|line| *line == "clean")
            .count(),
        2
    );
    let cargo_args = fs::read_to_string(args_log).unwrap();
    assert!(cargo_args.lines().any(|line| line == "--package"));
    assert!(cargo_args.lines().any(|line| line == "fixture@0.1.0"));
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
