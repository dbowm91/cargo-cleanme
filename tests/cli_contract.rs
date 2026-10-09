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

    /// Run the same argv through Cargo's external-subcommand entry point.
    ///
    /// The binary under test is staged *inside* `self.bin` — the same directory
    /// that holds the Cargo stub — so `cargo cleanme` resolves it while the
    /// stub still intercepts every Cargo call cargo-cleanme makes in turn.
    /// `assert_staged_is_resolved` below is the premise: without it this case
    /// would report on whichever `cargo-cleanme` happened to be installed, which
    /// is the C012 failure.
    #[cfg(unix)]
    fn run_as_plugin(&self, args: &[&str]) -> std::process::Output {
        let staged = self
            .bin
            .join(format!("cargo-cleanme{}", std::env::consts::EXE_SUFFIX));
        fs::copy(env!("CARGO_BIN_EXE_cargo-cleanme"), &staged).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&staged).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&staged, permissions).unwrap();
        }
        let path = path_with(&self.bin);
        assert_staged_is_resolved(&path, &staged);

        let real_cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut command = Command::new("cargo");
        command.args(["cleanme", "--config"]);
        command.arg(&self.config);
        command.args(["--no-progress", "--format", "json"]);
        command.args(args);
        command
            .env("PATH", &path)
            .env("CARGO_REAL", real_cargo)
            .env("FIXTURE_ROOT", &self.root)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
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

