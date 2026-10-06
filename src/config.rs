use crate::error::AppError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const CONFIG_TEMPLATE: &str = include_str!("../config.toml");
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub cleanup: CleanupConfig,
}
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CleanupConfig {
    #[serde(default)]
    pub allowed_output_roots: Vec<PathBuf>,
    #[serde(default)]
    pub policy: CleanupPolicyConfig,
}
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CleanupPolicyConfig {
    #[serde(default)]
    pub min_reclaimable_bytes: u64,
    pub min_inactive_seconds: Option<u64>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScanConfig {
    #[serde(default = "default_recency")]
    pub recency_seconds: u64,
    pub root: Option<PathBuf>,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub unignore: Vec<PathBuf>,
    #[serde(default = "default_retention")]
    pub learned_root_retention_days: u32,
}
impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            recency_seconds: 300,
            root: None,
            ignore: vec![],
            unignore: vec![],
            learned_root_retention_days: 30,
        }
    }
}
fn default_recency() -> u64 {
    300
}
fn default_retention() -> u32 {
    30
}
#[derive(Clone, Debug)]
pub struct ConfigPathResolver {
    override_path: Option<PathBuf>,
}
impl ConfigPathResolver {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            override_path: path,
        }
    }
    pub fn path(&self) -> Result<PathBuf, AppError> {
        if let Some(p) = &self.override_path {
            return Ok(p.clone());
        }
        let d = ProjectDirs::from("", "", "cargo-cleanme").ok_or_else(|| {
            AppError::Config("platform config directory is unavailable; use --config".into())
        })?;
        Ok(d.config_dir().join("config.toml"))
    }
}
pub fn load(path: &Path) -> Result<Config, AppError> {
    let mut c = if path.exists() {
        let s = fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("cannot read {}: {e}", path.display())))?;
        toml::from_str(&s)
            .map_err(|e| AppError::Config(format!("cannot parse {}: {e}", path.display())))?
    } else {
        Config::default()
    };
    if c.scan.recency_seconds == 0 {
        return Err(AppError::Config(
            "scan.recency_seconds must be greater than 0; 0 would disable the inactivity guard that keeps recently used artifacts out of cleanup".into(),
        ));
    }
    if c.scan.recency_seconds > u64::MAX / 1_000_000_000 {
        return Err(AppError::Config("scan.recency_seconds is too large".into()));
    }
    if c.scan.learned_root_retention_days > 3650 {
        return Err(AppError::Config(
            "scan.learned_root_retention_days must be between 0 and 3650".into(),
        ));
    }
    if let Some(p) = &c.scan.root {
        require_absolute(p, "scan.root")?;
    }
    for p in &c.scan.ignore {
        if !Path::new(p).is_absolute() {
            return Err(AppError::Config(format!(
                "ignore pattern must be absolute: {p}"
            )));
        }
    }
    for p in &c.scan.unignore {
        require_absolute(p, "unignore")?;
    }
    for p in &c.cleanup.allowed_output_roots {
        require_absolute(p, "cleanup.allowed_output_roots")?;
    }
    if let Some(seconds) = c.cleanup.policy.min_inactive_seconds
        && seconds > u64::MAX / 1_000_000_000
    {
        return Err(AppError::Config(
            "cleanup.policy.min_inactive_seconds is too large".into(),
        ));
    }
    // Every pattern the scanner and the cleaner compile is validated here, so
    // `config edit` fails exactly as loudly as the next scan/clean would.
    for pattern in c
        .cleanup
        .policy
        .include
        .iter()
        .chain(&c.cleanup.policy.exclude)
    {
        globset::Glob::new(pattern).map_err(|e| {
            AppError::Config(format!("invalid cleanup policy glob {pattern:?}: {e}"))
        })?;
    }
    for pattern in &c.scan.ignore {
        globset::Glob::new(pattern)
            .map_err(|e| AppError::Config(format!("invalid scan.ignore glob {pattern:?}: {e}")))?;
    }
    // Patterns are matched against canonicalized paths, so a pattern spelled the
    // way the user's own shell shows it (`/tmp/...` where `/tmp` is a symlink)
    // would silently match nothing. Canonicalize each pattern's literal prefix.
    c.scan.ignore = c
        .scan
        .ignore
        .iter()
        .map(|p| canonical_pattern_prefix(p, PatternKind::Glob))
        .collect();
    // `unignore` is a literal relative path, not a glob, so its canonical
    // spelling is spliced unescaped: escaping it would corrupt the path.
    c.scan.unignore = c
        .scan
        .unignore
        .iter()
        .map(|p| {
            PathBuf::from(canonical_pattern_prefix(
                &p.to_string_lossy(),
                PatternKind::Literal,
            ))
        })
        .collect();
    c.cleanup.policy.include = c
        .cleanup
        .policy
        .include
        .iter()
        .map(|p| canonical_pattern_prefix(p, PatternKind::Glob))
        .collect();
    c.cleanup.policy.exclude = c
        .cleanup
        .policy
        .exclude
        .iter()
        .map(|p| canonical_pattern_prefix(p, PatternKind::Glob))
        .collect();
    // The rewrite above manufactures pattern text that no longer exists in the
    // file, so what was validated on the way in is not what is compiled on the
    // way out. Validating the results is what stops a canonical spelling from
    // turning a good pattern into an uncompilable one (or, worse, a different
    // one). No authorization depends on this list: `ignore`/`include`/`exclude`
    // filter discovery and policy, never ownership.
    for pattern in c
        .scan
        .ignore
        .iter()
        .chain(&c.cleanup.policy.include)
        .chain(&c.cleanup.policy.exclude)
    {
        globset::Glob::new(pattern)
            .map_err(|e| AppError::Config(format!("invalid scan.ignore glob {pattern:?}: {e}")))?;
    }
    Ok(c)
}

