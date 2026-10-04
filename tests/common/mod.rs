//! Shared test fixture helpers for the cargo-cleanme integration suites.
//!
//! Not a test target: Cargo only compiles top-level `tests/*.rs` files, so this
//! module is included by each suite with `mod common;`.

#![allow(dead_code)]

use std::io;
use std::path::Path;
use std::time::SystemTime;

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