/// C023 §6/§10: **every accepted simulation spelling reaches `Simulate` and
/// spawns zero `cargo clean` processes.**
///
/// The 0.2.0 defect was not "clap rejects the flag" — it accepted
/// `--dry-run clean ROOT`, the spelling Cargo itself passes, and the `Clean` arm
/// read only its own three flags. Dispatch fell through to `Execute`, a real
/// `cargo clean` ran, the run reported success, and it exited 0. The published
/// binary was proved to do exactly that: 65 KB of artifacts destroyed by a
/// command whose name said `--dry-run`.
///
/// So the missing premise in M012A was never "does clap accept this?" — it was
/// "does *every* accepted spelling reach the same execution mode?". Each case
/// below therefore asserts the mode label, the machine summary, **and** the
/// absence of a Cargo clean subprocess, on a fixture that would otherwise be
/// fully eligible to be cleaned. A mode-label assertion alone would pass if
/// dispatch were right and execution were not.
///
/// `--cargo-preview` is included because it is the one accepted combination
/// that means two different things at once. Simulation wins: it is the mode that
/// spawns no Cargo clean at all, and `--cargo-preview` is a request for
/// *Cargo's* dry run, which does spawn Cargo.
#[cfg(unix)]
#[test]
fn every_accepted_simulation_spelling_reaches_simulate_and_spawns_no_cargo_clean() {
    // (label, argv template). `{root}` is the fixture project root.
    let spellings: &[(&str, &[&str])] = &[
        ("bare --dry-run", &["--dry-run"]),
        (
            "root --dry-run before clean",
            &["--dry-run", "clean", "{root}"],
        ),
        (
            "root --dry-run after clean",
            &["clean", "--dry-run", "{root}"],
        ),
        (
            "legacy --dryrun before clean",
            &["--dryrun", "clean", "{root}"],
        ),
        (
            "legacy --dryrun after clean",
            &["clean", "--dryrun", "{root}"],
        ),
        (
            "root --dry-run combined with clean --cargo-preview",
            &["--dry-run", "clean", "--cargo-preview", "{root}"],
        ),
    ];

    for (label, template) in spellings {
        let fixture = MaintenanceFixture::new(true);
        let root = fixture.root.to_str().unwrap().to_string();
        let args: Vec<&str> = template
            .iter()
            .map(|arg| {
                if *arg == "{root}" {
                    root.as_str()
                } else {
                    *arg
                }
            })
            .collect();

        assert!(
            fixture.artifact().exists(),
            "premise for {label}: the artifact exists and the project is otherwise eligible"
        );
        let output = fixture.run(&args);
        assert!(
            output.status.success(),
            "{label} must be accepted: stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|e| panic!("{label} produced no machine envelope: {e}"));

        assert_eq!(json["operation"], "clean", "{label}: {json}");
        assert_eq!(
            json["mode"], "simulate",
            "{label} silently fell through to another execution mode: {json}"
        );
        assert_eq!(
            json["result"]["summary"]["simulated"], 1,
            "{label} simulated nothing: {json}"
        );
        assert_eq!(
            json["result"]["summary"]["cleaned"], 0,
            "{label} reports recovered bytes, which only a real clean produces: {json}"
        );
        // The discriminating assertions. The label can be right while the
        // execution is wrong; these cannot.
        assert_eq!(
            fixture.clean_calls(),
            0,
            "{label} spawned a real cargo clean"
        );
        assert!(
            fixture.artifact().exists(),
            "{label} removed a build artifact"
        );
        // Simulation still resolves: it is not a skip dressed up as one.
        assert!(
            common::cargo_calls(&fixture.log, "metadata") >= 1,
            "{label} never resolved the workspace, so it proved nothing"
        );
    }
}

/// The Cargo plugin entry point normalizes to the same argv, so the same
/// guarantee has to hold there — that is the spelling a user actually types.
///
/// This case is separate because it is the one whose premise can silently rot:
/// it is only meaningful if the staged binary is the external subcommand Cargo
/// resolves, which is asserted before the run rather than assumed.
#[cfg(unix)]
#[test]
fn the_cargo_plugin_simulation_spelling_also_spawns_no_cargo_clean() {
    let fixture = MaintenanceFixture::new(true);
    let root = fixture.root.to_str().unwrap().to_string();
    assert!(fixture.artifact().exists(), "premise: the artifact exists");

    let output = fixture.run_as_plugin(&["--dry-run", "clean", &root]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["mode"], "simulate", "{json}");
    assert_eq!(fixture.clean_calls(), 0, "{json}");
    assert!(
        fixture.artifact().exists(),
        "simulation removed an artifact"
    );
}

/// The one combination that has no coherent public meaning is refused at parse
/// time rather than silently resolved.
///
/// `clean --dry-run --cargo-preview` asks for two mutually exclusive modes from
/// the same subcommand, and clap declares them conflicting. That refusal is the
/// contract: silently picking one would mean a user who asked for both gets
/// whichever the dispatch happened to prefer, with nothing on stdout to say so.
///
/// The asymmetry matters and is not accidental. The *same* two flags spelled
/// across the subcommand boundary (`--dry-run clean --cargo-preview ROOT`) are
/// accepted, because clap cannot make a root flag conflict with a subcommand's;
/// that case is resolved to simulation by `every_accepted_simulation_spelling…`
/// above. This test pins the half that must fail, so making `--dry-run` a
/// `global = true` flag — which would collide with `update --dry-run`'s own
/// argument id — cannot quietly widen what is accepted.
#[cfg(unix)]
#[test]
fn clean_rejects_simulate_and_cargo_preview_together_at_parse_time() {
    let fixture = MaintenanceFixture::new(true);
    let root = fixture.root.to_str().unwrap().to_string();

    let output = fixture.run(&["clean", "--dry-run", "--cargo-preview", &root]);
    assert!(
        !output.status.success(),
        "two exclusive modes must not resolve to a successful run: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        fixture.clean_calls() == 0 && fixture.artifact().exists(),
        "a refused parse must not have spawned anything"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with"),
        "the refusal must name the conflict instead of silently choosing a mode: {stderr}"
    );
}

/// C023 §6: the updater's own dry-run is a *different* flag with a *different*
/// argument id, which is why the root-level `--dry-run` was reconciled in one
/// explicit place rather than made `global = true`. Both spellings must reach
/// the same non-mutating updater path.
///
/// `update` is not asserted against the network here; this asserts the resolved
/// request, which is the part that silently regressed. A live updater rehearsal
/// is `plans/closure/distribution-release-update/m011c-status.md`'s subject.
#[test]
fn update_dry_run_reaches_the_same_non_mutating_path_from_either_spelling() {
    for args in [vec!["--dry-run", "update"], vec!["update", "--dry-run"]] {
        let argv: Vec<std::ffi::OsString> = std::iter::once("cargo-cleanme".to_string())
            .chain(args.iter().map(|a| a.to_string()))
            .map(std::ffi::OsString::from)
            .collect();
        let cli = cargo_cleanme::cli::Cli::try_parse_normalized_from(argv)
            .unwrap_or_else(|e| panic!("{args:?} must be accepted: {e}"));
        match cli.invocation() {
            cargo_cleanme::cli::Invocation::Update { dry_run } => assert!(
                dry_run,
                "{args:?} must resolve to a non-mutating updater request"
            ),
            other => panic!("{args:?} resolved to {other:?}, not an update"),
        }
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

// ---------------------------------------------------------------- update JSON

/// The executable file name Cargo itself would record for this package.
///
/// `.crates.toml` records real executable file names, so the Windows entry
/// carries `.exe` and the POSIX entries do not. Spelling it for the wrong
/// platform does not produce a refusal -- it produces "Cargo does not claim
/// this file", which is the opposite conclusion, so a test that hard-coded one
/// spelling would silently stop testing anything on the other two lanes.
fn cargo_recorded_binary_name() -> &'static str {
    if cfg!(windows) {
        "cargo-cleanme.exe"
    } else {
        "cargo-cleanme"
    }
}

/// Write the built binary to `destination` so that it can be executed
/// immediately, on every host.
///
/// `fs::copy` is deliberately not used. On Linux it goes through
/// `copy_file_range`, and executing the destination straight afterwards can
/// fail with `ETXTBSY` ("Text file busy") because the inode is still open for
/// writing. That is not hypothetical: the `msrv` job of CI run 37408907268 hit
/// it here, on a runner whose `/tmp` is overlayfs, while the same test was green
/// on all three OS lanes and on twelve consecutive local runs. A plain
/// read-then-write has no such window, and `sync_all` makes "these bytes are on
/// disk" an enforced premise rather than an assumption.
///
/// The exec bit is set explicitly instead of inherited, so the staged file is
/// runnable whether or not the source mode survived the copy.
fn stage_runnable_binary(destination: &std::path::Path) {
    let bytes =
        fs::read(env!("CARGO_BIN_EXE_cargo-cleanme")).expect("the built binary is readable");
    fs::write(destination, &bytes).expect("stage the built binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(destination).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(destination, permissions).unwrap();
    }
    // Opened for **writing**, not reading: on Windows `sync_all` is
    // `FlushFileBuffers`, which fails with `PermissionDenied` on a read-only
    // handle. `write(true)` without `truncate(true)` leaves the bytes alone,
    // and the handle is dropped at the end of this statement, so nothing holds
    // the file open by the time the case executes it.
    std::fs::OpenOptions::new()
        .write(true)
        .open(destination)
        .expect("the staged binary reopens for flush")
        .sync_all()
        .expect("the staged binary is flushed before it is executed");
}

/// A staged copy of the built binary inside a directory shaped like
/// `cargo install --root DIR`, with Cargo's own record file beside it.
///
/// The copy matters: provenance is classified from the *running executable's*
/// location, so the refusal can only be produced by a binary that genuinely
/// sits under a Cargo root. The record is the verbatim `v1` schema cargo
/// 1.99.0 writes, and the version is deliberately **older** than the crate
/// version so that the refusal is about ownership and not about being current.
fn cargo_managed_installation(dir: &std::path::Path) -> std::path::PathBuf {
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let staged = bin.join(cargo_recorded_binary_name());
    stage_runnable_binary(&staged);
    fs::write(
        dir.join(".crates.toml"),
        format!(
            "[v1]\n\"cargo-cleanme 0.1.6 \
             (registry+https://github.com/rust-lang/crates.io-index)\" = [\"{}\"]\n",
            cargo_recorded_binary_name()
        ),
    )
    .unwrap();
    staged
}

/// A JSON update failure must not look like a successful update.
///
/// This drives the real binary, not the library seam, and it is deliberately
/// hermetic: a mutating update over forbidden provenance refuses *before* it
/// asks the registry anything (C018's ordering fix), so this exercises the
/// refusal path with no network and no published-release state. That is what
/// makes the "no document" assertion meaningful -- a run that reached the
/// registry could fail for an unrelated reason and prove nothing about the
/// failure shape.
#[test]
fn json_update_refusal_emits_no_document_and_exits_non_zero() {
    let dir = tempfile::tempdir().unwrap();
    let staged = cargo_managed_installation(dir.path());

    let output = Command::new(&staged)
        .args(["--format", "json", "update"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "a forbidden-provenance update must fail; stdout was {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stdout.is_empty(),
        "a failed update printed {} bytes on stdout: {:?}",
        output.stdout.len(),
        String::from_utf8_lossy(&output.stdout)
    );
    // Belt and braces: even if a future change emitted something, it must not
    // be parseable as the success document this plan pins.
    assert!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).is_err(),
        "a failure must never emit a success-shaped JSON document"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cargo install"),
        "the refusal must name the manager command; stderr was {stderr:?}"
    );
}

/// The success stream must reach stdout through the seam the contract tests
/// drive.
///
/// A hermetic *successful* update is not representable: every success path
/// resolves the version authority from crates.io and then replaces bytes, so a
/// test would either depend on published release state or need a
/// production-only network hook. Rather than invent one, this pins the link
/// that is otherwise unproven -- that the JSON branch of `run_update` prints
/// exactly the library stream and nothing else -- which is what turns the
/// library's one-document/one-newline guarantee into a statement about this
/// process's stdout.
#[test]
fn the_json_update_branch_prints_only_the_tested_seam() {
    let raw = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/main.rs"))
        .expect("main.rs is readable from the integration test's working directory");
    // Normalize before slicing. A Windows checkout has CRLF, so a `\n}\n`
    // search finds nothing there and this case fails with "run_update is a
    // top-level function" on a function that is top-level on every platform.
    // That is the C012 shape: the lane that reports the failure is not the
    // lane whose environment the subject was written for. This exact
    // assumption was the first thing the Windows lane caught in C022's own
    // work, one release after it caught the C021 analogue.
    let source = raw.replace("\r\n", "\n");
    let start = source.find("fn run_update(").expect("run_update exists");
    let body = &source[start..];
    let end = body
        .find("\n}\n")
        .expect("run_update is a top-level function");
    let body = &body[..end];

    assert_eq!(
        body.matches("print!(").count(),
        1,
        "the JSON branch must have exactly one unterminated write to stdout; a \
         second one is a second fragment on a stream a JSON consumer parses. \
         (`print!(` cannot match the `println!(` the human branch uses.)"
    );
    assert!(
        body.contains("update_json_stream"),
        "the JSON branch must print output::update_json_stream, the seam the update \
         JSON contract tests exercise; see plans/implementation/\
         distribution-release-update/c022-pre-release-machine-contract-hardening.md §5"
    );
}

// ---------------------------------------------------------------------------
// C028 — stale learned-root availability and cleanup-scope corrective.
//
// Platform scope: every case substitutes the POSIX Cargo stub from
// `common::write_fake_cargo` and overrides `$HOME`/`$XDG_STATE_HOME`, which the
// Windows profile/known-folder APIs ignore; the Windows lane covers the
// equivalent strict-explicit-root path through the existing invalid-root
// cases. Symlink and permission cases are Unix-gated at the call site for the
// same reason `write_fake_cargo` documents.
// ---------------------------------------------------------------------------

/// A Routine-scope fixture with isolated learned state: `$HOME` holds a seed
/// sibling project and `$XDG_STATE_HOME` (Linux) or the macOS application
/// data directory holds a caller-written `discovery-state.json`.
///
/// The state path is computed, not hardcoded, and every case below asserts the
/// omission/blocked evidence that only appears when the binary read that
/// exact file — so a wrong path fails the test instead of passing silently.
#[cfg(unix)]
struct StaleLearnedFixture {
    _temp: tempfile::TempDir,
    home: PathBuf,
    sibling: PathBuf,
    target: PathBuf,
    config: PathBuf,
    log: PathBuf,
    args_log: PathBuf,
    bin: PathBuf,
    state_home: PathBuf,
}

#[cfg(unix)]
impl StaleLearnedFixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        // "Developer" is a seed name with no case-variant among the seed
        // names, so exactly one seed candidate exists even on
        // case-insensitive macOS runners (a "Projects" dir would also match
        // the "projects" seed there and admit a duplicate root).
        let projects = home.join("Developer");
        std::fs::create_dir_all(&projects).unwrap();
        let sibling = common::inactive_project(&projects, "sibling");
        let target = sibling.join("target");
        let bin = temp.path().join("bin");
        common::write_fake_cargo(&bin, temp.path());
        let config = temp.path().join("config.toml");
        fs::write(&config, "[scan]\nrecency_seconds = 300\n").unwrap();
        Self {
            log: temp.path().join("cargo.log"),
            args_log: temp.path().join("cargo-args.log"),
            state_home: temp.path().join("state-home"),
            _temp: temp,
            home,
            sibling,
            target,
            config,
            bin,
        }
    }

    fn state_file(&self) -> PathBuf {
        // Mirrors `discovery_state::state_path` for the overridden
        // environment: XDG state home on Linux, the application data
        // directory under the overridden `$HOME` on macOS.
        #[cfg(target_os = "macos")]
        {
            self.home
                .join("Library/Application Support/cargo-cleanme/discovery-state.json")
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.state_home.join("cargo-cleanme/discovery-state.json")
        }
    }

    fn write_state(&self, learned: &[PathBuf]) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let roots: Vec<serde_json::Value> = learned
            .iter()
            .map(|p| serde_json::json!({"path": p, "last_project_seen_at": now}))
            .collect();
        let state = serde_json::json!({"schema_version": 2, "last_full_at": now, "projects": [], "learned_roots": roots});
        let path = self.state_file();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&state).unwrap()).unwrap();
    }

    /// A learned root that was present when learned and deleted before the
    /// run: the reported Codex-worktree lifecycle in miniature.
    fn write_deleted_learned_root(&self) -> PathBuf {
        let stale = self._temp.path().join("deleted-worktree");
        std::fs::create_dir(&stale).unwrap();
        self.write_state(std::slice::from_ref(&stale));
        std::fs::remove_dir(&stale).unwrap();
        assert!(!stale.exists());
        stale
    }

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
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state_home)
            .env("PATH", path_with(&self.bin))
            .env("FIXTURE_ROOT", &self.sibling)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
    }

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
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state_home)
            .env("PATH", path_with(&self.bin))
            .env("FIXTURE_ROOT", &self.sibling)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
    }

    /// The same argv through Cargo's external-subcommand entry point, with the
    /// premise asserted the C012 way: the staged binary must provably be the
    /// one Cargo resolves.
    fn run_as_plugin(&self, args: &[&str]) -> std::process::Output {
        let staged = self
            .bin
            .join(format!("cargo-cleanme{}", std::env::consts::EXE_SUFFIX));
        fs::copy(env!("CARGO_BIN_EXE_cargo-cleanme"), &staged).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&staged).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&staged, permissions).unwrap();
        }
        let path = path_with(&self.bin);
        assert_staged_is_resolved(&path, &staged);

        let real_cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut command = Command::new("cargo");
        command.args(["cleanme", "--config"]);
        command.arg(&self.config);
        command.args(["--no-progress", "--format", "json"]);
        command.args(args);
        command
            .env("PATH", &path)
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", &self.state_home)
            .env("CARGO_REAL", real_cargo)
            .env("FIXTURE_ROOT", &self.sibling)
            .env("FIXTURE_TARGET", &self.target)
            .env("CARGO_LOG", &self.log)
            .env("CARGO_ARGS_LOG", &self.args_log)
            .output()
            .unwrap()
    }

    fn clean_calls(&self) -> usize {
        common::cargo_calls(&self.log, "clean")
    }

    fn cargo_calls(&self, subcommand: &str) -> usize {
        common::cargo_calls(&self.log, subcommand)
    }

    fn artifact(&self) -> PathBuf {
        self.target.join("artifact.bin")
    }
}

