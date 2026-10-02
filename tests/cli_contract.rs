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
fn cargo_external_config_init_help_matches_direct() {
    let (_dir_guard, _staged) = staged_cargo_cleanme();
    let direct = Command::new(env!("CARGO_BIN_EXE_cargo-cleanme"))
        .args(["config", "init", "--help"])
        .output()
        .expect("run direct config init --help");
    assert!(direct.status.success());
    let direct_stdout = String::from_utf8_lossy(&direct.stdout).into_owned();
    assert!(direct_stdout.contains("--force"));

    let external = Command::new("cargo")
        .args(["cleanme", "config", "init", "--help"])
        .env("PATH", path_with(_dir_guard.path()))
        .env("CARGO_HOME", _dir_guard.path().join("cargo-home"))
        .output()
        .expect("run cargo cleanme config init --help");
    assert!(external.status.success());
    assert_eq!(direct_stdout, String::from_utf8_lossy(&external.stdout));
}
