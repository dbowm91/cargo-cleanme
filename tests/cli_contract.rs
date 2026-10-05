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

/// The JSON `scope` label must describe the scope that was resolved, not the
/// flags that were passed.
///
/// A configured `scan.root` with no CLI root resolves to `ScanScope::Explicit`,
/// so the label has to be derived from the resolved policy. Deriving it from
/// `root.is_some()` reported `"routine"` for a scan that was in fact pinned to
/// one explicit root — the label a consumer uses to tell what was scanned.
///
/// M012A: rootless `scan` is now Full, so the configured-root case is reached
/// through `scan --known`, which is the operation that still consults the
/// maintenance scope resolver.
#[test]
fn json_scope_label_follows_the_resolved_scope_not_the_cli_flags() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    // `root` pins the scan to this directory with no CLI argument, which is the
    // case that previously mislabelled itself as a Routine scan.
    fs::write(
        &config,
        format!(
            "[scan]\nrecency_seconds = 300\nroot = {}\n",
            toml::Value::String(dir.path().to_str().unwrap().replace('\\', "\\\\"))
        ),
    )
    .unwrap();

    let run = |args: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args(["--config", config.to_str().unwrap(), "--no-progress"]);
        command.args(["--format", "json", "scan"]);
        command.args(args);
        command.output().unwrap()
    };

    // Configured root, no CLI root: resolved Explicit, so reported "explicit".
    let from_config = run(&["--known"]);
    assert!(
        from_config.status.success(),
        "{}",
        String::from_utf8_lossy(&from_config.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&from_config.stdout).unwrap();
    assert_eq!(
        json["scope"], "explicit",
        "a configured scan.root resolves to an explicit scope: {json}"
    );

    // The CLI root still reports "explicit" — the existing contract, unchanged.
    let from_cli = run(&[dir.path().to_str().unwrap()]);
    assert!(from_cli.status.success());
    let json: serde_json::Value = serde_json::from_slice(&from_cli.stdout).unwrap();
    assert_eq!(json["scope"], "explicit");
}

/// M012A §7: incompatible scan scope selectors are rejected by the parser, so
/// they cannot reach traversal at all.
///
/// The pass condition is the specific failure — clap's conflict error on
/// stderr with nothing on stdout — rather than "the command failed", which a
/// Full scan or a config error could also produce.
#[test]
fn scan_scope_conflicts_are_rejected_before_any_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    for args in [
        vec!["scan", dir.path().to_str().unwrap(), "--known"],
        vec!["scan", dir.path().to_str().unwrap(), "--full"],
        vec!["scan", "--known", "--full"],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args(["--config", config.to_str().unwrap(), "--no-progress"]);
        command.args(&args);
        let output = command.output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?} must be a usage error, not a run: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stdout.is_empty(),
            "{args:?} wrote a report before being rejected"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
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
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
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

// ---------------------------------------------------------------------------
// M012A — canonical maintenance invocation and cleanup mode semantics.
//
// Platform scope for every `#[cfg(unix)]` case below: these substitute a POSIX
// `/bin/sh` Cargo stub (see `common::write_fake_cargo`), which is deliberately
// Unix-only. The load-bearing assertions are the stub's own call log and the
// filesystem, so a lane that could not run the stub proves nothing here rather
// than reporting green.
// ---------------------------------------------------------------------------

/// One inactive Cargo project, a config pinning the maintenance scope to it, and
/// a Cargo stub whose call log proves what was (and was not) spawned.
#[cfg(unix)]
struct MaintenanceFixture {
    temp: tempfile::TempDir,
    root: PathBuf,
    target: PathBuf,
    config: PathBuf,
    log: PathBuf,
    args_log: PathBuf,
    bin: PathBuf,
}

#[cfg(unix)]
impl MaintenanceFixture {
    fn new(with_configured_root: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = common::inactive_project(temp.path(), "workspace");
        let target = root.join("target");
        let bin = temp.path().join("bin");
        common::write_fake_cargo(&bin, temp.path());
        let config = temp.path().join("config.toml");
        let body = if with_configured_root {
            format!(
                "[scan]\nrecency_seconds = 300\nroot = {}\n",
                toml::Value::String(root.to_str().unwrap().replace('\\', "\\\\"))
            )
        } else {
            "[scan]\nrecency_seconds = 300\n".to_string()
        };
        fs::write(&config, body).unwrap();
        Self {
            log: temp.path().join("cargo.log"),
            args_log: temp.path().join("cargo-args.log"),
            temp,
            root,
            target,
            config,
            bin,
        }
    }