/// C028 premise-negative control and primary regression: a directory that was
/// legitimately learned while present and removed before the next bare
/// Execute/Simulate must not abort the run, and the surviving sibling still
/// receives the same decision simulation and execution agree on.
///
/// Against the unfixed baseline this exact run exits 2 with
/// `invalid scan root <stale>: No such file or directory (os error 2)` and
/// cleans nothing; the assertions below (exit 0, `cleaned == 1`, one Cargo
/// clean, artifact gone) distinguish the absence-path defect from any other
/// error.
#[cfg(unix)]
#[test]
fn stale_learned_root_is_omitted_and_bare_routine_execute_cleans_the_sibling() {
    for via_plugin in [false, true] {
        let fixture = StaleLearnedFixture::new();
        let stale = fixture.write_deleted_learned_root();
        assert!(fixture.artifact().exists(), "premise: artifact exists");

        let output = if via_plugin {
            fixture.run_as_plugin(&[])
        } else {
            fixture.run(&[])
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "via_plugin={via_plugin}: a stale learned root must not abort Routine cleanup:\nstdout: {}\nstderr: {stderr}",
            String::from_utf8_lossy(&output.stdout),
        );
        assert!(
            !stderr.contains("invalid scan root"),
            "via_plugin={via_plugin}: the baseline fatal must be gone: {stderr}"
        );
        assert!(
            stderr.contains("omitting unavailable"),
            "via_plugin={via_plugin}: the omission must be reported truthfully: {stderr}"
        );
        // Exactly one JSON envelope on stdout.
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["operation"], "clean");
        assert_eq!(json["scope"], "routine", "{json}");
        assert_eq!(json["mode"], "execute", "{json}");
        assert_eq!(json["result"]["summary"]["cleaned"], 1, "{json}");
        // The effective universe is the surviving seed tree, not the stale
        // path and not an empty set.
        let selected = json["result"]["selected_roots"].as_array().unwrap();
        assert!(
            !selected.is_empty(),
            "the surviving root must be traversed: {json}"
        );
        assert!(
            !selected
                .iter()
                .any(|r| r.as_str().unwrap() == stale.to_str().unwrap()),
            "the stale root must not be traversed: {json}"
        );
        assert_eq!(fixture.clean_calls(), 1, "via_plugin={via_plugin}: {json}");
        assert!(
            !fixture.artifact().exists(),
            "via_plugin={via_plugin}: execute must clean the sibling"
        );
    }
}