/// How a pattern-shaped config value is matched, which decides whether its
/// canonical spelling may be bracket-escaped on the way in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    /// Compiled by `globset`. A canonical directory name carrying a glob
    /// character would be read as a pattern, not as itself.
    Glob,
    /// Compared as a literal path (`scan.unignore` is a literal relative path by
    /// design, predating glob support).
    Literal,
}

/// Escape globset metacharacters in a literal path so it splices in as itself.
///
/// Returns `None` for a spelling that cannot be escaped portably, so the caller
/// can leave the user's pattern alone rather than guess. `{` opens an alternation
/// that no bracket form closes, and `\` is the escape character on Unix but a
/// path separator on Windows.
fn escape_glob_literal(path: &str) -> Option<String> {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            // `[]]` and `[[]` are single-character classes holding `]` and `[`;
            // globset's class parser accepts a leading `]` as a literal member.
            '*' | '?' | '[' | ']' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            '{' | '}' | '\\' => return None,
            _ => out.push(c),
        }
    }
    Some(out)
}

/// Rewrite a pattern's glob-free prefix to its canonical spelling.
///
/// Only the leading literal run can be canonicalized, so the suffix is copied
/// verbatim. The pattern is left untouched when the prefix is relative (it could
/// not be resolved against anything meaningful), when it ends in an escape
/// (the split would land inside an escape sequence), when it does not
/// resolve — a pattern naming a path that does not exist still means exactly
/// what it says: it matches nothing — or when the canonical spelling cannot be
/// escaped and therefore cannot be spliced without changing the match.
fn canonical_pattern_prefix(pattern: &str, kind: PatternKind) -> String {
    let cut = pattern.find(['*', '?', '[', '{']).unwrap_or(pattern.len());
    let (prefix, tail) = pattern.split_at(cut);
    if prefix.ends_with('\\') {
        return pattern.to_owned();
    }
    // Trailing separators are structural (`…/x/*` means "children of x"), so
    // they are preserved around the canonicalized directory name.
    let bare = prefix.trim_end_matches(['/', '\\']);
    if bare.is_empty() || !Path::new(bare).is_absolute() {
        return pattern.to_owned();
    }
    let separators = &prefix[bare.len()..];
    match fs::canonicalize(bare) {
        Ok(canonical) => {
            let canonical = canonical.to_string_lossy();
            if canonical == bare {
                pattern.to_owned()
            } else {
                // The canonical spelling is a real directory name, not a
                // pattern. Splicing it raw lets *its* glob characters rewrite
                // what the user's own pattern matches: a directory named
                // `real[abc]` becomes a character class that stops matching the
                // tree the user named and matches two they never wrote down.
                match kind {
                    PatternKind::Glob => match escape_glob_literal(&canonical) {
                        Some(literal) => format!("{literal}{separators}{tail}"),
                        None => pattern.to_owned(),
                    },
                    PatternKind::Literal => format!("{canonical}{separators}{tail}"),
                }
            }
        }
        Err(_) => pattern.to_owned(),
    }
}