    /// Run cargo-cleanme with the stub on PATH and return its output.
    fn run(&self, args: &[&str]) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args([
            "--config",
            self.config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "json",
        ]);
        command.args(args);
        command
            .env("PATH", path_with(&self.bin))
            .env("FIXTURE_ROOT", &self.root)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
    }

    fn clean_calls(&self) -> usize {
        common::cargo_calls(&self.log, "clean")
    }

    fn artifact(&self) -> PathBuf {
        self.target.join("artifact.bin")
    }

    /// Recreate the deleted artifact and put its timestamps back where the
    /// recency guard can see an inactive project again. A real Execute removed
    /// it, which also refreshed the enclosing directory's mtime.
    fn restore_artifact(&self) {
        let artifact = self.artifact();
        fs::write(&artifact, vec![7u8; 4096]).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        common::backdate_file(&artifact, old);
        common::backdate_file(&self.target, old);
    }
}

/// ADR 003 §1: bare `cargo cleanme` is Routine Execute through the ordinary
/// combined-root safety engine, not a read-only scan.
///
/// This is the premise-negative regression for the milestone. Under the
/// pre-M012A dispatch, `Cli::command` was `None` and `main` fell through to
/// `run_scan(None, false, ...)`, so this run produced `operation = "scan"`,
/// never invoked Cargo clean, and left the artifact in place.
#[cfg(unix)]
#[test]
fn bare_invocation_executes_routine_cleanup_and_is_not_a_scan() {
    let fixture = MaintenanceFixture::new(true);
    assert!(fixture.artifact().exists(), "premise: the artifact exists");

    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(
        json["operation"], "clean",
        "bare invocation must reach cleanup, not scan: {json}"
    );
    assert!(
        json["result"]["units"].is_array(),
        "a cleanup envelope has units; a scan envelope has groups: {json}"
    );
    assert_eq!(json["mode"], "execute");
    // A configured legacy `scan.root` is the exclusive Explicit override, so
    // the resolved scope — not the argv spelling — is what the label names.
    assert_eq!(json["scope"], "explicit", "{json}");
    assert_eq!(json["result"]["summary"]["cleaned"], 1, "{json}");

    // The behaviour, not just the label: Cargo was invoked and the artifact is
    // gone. A scan would have left both untouched.
    assert_eq!(fixture.clean_calls(), 1, "{json}");
    assert!(!fixture.artifact().exists(), "execute must clean");
}