/// The Simulate/Execute parity half: the same stale-root run under `--dry-run`
/// reaches the same universe with zero Cargo clean spawns of any kind.
#[cfg(unix)]
#[test]
fn stale_learned_root_simulate_agrees_with_execute_and_spawns_no_cargo_clean() {
    let fixture = StaleLearnedFixture::new();
    fixture.write_deleted_learned_root();

    let output = fixture.run(&["--dry-run"]);
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "clean");
    assert_eq!(json["scope"], "routine", "{json}");
    assert_eq!(json["mode"], "simulate", "{json}");
    assert_eq!(json["result"]["summary"]["simulated"], 1, "{json}");
    assert_eq!(json["result"]["summary"]["cleaned"], 0, "{json}");
    assert_eq!(
        fixture.clean_calls(),
        0,
        "simulation must invoke no cargo clean of any kind"
    );
    assert!(
        fixture.artifact().exists(),
        "simulation must not remove anything"
    );
    assert!(fixture.cargo_calls("metadata") >= 1, "{json}");
}

/// Explicit roots stay authoritative: a missing configured `scan.root` and a
/// missing explicit `clean ROOT` are fatal usage errors with zero Cargo clean
/// spawns, even alongside available seed/learned paths.
#[cfg(unix)]
#[test]
fn missing_explicit_roots_stay_fatal_with_zero_cargo_clean_spawns() {
    // Configured scan.root, missing.
    let fixture = StaleLearnedFixture::new();
    let missing = fixture._temp.path().join("missing-explicit-root");
    let body = format!(
        "[scan]\nrecency_seconds = 300\nroot = {}\n",
        toml::Value::String(missing.to_str().unwrap().replace('\\', "\\\\"))
    );
    fs::write(&fixture.config, body).unwrap();
    let output = fixture.run(&[]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a missing configured root is a usage error: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.clean_calls(), 0);
    assert!(fixture.artifact().exists());

    // Explicit CLI root, missing.
    let fixture = StaleLearnedFixture::new();
    let missing = fixture._temp.path().join("missing-cli-root");
    let output = fixture.run(&["clean", missing.to_str().unwrap()]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "a missing explicit CLI root is a usage error: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.clean_calls(), 0);
    assert!(fixture.artifact().exists());
}

/// All automatic roots safely missing is a successful, truthfully empty no-op:
/// exit 0 with zero Cargo clean/preview spawns — never a fatal error and
/// never a claim that zero roots were traversed silently.
#[cfg(unix)]
#[test]
fn all_automatic_roots_missing_is_a_successful_empty_no_op() {
    let fixture = StaleLearnedFixture::new();
    // Remove the seed tree so nothing automatic remains but the stale root.
    std::fs::remove_dir_all(fixture.home.join("Developer")).unwrap();
    fixture.write_deleted_learned_root();

    let output = fixture.run(&["--dry-run"]);
    assert!(
        output.status.success(),
        "an empty automatic scope is not an error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["operation"], "clean", "{json}");
    assert_eq!(json["scope"], "routine", "{json}");
    assert_eq!(json["result"]["units"], serde_json::json!([]), "{json}");
    assert_eq!(fixture.clean_calls(), 0);
    assert_eq!(fixture.cargo_calls("metadata"), 0);
}

/// A regular file where an automatic root was is positive non-directory
/// evidence: omit it like absence. A symlink (even to a directory) is never
/// followed and never treated as positive absence: block with exit 1 and zero
/// spawns of any kind.
#[cfg(unix)]
#[test]
fn automatic_file_root_is_omitted_and_symlink_root_blocks_without_traversal() {
    // Regular file: omitted, sibling still cleaned.
    let fixture = StaleLearnedFixture::new();
    let file = fixture._temp.path().join("file-instead-of-dir");
    fs::write(&file, b"not a directory").unwrap();
    fixture.write_state(&[file]);
    let output = fixture.run(&[]);
    assert!(
        output.status.success(),
        "a file-typed automatic root must be omitted:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["summary"]["cleaned"], 1, "{json}");
    assert_eq!(fixture.clean_calls(), 1);

    // Symlink to a directory: blocked, nothing traversed, nothing spawned —
    // not even workspace resolution. This is the `is_dir`-follows-symlinks
    // trap: `Path::is_dir` says true, the walker would never descend.
    let fixture = StaleLearnedFixture::new();
    let target = fixture._temp.path().join("link-target");
    std::fs::create_dir(&target).unwrap();
    let link = fixture._temp.path().join("symlinked-root");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(link.is_dir(), "premise: is_dir follows the symlink");
    fixture.write_state(&[link]);
    let output = fixture.run(&["--dry-run"]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a symlinked automatic root must block, not omit:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["scope_blocked"], true, "{json}");
    assert_eq!(fixture.clean_calls(), 0);
    assert_eq!(
        fixture.cargo_calls("metadata"),
        0,
        "a blocked scope resolves nothing"
    );

    // Broken symlink: the same block, never positive absence.
    let fixture = StaleLearnedFixture::new();
    let broken = fixture._temp.path().join("broken-root");
    std::os::unix::fs::symlink(fixture._temp.path().join("nowhere"), &broken).unwrap();
    fixture.write_state(&[broken]);
    let output = fixture.run(&["--dry-run"]);
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["scope_blocked"], true, "{json}");
    assert_eq!(fixture.clean_calls(), 0);
}

/// An unreadable automatic root is indeterminate coverage, not absence: block
/// with exit 1 and zero spawns. Skipped with the premise stated when the
/// process can still read through the mode bits (e.g. running as root), where
/// the fixture could not produce the indeterminate state it claims.
#[cfg(unix)]
#[test]
fn unreadable_automatic_root_blocks_instead_of_omitting() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = StaleLearnedFixture::new();
    let locked = fixture._temp.path().join("locked-root");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert!(
        locked.read_dir().is_err(),
        "premise: effective test identity must not read mode-000 root"
    );
    fixture.write_state(&[locked]);
    let output = fixture.run(&["--dry-run"]);
    std::fs::set_permissions(
        fixture._temp.path().join("locked-root"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "an unreadable automatic root must block:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["scope_blocked"], true, "{json}");
    assert_eq!(fixture.clean_calls(), 0);
}

/// A traversal diagnostic still blocks cleanup, but read-only Cargo metadata
/// resolution must be reflected in the report rather than represented as zero.
#[cfg(unix)]
#[test]
fn incomplete_discovery_reports_actual_resolution_counts_and_spawns_no_clean() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = StaleLearnedFixture::new();
    let inaccessible = fixture.home.join("Developer/inaccessible");
    fs::create_dir(&inaccessible).unwrap();
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o000)).unwrap();
    assert!(
        fs::read_dir(&inaccessible).is_err(),
        "premise: child is unreadable"
    );

    let output = fixture.run(&["--dry-run"]);
    fs::set_permissions(&inaccessible, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["scope_blocked"], true, "{json}");
    assert_eq!(json["result"]["discovered_manifests"], 1, "{json}");
    assert_eq!(json["result"]["resolved_workspaces"], 1, "{json}");
    assert_eq!(json["result"]["units_considered"], 1, "{json}");
    assert!(
        fixture.cargo_calls("metadata") > 0,
        "Cargo metadata was attempted"
    );
    assert_eq!(
        fixture.clean_calls(),
        0,
        "incomplete discovery blocks every clean"
    );
}

