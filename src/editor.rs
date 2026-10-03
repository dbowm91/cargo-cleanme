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
            let mut words = split_words(&value)?;
            if words.is_empty() {
                continue;
            }
            let program = resolve_program(&words.remove(0)).ok_or_else(|| {
                format!(
                    "editor executable was not found or is not a file: {}",
                    value.trim()
                )
            })?;
            return Ok((program, words));
        }
    }
    for name in ["hx", "vim", "vi", "nano"] {
        if let Some(program) = find_on_path(name) {
            return Ok((program, Vec::new()));
        }
    }
    Err("no editor found; set VISUAL or EDITOR, or install hx, vim, vi, or nano".into())
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
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None
                } else {
                    word.push(ch)
                }
            }
            Some('"') => match ch {
                '"' => quote = None,
                '\\' if matches!(chars.peek(), Some('"' | '\\')) => {
                    word.push(chars.next().unwrap())
                }
                _ => word.push(ch),
            },
            None => match ch {
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
            },
            _ => unreachable!(),
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
}