/// ADR 003 §2: bare `--dry-run` is application simulation and spawns no
/// `cargo clean` process at all — not even Cargo's own dry run.
#[cfg(unix)]
#[test]
fn bare_dry_run_simulates_with_zero_cargo_clean_processes() {
    let fixture = MaintenanceFixture::new(true);
    let output = fixture.run(&["--dry-run"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "clean");
    assert_eq!(json["mode"], "simulate", "{json}");
    assert_eq!(json["scope"], "explicit", "{json}");
    assert_eq!(json["result"]["summary"]["simulated"], 1, "{json}");
    assert_eq!(json["result"]["summary"]["cleaned"], 0, "{json}");

    // The discriminating assertion: simulation resolved and proved everything,
    // and still spawned nothing.
    assert_eq!(
        fixture.clean_calls(),
        0,
        "--dry-run must invoke no cargo clean of any kind"
    );
    assert!(
        fixture.artifact().exists(),
        "simulation must not remove anything"
    );
    // Cargo *was* used for workspace resolution, which is what makes this a
    // simulation rather than a skip.
    assert!(common::cargo_calls(&fixture.log, "metadata") >= 1);
}

/// A bare maintenance run with no known roots is a successful no-op report,
/// not an error: exit 0 with a complete envelope stating the resolved scope.
///
/// Platform scope: `directories` resolves `$HOME` on Unix, so overriding it
/// gives a home with none of the Routine seed directories and no learned state.
/// Windows resolves the profile through a known-folder API that ignores the
/// environment, so this fixture cannot bound the scope there; the Windows lane
/// covers the equivalent zero-result path through `json_scan_within_an_empty_root`.
#[cfg(unix)]
#[test]
fn bare_cleanup_with_no_known_roots_is_a_successful_no_op() {
    let fixture = MaintenanceFixture::new(false);
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
    command.args([
        "--config",
        fixture.config.to_str().unwrap(),
        "--no-progress",
        "--format",
        "json",
    ]);
    let output = command
        .env("HOME", home.path())
        .env("PATH", path_with(&fixture.bin))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "an empty maintenance scope is not an error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "clean", "{json}");
    assert_eq!(json["scope"], "routine", "{json}");
    assert_eq!(json["mode"], "execute", "{json}");
    assert_eq!(json["result"]["summary"]["cleaned"], 0, "{json}");
    assert_eq!(json["result"]["units"], serde_json::json!([]), "{json}");
}

/// Unresolved ownership blocks the whole selected scope in every mode, and the
/// block is visible through the bare front door.
#[cfg(unix)]
#[test]
fn bare_cleanup_with_unresolved_ownership_blocks_and_runs_no_cargo_clean() {
    let fixture = MaintenanceFixture::new(true);
    // A manifest that cannot resolve is the canonical incomplete-coverage
    // shape: one unresolvable participant blocks the complete scope.
    fs::write(
        fixture.root.join("Cargo.toml"),
        "this is not a valid cargo manifest [\n",
    )
    .unwrap();

    let output = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "clean", "{json}");
    assert_eq!(json["result"]["scope_blocked"], true, "{json}");
    assert!(
        json["result"]["scope_reason"]
            .as_str()
            .unwrap()
            .contains("ownership could not be proven"),
        "{json}"
    );
    assert_eq!(
        fixture.clean_calls(),
        0,
        "a blocked scope must spawn no Cargo clean"
    );
}