/// Log mode tells the truth without leaking paths: the stale-root omission
/// stays one bounded ASCII line with exit 0, and the symlink block reports
/// `status=blocked` with exit 1.
#[cfg(unix)]
#[test]
fn stale_and_blocked_automatic_roots_keep_the_bounded_log_contract() {
    let fixture = StaleLearnedFixture::new();
    let stale = fixture.write_deleted_learned_root();
    let output = fixture.run_log(&["--dry-run"]);
    assert!(output.status.success());
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" op=clean "), "{line}");
    assert!(line.contains(" scope=routine "), "{line}");
    assert!(
        !line.contains(stale.to_str().unwrap()),
        "no paths in log: {line}"
    );

    let fixture = StaleLearnedFixture::new();
    let target = fixture._temp.path().join("log-link-target");
    std::fs::create_dir(&target).unwrap();
    let link = fixture._temp.path().join("log-link-root");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    fixture.write_state(std::slice::from_ref(&link));
    let output = fixture.run_log(&["--dry-run"]);
    assert_eq!(output.status.code(), Some(1));
    let line = assert_bounded_log_line(&output.stdout);
    assert!(line.contains(" status=blocked "), "{line}");
    assert!(
        !line.contains(link.to_str().unwrap()),
        "no paths in log: {line}"
    );
}
