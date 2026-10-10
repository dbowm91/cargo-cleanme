//! Shared test fixture helpers for the cargo-cleanme integration suites.
//!
//! Not a test target: Cargo only compiles top-level `tests/*.rs` files, so this
//! module is included by each suite with `mod common;`.

#![allow(dead_code)]

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Set a path's modification time, for a file *or* a directory.
///
/// `std::fs::File::open` cannot open a directory on Windows, so a plain
/// `set_modified` on a directory is a hard error there. That matters because
/// the scanner's recency verdict includes the workspace root directory itself
/// (`traverse::workspace_member_activity`), so a fixture whose workspace root
/// cannot be backdated does not mean the same thing on every platform.
///
/// Windows needs a handle opened with `FILE_FLAG_BACKUP_SEMANTICS` and
/// `FILE_WRITE_ATTRIBUTES` plus `SetFileTime`. `FILE_WRITE_ATTRIBUTES` is the
/// minimum right that permits setting a timestamp, and the one right a
/// directory handle can be granted.
pub fn set_path_modified(path: &Path, when: SystemTime) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
            FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, SetFileTime,
        };

        // Windows file times count 100-nanosecond intervals since 1601-01-01,
        // which is 11644473600 seconds after the Unix epoch.
        const EPOCH_OFFSET_TICKS: u128 = 11_644_473_600 * 10_000_000;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        let since_epoch = when
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "pre-epoch timestamp"))?;
        let ticks = u64::try_from(since_epoch.as_nanos() / 100 + EPOCH_OFFSET_TICKS)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "timestamp out of range"))?;
        let filetime = FILETIME {
            dwLowDateTime: (ticks & 0xFFFF_FFFF) as u32,
            dwHighDateTime: (ticks >> 32) as u32,
        };

        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        wide.push(0);
        // SAFETY: `wide` is a NUL-terminated UTF-16 buffer that outlives the
        // call, the security-attributes pointer is null as documented, and the
        // returned handle is closed on every path below.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                FILE_WRITE_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `handle` was opened above and is closed exactly once;
        // `filetime` outlives the call.
        let ok = unsafe {
            SetFileTime(
                handle,
                std::ptr::null(),
                std::ptr::null(),
                &filetime as *const FILETIME,
            )
        };
        // SAFETY: `handle` was successfully opened above.
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        if std::fs::symlink_metadata(path)?.is_dir() {
            std::fs::File::open(path)?.set_modified(when)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)?
                .set_modified(when)
        }
    }
}

/// Backdate a fixture file, failing loudly if the timestamp cannot be applied.
pub fn backdate_file(path: &Path, old: SystemTime) {
    if let Err(error) = set_path_modified(path, old) {
        panic!("could not backdate {}: {error}", path.display());
    }
}