/// Advanced cleanup defaults to Execute, and Cargo's own preview is reachable
/// only under its explicit name.
#[cfg(unix)]
#[test]
fn advanced_cleanup_defaults_to_execute_and_cargo_preview_is_explicit() {
    let fixture = MaintenanceFixture::new(true);
    let root = fixture.root.to_str().unwrap().to_string();

    let execute = fixture.run(&["clean", &root]);
    assert!(
        execute.status.success(),
        "{}",
        String::from_utf8_lossy(&execute.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&execute.stdout).unwrap();
    assert_eq!(json["operation"], "clean");
    assert_eq!(json["scope"], "explicit", "{json}");
    assert_eq!(
        json["mode"], "execute",
        "absent a mode flag, clean executes: {json}"
    );
    assert_eq!(json["result"]["summary"]["cleaned"], 1, "{json}");
    assert_eq!(fixture.clean_calls(), 1);
    assert!(!fixture.artifact().exists());

    // Recreate the artifact so the preview has something to prove it did not do.
    fixture.restore_artifact();
    let preview = fixture.run(&["clean", &root, "--cargo-preview"]);
    assert!(preview.status.success());
    let json: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(json["mode"], "preview", "{json}");
    assert_eq!(json["result"]["summary"]["previewed"], 1, "{json}");
    assert_eq!(
        fixture.clean_calls(),
        2,
        "Cargo preview does spawn Cargo, with --dry-run"
    );
    assert!(
        fixture.artifact().exists(),
        "Cargo preview must never remove anything"
    );
    assert!(
        fs::read_to_string(&fixture.args_log)
            .unwrap()
            .lines()
            .any(|line| line == "--dry-run"),
        "Cargo preview is Cargo's own --dry-run, distinguishable from simulation"
    );

    // Simulation adds nothing: still two Cargo clean invocations in total.
    let simulate = fixture.run(&["clean", &root, "--dry-run"]);
    assert!(simulate.status.success());
    let json: serde_json::Value = serde_json::from_slice(&simulate.stdout).unwrap();
    assert_eq!(json["mode"], "simulate", "{json}");
    assert_eq!(fixture.clean_calls(), 2);
    assert!(fixture.artifact().exists());
}

/// The retained compatibility aliases select exactly the canonical modes and
/// nothing else. They print nothing, so an unattended run stays quiet.
#[cfg(unix)]
#[test]
fn hidden_compatibility_aliases_map_exactly_to_the_canonical_modes() {
    let fixture = MaintenanceFixture::new(true);
    let root = fixture.root.to_str().unwrap().to_string();

    let legacy_yes = fixture.run(&["clean", &root, "--yes"]);
    let legacy_dryrun = fixture.run(&["clean", &root, "--dryrun"]);
    let canonical_dry_run = fixture.run(&["clean", &root, "--dry-run"]);
    for output in [&legacy_yes, &legacy_dryrun, &canonical_dry_run] {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let yes: serde_json::Value = serde_json::from_slice(&legacy_yes.stdout).unwrap();
    let legacy: serde_json::Value = serde_json::from_slice(&legacy_dryrun.stdout).unwrap();
    let canonical: serde_json::Value = serde_json::from_slice(&canonical_dry_run.stdout).unwrap();

    assert_eq!(yes["mode"], "execute", "{yes}");
    assert_eq!(legacy["mode"], canonical["mode"], "{legacy} vs {canonical}");
    assert_eq!(legacy["mode"], "simulate", "{legacy}");
    for output in [&legacy_yes, &legacy_dryrun, &canonical_dry_run] {
        assert!(
            !String::from_utf8_lossy(&output.stderr).contains("deprecat"),
            "a hidden alias must not add chatter to an unattended run: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// `scan --known` resolves the Routine scope when no `scan.root` is configured,
/// and the Explicit scope when one is.
///
/// Platform scope: `directories` resolves `$HOME` on Unix, which is what bounds
/// the Routine seed search to this fixture. Windows resolves the profile
/// through a known-folder API that ignores the environment.
#[cfg(unix)]
#[test]
fn known_scan_resolves_routine_without_a_configured_root_and_explicit_with_one() {
    let fixture = MaintenanceFixture::new(false);
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join("projects")).unwrap();
    let seeded = common::inactive_project(&home.path().join("projects"), "routine-project");
    let pinned_config = fixture.temp.path().join("configured.toml");
    fs::write(
        &pinned_config,
        format!(
            "[scan]\nrecency_seconds = 300\nroot = {}\n",
            toml::Value::String(seeded.to_str().unwrap().replace('\\', "\\\\"))
        ),
    )
    .unwrap();

    let run = |config: &std::path::Path, root: &std::path::Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "json",
            "scan",
            "--known",
        ]);
        let output = command
            .env("HOME", home.path())
            .env("PATH", path_with(&fixture.bin))
            .env("FIXTURE_ROOT", root)
            .env("FIXTURE_TARGET", root.join("target"))
            .env("CARGO_LOG", &fixture.log)
            .env("CARGO_ARGS_LOG", &fixture.args_log)
            .output()
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let routine = run(&fixture.config, &seeded);
    assert_eq!(routine["operation"], "scan", "{routine}");
    assert_eq!(
        routine["scope"], "routine",
        "no configured root means the Routine scope: {routine}"
    );
    assert_eq!(
        routine["result"]["discovered_manifests"], 1,
        "the seeded project must actually be discovered, or the label proves nothing: {routine}"
    );

    let explicit = run(&pinned_config, &seeded);
    assert_eq!(explicit["operation"], "scan", "{explicit}");
    assert_eq!(explicit["scope"], "explicit", "{explicit}");
}

/// A zero-result scan inside a bounded root: the platform-neutral counterpart
/// of `bare_cleanup_with_no_known_roots_is_a_successful_no_op`.
#[test]
fn json_scan_within_an_empty_root_is_a_successful_zero_result_report() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "json",
            "scan",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "scan");
    assert_eq!(json["scope"], "explicit");
    assert_eq!(json["result"]["groups"], serde_json::json!([]));
    assert_eq!(json["result"]["summary"]["group_count"], 0);
}

// ---------------------------------------------------------------------------
// M012B — bounded unattended log output.
//
// Platform scope: these substitute the POSIX Cargo stub from
// `common::write_fake_cargo`, for the same reason as the cases above.
// ---------------------------------------------------------------------------

