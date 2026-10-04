use std::{
    env,
    path::{Path, PathBuf},
};

/// Resolve VISUAL, EDITOR, or the deterministic fallback without invoking a shell.
pub fn resolve_editor() -> Result<(PathBuf, Vec<String>), String> {
    for key in ["VISUAL", "EDITOR"] {
        if let Ok(value) = env::var(key)
            && !value.trim().is_empty()
        {
            let mut words = match split_words(&value) {
                Ok(words) => words,
                Err(_) => continue,
            };
            if words.is_empty() {
                continue;
            }
            // An unusable value in one variable must not hide the next
            // candidate: fall through to EDITOR, then to the fallback list.
            let program = words[0].clone();
            if let Some(resolved) = resolve_program(&program) {
                words.remove(0);
                return Ok((resolved, words));
            }
        }
    }
    for name in ["hx", "vim", "vi", "nano"] {
        if let Some(program) = find_on_path(name) {
            return Ok((program, Vec::new()));
        }
    }
    Err(
        "no editor found; set VISUAL or EDITOR to an executable, or install hx, vim, vi, or nano"
            .into(),
    )
}

fn resolve_program(spec: &str) -> Option<PathBuf> {
    let path = Path::new(spec);
    if path.components().count() > 1 || path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    find_on_path(spec)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    for dir in env::split_paths(&paths) {
        #[cfg(windows)]
        {
            let mut suffixes = vec![String::new()];
            if Path::new(name).extension().is_none() {
                let pathext =
                    env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into());
                suffixes.extend(
                    pathext
                        .to_string_lossy()
                        .split(';')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                );
            }
            for suffix in suffixes {
                let p = dir.join(format!("{name}{suffix}"));
                if runnable_file(&p) {
                    return Some(p);
                }
            }
        }
        #[cfg(not(windows))]
        {
            let p = dir.join(name);
            if runnable_file(&p) {
                return Some(p);
            }
        }
    }
    None
}

fn runnable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn split_words(spec: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = spec.chars().peekable();
    while let Some(ch) = chars.next() {
        // Inside a quoted run only the opening quote closes it, and only a
        // double-quoted run honours backslash escapes.
        if let Some(open) = quote {
            match ch {
                c if c == open => quote = None,
                '\\' if open == '"' && matches!(chars.peek(), Some('"' | '\\')) => {
                    word.push(chars.next().unwrap())
                }
                c => word.push(c),
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
                started = true;
            }
            '\\' => {
                if let Some(next) = chars.next() {
                    if next.is_whitespace() || next == '\\' || next == '\'' || next == '"' {
                        word.push(next)
                    } else {
                        word.push('\\');
                        word.push(next)
                    }
                } else {
                    return Err("VISUAL/EDITOR has an unfinished escape".into());
                }
                started = true;
            }
            c if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("VISUAL/EDITOR has an unfinished quote".into());
    }
    if started {
        words.push(word)
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn parser_preserves_arguments_and_windows_backslashes() {
        assert_eq!(
            split_words("code --wait 'path with spaces'").unwrap(),
            ["code", "--wait", "path with spaces"]
        );
        assert_eq!(
            split_words(r#"editor "C:\Users\Jane\file.toml""#).unwrap(),
            ["editor", r"C:\Users\Jane\file.toml"]
        );
        assert!(split_words("vim 'unfinished").is_err());
    }

    #[test]
    fn unresolvable_visual_falls_through_to_the_next_candidate() {
        // L19: an unusable VISUAL used to hard-fail the whole command, even when
        // EDITOR named a perfectly good program.
        let program = tempfile::tempdir().unwrap();
        let editor = program.path().join("ed");
        // fixture-scope: never executed on any platform, so the `#!` line is
        // not a claim that this file is an executable. `resolve_editor` only
        // *resolves* a program (`resolve_program` accepts any existing file for
        // an explicit path); it never spawns it. The case asserts the
        // fall-through from an unusable VISUAL to a usable EDITOR, and would be
        // identical with an empty file.
        fs::write(&editor, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let dir = program.path();
        // SAFETY: single-threaded test process; restores the previous values.
        let previous_visual = std::env::var_os("VISUAL");
        let previous_editor = std::env::var_os("EDITOR");
        unsafe {
            std::env::set_var("VISUAL", "/nonexistent/editor");
            std::env::set_var("EDITOR", &editor);
            let (resolved, args) = resolve_editor().unwrap();
            assert_eq!(resolved, editor, "must fall through to EDITOR");
            assert!(args.is_empty());
            std::env::remove_var("VISUAL");
            let (resolved, _) = resolve_editor().unwrap();
            assert_eq!(resolved, editor);
        }
        match previous_visual {
            Some(v) => unsafe { std::env::set_var("VISUAL", v) },
            None => unsafe { std::env::remove_var("VISUAL") },
        }
        match previous_editor {
            Some(v) => unsafe { std::env::set_var("EDITOR", v) },
            None => unsafe { std::env::remove_var("EDITOR") },
        }
        let _ = dir;
    }
}