pub fn load_or_create(path: &Path) -> Result<Config, AppError> {
    if !path.exists() {
        create_initial(path)?;
    }
    load(path)
}

/// Ensure a config editor target exists without requiring its current contents to parse.
pub fn ensure_exists(path: &Path) -> Result<(), AppError> {
    if !path.exists() {
        create_initial(path)?;
    }
    Ok(())
}
fn require_absolute(p: &Path, field: &str) -> Result<(), AppError> {
    if !p.is_absolute() {
        Err(AppError::Config(format!(
            "{field} must be absolute: {}",
            p.display()
        )))
    } else {
        Ok(())
    }
}
/// Name the step that failed without changing the error's kind.
///
/// Every branch in `create_initial` decides what to do next by inspecting
/// `e.kind()` — `AlreadyExists` means a concurrent writer won and is success,
/// everything else is a failure. Wrapping an error to add context must
/// therefore preserve its kind exactly, or the race handling below silently
/// stops recognising its own benign outcome.
///
/// It is also the only way a diagnostic can say *which* call failed. A bare
/// `AppError::Io` reports the kind and the OS message but not the step, and on
/// Windows two different calls can report the same kind.
/// Monotonic counter making each staging name unique within this process.
static STAGING_NONCE: AtomicU64 = AtomicU64::new(0);

fn step_error(step: &str, e: std::io::Error) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{step} failed: {e}"))
}