/// Run the fixture with `--format log` instead of `json`.
#[cfg(unix)]
impl MaintenanceFixture {
    fn run_log(&self, args: &[&str]) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args([
            "--config",
            self.config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "log",
        ]);
        command.args(args);
        command
            .env("PATH", path_with(&self.bin))
            .env("FIXTURE_ROOT", &self.root)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
    }
}

/// Every rule the log contract states, asserted once so each case below only
/// has to state what is specific to it.
#[cfg(unix)]
fn assert_bounded_log_line(stdout: &[u8]) -> String {
    let text = String::from_utf8(stdout.to_vec()).expect("log output is ASCII");
    assert!(text.is_ascii(), "log output is ASCII by contract: {text:?}");
    assert!(
        text.len() <= cargo_cleanme::output::log::MAX_BYTES,
        "{} bytes exceeds the bound: {text:?}",
        text.len()
    );
    assert_eq!(
        text.matches('\n').count(),
        1,
        "exactly one terminating newline: {text:?}"
    );
    assert!(text.ends_with('\n'), "{text:?}");
    assert!(
        text.starts_with("cargo-cleanme op="),
        "the prefix identifies the tool: {text:?}"
    );
    assert!(
        !text.contains('\r') && !text.contains('\u{1b}'),
        "no progress or terminal control sequences: {text:?}"
    );
    for token in text.trim_end().split(' ') {
        assert!(
            !token.contains('"') && !token.contains('\t'),
            "no value needs quoting: {token:?}"
        );
        if let Some((key, _)) = token.split_once('=') {
            assert!(
                !key.is_empty() && key.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "keys are lowercase ascii identifiers: {token:?}"
            );
        }
    }
    text
}

/// M012B §4: exactly one bounded ASCII line on stdout, and nothing on stderr.
#[cfg(unix)]
#[test]
fn log_mode_routine_execute_is_one_bounded_line_and_a_silent_stderr() {
    let fixture = MaintenanceFixture::new(true);
    let output = fixture.run_log(&[]);
    assert!(
        output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" op=clean "), "{line}");
    assert!(line.contains(" status=ok "), "{line}");
    assert!(line.contains(" scope=explicit "), "{line}");
    assert!(line.contains(" mode=execute "), "{line}");
    assert!(line.contains(" cleaned=1 "), "{line}");
    assert!(
        line.contains(" reclaimed_bytes="),
        "an executed cleanup measured a decrease: {line}"
    );
    assert_eq!(
        fixture.clean_calls(),
        1,
        "log mode executes; the line is not a dry run in disguise"
    );
    assert!(
        output.stderr.is_empty(),
        "a successful run writes nothing to stderr in log mode: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// M012B §5: Simulate reports its mode and spawns no Cargo clean. Cargo preview
/// reports `preview` and claims no reclaimed bytes.
#[cfg(unix)]
#[test]
fn log_mode_distinguishes_simulate_from_cargo_preview() {
    let fixture = MaintenanceFixture::new(true);
    let simulate = fixture.run_log(&["--dry-run"]);
    assert!(simulate.status.success());
    let line = assert_bounded_log_line(&simulate.stdout);
    assert!(line.contains(" mode=simulate "), "{line}");
    assert!(
        !line.contains("reclaimed_bytes"),
        "a simulation recovers nothing and must not print a figure for it: {line}"
    );
    assert_eq!(fixture.clean_calls(), 0, "simulation spawns no Cargo clean");
    assert!(
        simulate.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&simulate.stderr)
    );

    let preview = fixture.run_log(&["clean", fixture.root.to_str().unwrap(), "--cargo-preview"]);
    assert!(preview.status.success());
    let line = assert_bounded_log_line(&preview.stdout);
    assert!(line.contains(" mode=preview "), "{line}");
    assert!(line.contains(" scope=explicit "), "{line}");
    assert!(
        !line.contains("reclaimed_bytes"),
        "Cargo preview removes nothing and must not print a figure for it: {line}"
    );
    assert_eq!(fixture.clean_calls(), 1, "Cargo preview does spawn Cargo");
    assert!(fixture.artifact().exists(), "and removes nothing");
}