/// One inactive, buildable Cargo project under `parent`.
///
/// Backdated everywhere the recency verdict can read, including the workspace
/// root directory itself: `traverse::workspace_member_activity` includes that
/// directory's mtime, so a fixture that backdates only its files looks
/// *active* and would silently prove nothing. Directory timestamps need a
/// platform-correct handle on Windows (C009), which [`backdate_file`] provides.
pub fn inactive_project(parent: &Path, name: &str) -> PathBuf {
    let project = parent.join(name);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("target/debug")).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2021'\n"),
    )
    .unwrap();
    std::fs::write(project.join("Cargo.lock"), "version = 4\n").unwrap();
    std::fs::write(project.join("src/main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        project.join("target/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    let artifact = project.join("target/artifact.bin");
    std::fs::write(&artifact, vec![7u8; 4096]).unwrap();
    std::fs::write(project.join("target/debug/app"), vec![0u8; 8192]).unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    for file in [
        project.join("Cargo.toml"),
        project.join("Cargo.lock"),
        project.join("src/main.rs"),
        project.join("target/CACHEDIR.TAG"),
        artifact.clone(),
        project.join("target/debug/app"),
    ] {
        backdate_file(&file, old);
    }
    for directory in [
        project.clone(),
        project.join("src"),
        project.join("target"),
        project.join("target/debug"),
    ] {
        backdate_file(&directory, old);
    }
    project
}

/// Write a Cargo stand-in into `bin` and return its path.
///
/// Platform scope: this substitutes a POSIX `/bin/sh` script, which is
/// deliberately Unix-only. A `#!/bin/sh` file named `cargo` is not a Windows
/// executable — `CreateProcess` resolves through PATHEXT and will not run it —
/// so a Windows counterpart would need a compiled stub, not this fixture with a
/// different extension. Every caller is `#[cfg(unix)]` with the reason recorded
/// at the call site, so "deliberately scoped" cannot decay into "never
/// executed" (C009/C012).
///
/// The stub logs its first argument to `<temp>/cargo.log` and every argument to
/// `<temp>/cargo-args.log`, answers `--version`, `locate-project`, and
/// `metadata`, and deletes `artifact.bin` only for a `clean` that does not
/// carry `--dry-run`.
///
/// When `CARGO_REAL` names a Cargo binary, an invocation whose first argument is
/// an external subcommand is handed to that Cargo verbatim. This is opt-in
/// rather than automatic because the stub must not silently become a different
/// tool: without `CARGO_REAL` an external-subcommand call still exits 2, exactly
/// as before.
#[cfg(unix)]
pub fn write_fake_cargo(bin: &Path, temp: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(bin).unwrap();
    let fake = bin.join("cargo");
    // Justification for rule 2 of check-fixture-portability.py: a deliberate
    // POSIX executable stub whose callers are all cfg(unix)-gated.
    std::fs::write(&fake, r##"#!/bin/sh
if [ "$1" = "--version" ]; then printf 'cargo %s (fixture)\n' "${CARGO_VERSION:-1.99.0}"; exit 0; fi
if [ -n "$CARGO_REAL" ] && [ "$1" != "--version" ]; then
  case "$1" in
    cleanme) exec "$CARGO_REAL" "$@" ;;
  esac
fi
printf '%s\n' "$1" >> "$CARGO_LOG"
for arg in "$@"; do printf '%s\n' "$arg" >> "$CARGO_ARGS_LOG"; done
dry=no
for arg in "$@"; do [ "$arg" = "--dry-run" ] && dry=yes; done
case "$1" in
  locate-project|metadata)
    if [ "$1" = "locate-project" ] && [ -n "$CARGO_FAIL_LOCATE" ]; then
      printf 'error: failed searching for potential workspace\nCaused by: fixture lookup failure\n' >&2
      exit 101
    fi
    # Real Cargo refuses a manifest it cannot parse. Without this check the
    # stub would answer every query, and a fixture built on an unresolvable
    # manifest would look like a clean one -- the "stub that is not the tool
    # the subject resolves" defect in a new costume.
    case "$(head -c 9 "$FIXTURE_ROOT/Cargo.toml" 2>/dev/null)" in
      '[package]') ;;
      *) printf 'error: failed to parse manifest at `%s/Cargo.toml`\n' "$FIXTURE_ROOT" >&2; exit 101 ;;
    esac
    if [ "$1" = locate-project ]; then
      printf '{"root":"%s/Cargo.toml"}\n' "$FIXTURE_ROOT"
    else
      printf '{"packages":[{"id":"fixture 0.1.0","name":"fixture","version":"0.1.0","manifest_path":"%s/Cargo.toml"}],"workspace_members":["fixture 0.1.0"],"workspace_root":"%s","target_directory":"%s"}\n' "$FIXTURE_ROOT" "$FIXTURE_ROOT" "$FIXTURE_TARGET"
    fi
    ;;
  clean)
    # A knob for the failure path, so a test can produce a real Cargo failure
    # rather than asserting on a failure it constructed by hand.
    if [ -n "$CARGO_FAIL_CLEAN" ]; then
      printf 'error: failed to remove `%s/target`\n' "$FIXTURE_ROOT" >&2
      exit 101
    fi
    if [ "$dry" = yes ]; then
      printf 'Removed 4096 files, done.\n'
    else
      rm -f "$FIXTURE_TARGET/artifact.bin"
      printf 'Removed 1 file, done.\n'
    fi
    ;;
  *) exit 2 ;;
esac
"##)
    .unwrap();
    let mut permissions = std::fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake, permissions).unwrap();
    std::fs::write(temp.join("cargo.log"), "").unwrap();
    std::fs::write(temp.join("cargo-args.log"), "").unwrap();
    fake
}

/// Count how many logged Cargo invocations used the given subcommand.
#[cfg(unix)]
pub fn cargo_calls(log: &Path, subcommand: &str) -> usize {
    std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .filter(|line| *line == subcommand)
        .count()
}