fn create_initial(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| step_error("creating the config directory", e))?;
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    if !parent.exists() {
        return Err(AppError::Config(format!(
            "config directory is unavailable: {}",
            parent.display()
        )));
    }
    let file_name = path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_else(|| std::ffi::OsString::from("config.toml"));
    // The staging name must be unique among everything that can be creating a
    // config at the same time, or "lost the race" becomes a routine event
    // dressed up as an error. The pid separates *processes*, so threads within
    // one process were the only remaining collision -- and they collided on
    // every concurrent first use, because they all started at the same
    // `attempt`. A process-wide nonce removes that collision instead of
    // teaching the code to recognise a lost one.
    //
    // The `attempt` suffix stays as a second line for the case the nonce cannot
    // cover: a stale `.tmp.` file left by a crashed process whose pid the OS
    // later reuses. That one really is a name collision, and `AlreadyExists` is
    // the authoritative answer to it.
    for attempt in 0..100 {
        let nonce = STAGING_NONCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = file_name.clone();
        temp_name.push(format!(".tmp.{}.{}.{}", std::process::id(), nonce, attempt));
        let temp = parent.join(temp_name);
        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp);
        let mut f = match created {
            Ok(f) => f,
            // Only a stale name from a previous process can land here now, and
            // `AlreadyExists` is the authoritative answer to that.
            //
            // It is deliberately NOT decided by probing `temp.exists()`. That
            // probe was tried and is wrong on both platforms: the winner of the
            // publication can unlink its staging file before a loser inspects
            // the path, so the name reads free and the real `AlreadyExists`
            // escapes as an error -- observed failing Linux, macOS, and the 1.89
            // lane -- and a Windows `PermissionDenied` leaks through the same
            // gap. A syscall answers whether the name was taken *then*; a probe
            // answers whether it is taken *now*. Where they disagree, the kind.
            //
            // A genuine permissions problem still hard-fails: with the nonce,
            // nothing else is competing for this name, so `PermissionDenied`
            // means exactly what it says.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(AppError::Io(step_error("creating a staging file", e))),
        };
        let write_result: std::io::Result<()> = (|| {
            f.write_all(CONFIG_TEMPLATE.as_bytes())
                .map_err(|e| step_error("writing the staging file", e))?;
            f.sync_all()
                .map_err(|e| step_error("flushing the staging file", e))?;
            drop(f);
            // Creating the final link is atomic and never overwrites a
            // concurrent winner. Both names are siblings on the same volume.
            match fs::hard_link(&temp, path) {
                Ok(()) => {}
                // Some filesystems have no hard links at all: FAT32 and exFAT,
                // several CIFS mounts, and WSL's `/mnt/c` drvfs. There
                // `link(2)` reports "unsupported" *after* the staging file is
                // written, which used to make every single command fail at
                // startup, because `config::load_or_create` is on every path.
                //
                // The fallback keeps the property the design cannot give up —
                // never overwriting a concurrent winner — by creating the
                // destination with `create_new`, which is equally exclusive.
                // What it gives up is crash-atomicity of the *contents*: a
                // crash mid-write leaves a truncated file, which `load` refuses
                // rather than silently reading as defaults. That is the right
                // trade against not running at all.
                Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                    // A successful `create_new` means this process owns the
                    // destination and no racer can have created it, so a later
                    // failure removes its own partial file — otherwise the
                    // lost-race check below would read our truncated write as a
                    // winner's complete config and call it success.
                    let mut published = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)
                        .map_err(|e| step_error("publishing the config", e))?;
                    let written = published
                        .write_all(CONFIG_TEMPLATE.as_bytes())
                        .and_then(|()| published.sync_all());
                    if let Err(e) = written {
                        drop(published);
                        let _ = fs::remove_file(path);
                        return Err(step_error("writing the config", e));
                    }
                }
                Err(e) => return Err(step_error("publishing the config", e)),
            }
            fs::remove_file(&temp).map_err(|e| step_error("removing the staging file", e))?;
            Ok(())
        })();
        if let Err(e) = write_result {
            let _ = fs::remove_file(&temp);
            // "A racer already published a complete config" is success, not
            // failure, and every caller reaching this point wants to carry on
            // with that file.
            //
            // Here the *filesystem* is the discriminator, because unlike the
            // staging name the destination is never unlinked: once published it
            // stays, so `path.is_file()` is a stable answer rather than a race
            // against a winner cleaning up. It is also the only file that can
            // exist at this path, because this is the code that publishes it,
            // and it is only ever published by hard-linking a fully written and
            // synced staging file -- so a regular file here is a complete
            // template, never a partial one. That makes the check independent
            // of which error the platform chose for the lost race: POSIX
            // reports `AlreadyExists`, and Windows can report
            // `PermissionDenied` for the same contention.
            if path.is_file() {
                return Ok(());
            }
            // A destination that exists but is not a regular file is still
            // refused: "exists" must not become "acceptable".
            if path.exists() {
                return Err(AppError::Config(format!(
                    "refusing to replace non-file config {}: {e}",
                    path.display()
                )));
            }
            return Err(AppError::Io(e));
        }
        return Ok(());
    }
    Err(AppError::Config(format!(
        "could not create a temporary config beside {}",
        path.display()
    )))
}
pub fn show(config: &Config) -> Result<String, AppError> {
    toml::to_string_pretty(config).map_err(|e| AppError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn patterns_are_canonicalized_so_exclusions_actually_apply() {
        // H1/M2/L22: patterns are matched against canonicalized paths, so a
        // pattern spelled the way the shell shows the path matched nothing - an
        // exclusion that silently excluded nothing.
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real");
        fs::create_dir_all(real.join("workspace")).unwrap();
        let link_parent = d.path().join("link");
        fs::create_dir_all(&link_parent).unwrap();
        let link = link_parent.join("real");
        symlink(&real, &link).unwrap();
        assert_ne!(
            link.display().to_string(),
            real.display().to_string(),
            "the fixture needs a non-canonical spelling"
        );

        let config = d.path().join("config.toml");
        let shell_spelled = format!("{}/*", link.display());
        fs::write(
            &config,
            format!(
                "[scan]\nignore = [\"{shell_spelled}\"]\n[cleanup.policy]\nexclude = [\"{shell_spelled}\"]\n"
            ),
        )
        .unwrap();
        let loaded = load(&config).unwrap();
        let canonical_prefix = fs::canonicalize(&real).unwrap();
        assert_eq!(
            loaded.scan.ignore,
            vec![format!("{}/*", canonical_prefix.display())]
        );
        assert_eq!(
            loaded.cleanup.policy.exclude,
            vec![format!("{}/*", canonical_prefix.display())]
        );

        // A pattern with no glob metacharacter, and one that does not resolve,
        // keep their spelling (the latter still matches nothing, by definition).
        fs::write(
            &config,
            format!("[cleanup.policy]\nexclude = [\"{}\"]\n", link.display()),
        )
        .unwrap();
        assert_eq!(
            load(&config).unwrap().cleanup.policy.exclude,
            vec![canonical_prefix.display().to_string()]
        );
        fs::write(
            &config,
            format!(
                "[cleanup.policy]\nexclude = [\"{}/does/not/exist/*\"]\n",
                link.display()
            ),
        )
        .unwrap();
        assert_eq!(
            load(&config).unwrap().cleanup.policy.exclude,
            vec![format!("{}/does/not/exist/*", link.display())]
        );
        // A relative pattern is never resolved against the process CWD.
        assert_eq!(
            canonical_pattern_prefix("relative/*", PatternKind::Glob),
            "relative/*",
            "relative patterns keep their spelling"
        );
    }

    #[test]
    fn a_canonical_spelling_cannot_inject_glob_characters() {
        // The rewrite validated the *user's* pattern and then spliced in text
        // that had never been validated. A directory named `real[abc]`, reached
        // through a symlink, turned `link/*` into a character class: the tree
        // the user named stopped matching, and two trees they never named
        // started being pruned.
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("real[abc]");
        fs::create_dir_all(&real).unwrap();
        for sibling in ["reala", "realb"] {
            fs::create_dir_all(d.path().join(sibling)).unwrap();
        }
        let link = d.path().join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        #[cfg(not(unix))]
        let link = {
            fs::create_dir(&link).unwrap();
            real.clone()
        };

        let config = d.path().join("config.toml");
        fs::write(
            &config,
            format!("[scan]\nignore = [\"{}/*\"]\n", link.display()),
        )
        .unwrap();
        let ignore = load(&config).unwrap().scan.ignore;
        assert_eq!(ignore.len(), 1);
        // The compiled pattern must match the tree the user named...
        let matcher = globset::Glob::new(&ignore[0]).unwrap().compile_matcher();
        assert!(
            matcher.is_match(format!("{}/proj", real.display())),
            "the named tree must still match: {}",
            ignore[0]
        );
        // ...and nothing else.
        for sibling in ["reala", "realb"] {
            assert!(
                !matcher.is_match(format!("{}/{}/proj", d.path().display(), sibling)),
                "{} must not be pruned by {}",
                sibling,
                ignore[0]
            );
        }
        // `unignore` is a literal path, so its canonical spelling is spliced
        // unescaped: escaping it would corrupt the path it names.
        let literal =
            canonical_pattern_prefix(&format!("{}/*", link.display()), PatternKind::Literal);
        assert_eq!(literal, format!("{}/*", real.display()));
        // A spelling with no portable escape is left exactly as written.
        let braces = d.path().join("a{b}c");
        fs::create_dir_all(&braces).unwrap();
        let brace_link = d.path().join("bracelink");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&braces, &brace_link).unwrap();
        #[cfg(not(unix))]
        let brace_link = {
            fs::create_dir(&brace_link).unwrap();
            braces.clone()
        };
        let written = format!("{}/*", brace_link.display());
        assert_eq!(
            canonical_pattern_prefix(&written, PatternKind::Glob),
            written,
            "a `{{` in the canonical spelling cannot be escaped, so the pattern is untouched"
        );
    }

    #[test]
    fn recency_seconds_of_zero_is_rejected() {
        // H5: 0 silently disabled the inactivity guard. The default is 300, so an
        // explicit 0 is always a mistake and must be reported as one.
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[scan]\nrecency_seconds = 0\n").unwrap();
        let err = load(&p).unwrap_err().to_string();
        assert!(err.contains("recency_seconds"), "{err}");
        assert!(err.contains("inactivity guard"), "{err}");
        fs::write(&p, "[scan]\nrecency_seconds = 1\n").unwrap();
        assert_eq!(load(&p).unwrap().scan.recency_seconds, 1);
    }

    #[test]
    fn out_of_range_retention_reports_a_range_not_a_parse_error() {
        // L23: a value beyond the field's integer width used to surface as a TOML
        // parse error ("invalid type: integer, expected u16").
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[scan]\nlearned_root_retention_days = 99999\n").unwrap();
        let err = load(&p).unwrap_err().to_string();
        assert!(
            err.contains("learned_root_retention_days must be between 0 and 3650"),
            "{err}"
        );
        fs::write(&p, "[scan]\nlearned_root_retention_days = 7\n").unwrap();
        assert_eq!(load(&p).unwrap().scan.learned_root_retention_days, 7);
    }

    #[test]
    fn invalid_scan_ignore_glob_is_reported_at_load() {
        // M6: an invalid `scan.ignore` glob used to be silently ignored and only
        // fail later (or never).
        //
        // The pattern must be absolute on *this* platform, otherwise the
        // absolute-path check fires first and the test would pass/fail for a
        // reason unrelated to glob compilation. Derive it from the platform
        // temporary directory instead of hardcoding a POSIX literal.
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let absolute_invalid = d.path().join("[unclosed");
        let pattern = absolute_invalid.to_string_lossy().replace('\\', "/");
        fs::write(&p, format!("[scan]\nignore = [\"{pattern}\"]\n")).unwrap();
        let err = load(&p).unwrap_err().to_string();
        assert!(err.contains("invalid scan.ignore glob"), "{err}");
    }

    #[test]
    fn default_recency_and_absent_config() {
        let d = tempfile::tempdir().unwrap();
        let c = load(&d.path().join("missing.toml")).unwrap();
        assert_eq!(c.scan.recency_seconds, 300);
    }
    #[test]
    fn malformed_and_relative_paths_fail() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("bad.toml");
        fs::write(&p, "[scan]\nrecency_seconds = nope\n").unwrap();
        assert!(load(&p).is_err());
        fs::write(&p, "[scan]\nroot = \"relative\"\n").unwrap();
        assert!(load(&p).is_err());
        fs::write(
            &p,
            "[cleanup]\nallowed_output_roots = [\"relative/root\"]\n",
        )
        .unwrap();
        assert!(load(&p).is_err());
    }
    #[test]
    fn checked_in_template_parses_and_matches_defaults() {
        let config: Config =
            toml::from_str(CONFIG_TEMPLATE).expect("checked-in template must parse");
        assert_eq!(config.scan.recency_seconds, 300);
        assert!(config.scan.root.is_none());
        assert!(config.scan.ignore.is_empty());
        assert!(config.scan.unignore.is_empty());
        assert!(config.cleanup.allowed_output_roots.is_empty());
        let defaults = Config::default();
        assert_eq!(config.scan.recency_seconds, defaults.scan.recency_seconds);
        assert_eq!(config.scan.root, defaults.scan.root);
        assert_eq!(config.scan.ignore, defaults.scan.ignore);
        assert_eq!(config.scan.unignore, defaults.scan.unignore);
        assert_eq!(
            config.cleanup.allowed_output_roots,
            defaults.cleanup.allowed_output_roots
        );
    }
    #[test]
    fn config_template_matches_repository_file() {
        let on_disk = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/config.toml"))
            .expect("repository config.toml must exist");
        assert_eq!(
            CONFIG_TEMPLATE, on_disk,
            "bootstrap source must equal the checked-in template"
        );
    }
    #[test]
    fn legacy_cleanup_config_loads_with_neutral_policy() {
        let config: Config = toml::from_str("[cleanup]\nallowed_output_roots = []\n").unwrap();
        assert_eq!(config.cleanup.policy.min_reclaimable_bytes, 0);
        assert_eq!(config.cleanup.policy.min_inactive_seconds, None);
        assert!(config.cleanup.policy.include.is_empty());
        assert!(config.cleanup.policy.exclude.is_empty());
    }
    #[test]
    fn cleanup_policy_globs_and_durations_are_validated() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("config.toml");
        fs::write(&path, "[cleanup.policy]\ninclude = [\"[\"]\n").unwrap();
        assert!(load(&path).is_err());
        fs::write(
            &path,
            "[cleanup.policy]\nmin_inactive_seconds = 18446744073709551615\n",
        )
        .unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn operational_bootstrap_refuses_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        let initial = load_or_create(&p).unwrap();
        assert_eq!(initial.scan.recency_seconds, 300);
        fs::write(&p, "[scan]\nrecency_seconds = 61\n").unwrap();
        assert_eq!(load_or_create(&p).unwrap().scan.recency_seconds, 61);
    }
    #[test]
    fn bootstrap_bytes_equal_checked_in_template() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        load_or_create(&p).unwrap();
        let written = fs::read_to_string(&p).unwrap();
        assert_eq!(written, CONFIG_TEMPLATE);
        // The written file must load and match defaults.
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.scan.recency_seconds, 300);
    }
    #[test]
    fn malformed_config_is_not_replaced_automatically() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("config.toml");
        fs::write(&p, "[scan]\nrecency_seconds = nope\n").unwrap();
        assert!(load(&p).is_err());
        assert!(load_or_create(&p).is_err());
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "[scan]\nrecency_seconds = nope\n"
        );
    }
    #[test]
    fn concurrent_first_use_creates_one_complete_template() {
        // The premise has to be *likely*, not merely possible. A single
        // 8-thread round against one directory hit the race roughly one run in
        // three on Windows, which is not a premise a guard can rest on: the
        // defect it guards is invisible on the runs that pass. Repeated rounds
        // against a fresh nested directory each time give every losing thread
        // many more chances to observe the destination while it is being
        // created, on every platform.
        const WORKERS: usize = 16;
        const ROUNDS: usize = 24;
        let d = tempfile::tempdir().unwrap();
        for round in 0..ROUNDS {
            let p = d.path().join(format!("round{round}/config.toml"));
            let workers: Vec<_> = (0..WORKERS)
                .map(|_| {
                    let path = p.clone();
                    std::thread::spawn(move || load_or_create(&path).unwrap())
                })
                .collect();
            for worker in workers {
                assert_eq!(
                    worker.join().unwrap().scan.recency_seconds,
                    300,
                    "round {round}: every racer must succeed"
                );
            }
            assert_eq!(
                fs::read(&p).unwrap(),
                CONFIG_TEMPLATE.as_bytes(),
                "round {round}: one complete template, never a partial one"
            );
        }
    }
}