/// M012B §5: a scope block is one line on stdout, a typed reason, exit 1.
#[cfg(unix)]
#[test]
fn log_mode_scope_block_is_one_line_with_a_typed_reason_and_exit_one() {
    let fixture = MaintenanceFixture::new(true);
    fs::write(fixture.root.join("Cargo.toml"), "not a manifest [\n").unwrap();
    let output = fixture.run_log(&[]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" status=blocked "), "{line}");
    assert!(line.contains(" reason=ownership_unproven "), "{line}");
    assert!(
        !line.contains("ownership could not be"),
        "prose must never reach a retained history line: {line}"
    );
    assert_eq!(fixture.clean_calls(), 0);
    assert!(
        output.stderr.is_empty(),
        "a blocked report is still a report, so stdout carries it and stderr stays quiet: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// M012B §5: one or more failed units are `status=failed` with exit 1.
#[cfg(unix)]
#[test]
fn log_mode_failed_cleanup_is_one_line_with_status_failed_and_exit_one() {
    let fixture = MaintenanceFixture::new(true);
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
    command.args([
        "--config",
        fixture.config.to_str().unwrap(),
        "--no-progress",
        "--format",
        "log",
    ]);
    let output = command
        .env("PATH", path_with(&fixture.bin))
        .env("FIXTURE_ROOT", &fixture.root)
        .env("FIXTURE_TARGET", &fixture.target)
        .env("CARGO_LOG", &fixture.log)
        .env("CARGO_ARGS_LOG", &fixture.args_log)
        .env("CARGO_FAIL_CLEAN", "1")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" status=failed "), "{line}");
    assert!(line.contains(" failed=1 "), "{line}");
    // Cargo's own stderr is captured, not forwarded: an unattended tail must not
    // inherit whatever a Cargo release decides to print.
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("failed to remove"),
        "Cargo stderr escaped onto stdout: {line}"
    );
}

/// M012B §5: a successful zero-result maintenance run is still `status=ok`.
#[cfg(unix)]
#[test]
fn log_mode_zero_result_maintenance_is_status_ok() {
    let fixture = MaintenanceFixture::new(false);
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
    command.args([
        "--config",
        fixture.config.to_str().unwrap(),
        "--no-progress",
        "--format",
        "log",
    ]);
    let output = command
        .env("HOME", home.path())
        .env("PATH", path_with(&fixture.bin))
        .output()
        .unwrap();
    assert!(output.status.success());
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" status=ok "), "{line}");
    assert!(line.contains(" scope=routine "), "{line}");
    assert!(line.contains(" cleaned=0 "), "{line}");
}

/// M012B §4/§5: scan summaries carry the resolved scope.
#[cfg(unix)]
#[test]
fn log_mode_scan_reports_the_resolved_scope() {
    let fixture = MaintenanceFixture::new(false);
    let home = tempfile::tempdir().unwrap();
    let seeded = common::inactive_project(&home.path().join("projects"), "seeded");
    let run = |config: &std::path::Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"));
        command.args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "log",
            "scan",
            "--known",
        ]);
        let output = command
            .env("HOME", home.path())
            .env("PATH", path_with(&fixture.bin))
            .env("FIXTURE_ROOT", &seeded)
            .env("FIXTURE_TARGET", seeded.join("target"))
            .env("CARGO_LOG", &fixture.log)
            .env("CARGO_ARGS_LOG", &fixture.args_log)
            .output()
            .unwrap();
        assert_bounded_log_line(&output.stdout)
    };
    let routine = run(&fixture.config);
    assert!(routine.contains(" op=scan "), "{routine}");
    assert!(routine.contains(" status=ok "), "{routine}");
    assert!(routine.contains(" scope=routine "), "{routine}");
    assert!(routine.contains(" manifests=1 "), "{routine}");
    assert!(
        routine.contains(" groups=1 "),
        "the seeded project must actually be found, or the line proves nothing: {routine}"
    );

    let pinned = fixture.temp.path().join("configured.toml");
    fs::write(
        &pinned,
        format!(
            "[scan]\nrecency_seconds = 300\nroot = {}\n",
            toml::Value::String(seeded.to_str().unwrap().replace('\\', "\\\\"))
        ),
    )
    .unwrap();
    let explicit = run(&pinned);
    assert!(explicit.contains(" scope=explicit "), "{explicit}");

    let cli_root = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            fixture.config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "log",
            "scan",
            seeded.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let explicit = assert_bounded_log_line(&cli_root.stdout);
    assert!(explicit.contains(" scope=explicit "), "{explicit}");
}

/// M012B §6: `--stats` stays an explicit stderr override and does not touch the
/// one stdout line.
#[cfg(unix)]
#[test]
fn log_mode_stats_is_opt_in_stderr_and_leaves_the_single_line_alone() {
    let fixture = MaintenanceFixture::new(true);
    // Run --stats first, then put the artifact back, so the two runs see the
    // same project and their lines are comparable. Comparing a pre-clean run
    // against a post-clean one would prove nothing about --stats.
    let stats = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            fixture.config.to_str().unwrap(),
            "--no-progress",
            "--stats",
            "--format",
            "log",
        ])
        .env("PATH", path_with(&fixture.bin))
        .env("FIXTURE_ROOT", &fixture.root)
        .env("FIXTURE_TARGET", &fixture.target)
        .env("CARGO_LOG", &fixture.log)
        .env("CARGO_ARGS_LOG", &fixture.args_log)
        .output()
        .unwrap();
    let stats_line = assert_bounded_log_line(&stats.stdout);
    fixture.restore_artifact();
    let plain = fixture.run_log(&[]);
    let line = assert_bounded_log_line(&plain.stdout);
    assert_eq!(
        stats_line.trim_end(),
        line.trim_end(),
        "the one stdout line is byte-identical with and without --stats"
    );
    assert!(
        String::from_utf8_lossy(&stats.stderr).contains("cleanup stats:"),
        "--stats remains an opt-in stderr override: {}",
        String::from_utf8_lossy(&stats.stderr)
    );
    assert!(
        plain.stderr.is_empty(),
        "and without it, stderr stays empty: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
}

/// M012B §6: normal diagnostic fan-out is suppressed in log mode; the line's
/// `diagnostics=` count carries it instead.
///
/// The case is discriminating in two directions: the scan really does produce
/// diagnostics (the human path narrates them on stderr), and log mode still
/// writes nothing there.
#[cfg(unix)]
#[test]
fn log_mode_suppresses_normal_diagnostic_fan_out() {
    let fixture = MaintenanceFixture::new(true);
    // A manifest the stub refuses to parse becomes a scan diagnostic. Scans are
    // deliberately partial-result tolerant, so this is a diagnostic and not a
    // blocked scope — which is exactly the state the human path narrates.
    fs::write(fixture.root.join("Cargo.toml"), "not a manifest [\n").unwrap();

    let human = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            fixture.config.to_str().unwrap(),
            "--no-progress",
            "scan",
            fixture.root.to_str().unwrap(),
        ])
        .env("PATH", path_with(&fixture.bin))
        .env("FIXTURE_ROOT", &fixture.root)
        .env("FIXTURE_TARGET", &fixture.target)
        .env("CARGO_LOG", &fixture.log)
        .env("CARGO_ARGS_LOG", &fixture.args_log)
        .output()
        .unwrap();
    assert!(
        human.status.success(),
        "a scan with an unresolvable manifest is a partial result, not a failure: {}",
        String::from_utf8_lossy(&human.stderr)
    );
    let human_stderr = String::from_utf8_lossy(&human.stderr);
    assert!(
        human_stderr.contains("filesystem diagnostics"),
        "premise: the human path does fan diagnostics out to stderr, got {human_stderr:?}"
    );

    let log = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            fixture.config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "log",
            "scan",
            fixture.root.to_str().unwrap(),
        ])
        .env("PATH", path_with(&fixture.bin))
        .env("FIXTURE_ROOT", &fixture.root)
        .env("FIXTURE_TARGET", &fixture.target)
        .env("CARGO_LOG", &fixture.log)
        .env("CARGO_ARGS_LOG", &fixture.args_log)
        .output()
        .unwrap();
    assert!(log.status.success());
    let line = assert_bounded_log_line(&log.stdout);
    assert!(
        line.trim_end()
            .split(' ')
            .any(|token| token == "diagnostics=1"),
        "the count replaces the fan-out: {line}"
    );
    assert!(
        log.stderr.is_empty(),
        "log mode must not fan diagnostics out to stderr: {}",
        String::from_utf8_lossy(&log.stderr)
    );
}

/// M012B §6: a pre-report fatal error is exactly one bounded stderr line and no
/// stdout at all.
#[cfg(unix)]
#[test]
fn log_mode_fatal_error_is_one_bounded_stderr_line_and_no_stdout() {
    let fixture = MaintenanceFixture::new(true);
    let bad = fixture.temp.path().join("broken.toml");
    fs::write(&bad, "this is = not [ valid toml\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            bad.to_str().unwrap(),
            "--no-progress",
            "--format",
            "log",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "no report exists, so no summary line: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        text.trim_end().lines().count(),
        1,
        "exactly one stderr line: {text:?}"
    );
    assert!(
        text.len() <= cargo_cleanme::output::log::MAX_BYTES,
        "{text:?}"
    );
    assert!(
        text.contains("op=clean") && text.contains("status=error") && text.contains("reason="),
        "{text:?}"
    );
    assert!(
        !text.contains("toml") && !text.contains(bad.to_str().unwrap()),
        "the bounded line carries a typed code, not the config path or its parse error: {text:?}"
    );
}

/// M012B §8: JSON and human are unchanged by the existence of log mode.
#[cfg(unix)]
#[test]
fn json_and_human_output_are_unchanged_by_log_mode() {
    let json_fixture = MaintenanceFixture::new(true);
    let json = json_fixture.run(&[]);
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(parsed["operation"], "clean");
    assert_eq!(parsed["mode"], "execute");
    assert_eq!(parsed["scope"], "explicit");
    assert!(parsed["result"]["units"].is_array());

    let log = json_fixture.run_log(&[]);
    let line = assert_bounded_log_line(&log.stdout);
    // Same operation, scope, and mode from the same run — the two renderers
    // project one report, they do not each decide what happened.
    assert!(line.contains(" op=clean "), "{line}");
    assert!(line.contains(" scope=explicit "), "{line}");
    assert!(line.contains(" mode=execute "), "{line}");

    let human = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            json_fixture.config.to_str().unwrap(),
            "--no-progress",
        ])
        .env("PATH", path_with(&json_fixture.bin))
        .env("FIXTURE_ROOT", &json_fixture.root)
        .env("FIXTURE_TARGET", &json_fixture.target)
        .env("CARGO_LOG", &json_fixture.log)
        .env("CARGO_ARGS_LOG", &json_fixture.args_log)
        .output()
        .unwrap();
    assert!(
        human.status.success(),
        "{}",
        String::from_utf8_lossy(&human.stderr)
    );
    let text = String::from_utf8_lossy(&human.stdout);
    assert!(
        text.contains("combined roots ") && text.contains("cleanup:"),
        "human output keeps its own shape: {text:?}"
    );
}

/// M012B §4: an invalid format value is still a usage error, so log is not a
/// silently-accepted typo.
#[test]
fn log_is_an_explicit_value_and_invalid_formats_still_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "jsonl",
            "scan",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

/// The one-document-per-invocation invariant, which `clean --full` can break.
///
/// This regression was introduced by M012A, which deleted the scan report's
/// `emit_output` guard and therefore let the internal Full reconciliation write
/// its report ahead of the cleanup report — two JSON documents on one stream.
/// It was found by reading `architecture/13-orchestration.md`, not by a test,
/// because no case ran `clean --full`.
///
/// The test below cannot run `clean --full` either: that is a full filesystem
/// walk, and this suite keeps every case bounded. What it *does* prove is the
/// property that made the bug possible — that a scan's report is emitted exactly
/// once — and it asserts the guard exists in the source, which is the only
/// mechanism available from outside the process. That is weaker than an
/// end-to-end case and is recorded as such.
#[test]
fn scan_emits_exactly_one_json_document() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args([
            "--config",
            config.to_str().unwrap(),
            "--no-progress",
            "--format",
            "json",
            "scan",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        output.stdout.iter().filter(|b| **b == b'\n').count(),
        1,
        "one newline-terminated document"
    );
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .expect("stdout parses as exactly one JSON document, not a concatenation");

    // The guard that keeps `clean --full` from emitting two documents in a row.
    // If this fails, a case can run `clean --full` cheaply again.
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("main.rs is readable from the integration test's working directory");
    assert!(
        source.contains("enum EmitReport"),
        "the internal-scan report guard was removed; see architecture/13-orchestration.md §4"
    );
}
