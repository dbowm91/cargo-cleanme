//! Generate the shell completions and the manpage from the clap command model.
//!
//! Run as `cargo run --quiet --bin generate-docs`. The clap command model in
//! `cargo_cleanme::cli` is the single source of truth; nothing here restates
//! command syntax, so a new flag cannot be documented in one place and missing
//! from the other.
//!
//! `--check` regenerates in memory and compares against the checked-in tree
//! without writing. CI runs that mode, so a command change that was not
//! regenerated fails the build instead of shipping a stale manpage.
//!
//! This binary is excluded from the published package by the `include`
//! allowlist in `Cargo.toml`: it is a maintenance tool, not product surface.

use std::path::Path;

use clap::CommandFactory;

const PRODUCT: &str = "cargo-cleanme";

/// Shells to generate. Elvish is intentionally included: `clap_complete`
/// supports it directly and needs no per-shell maintenance from us.
const SHELLS: &[clap_complete::Shell] = &[
    clap_complete::Shell::Bash,
    clap_complete::Shell::Zsh,
    clap_complete::Shell::Fish,
    clap_complete::Shell::PowerShell,
    clap_complete::Shell::Elvish,
];

fn main() {
    let check_only = std::env::args().any(|arg| arg == "--check");
    if let Err(error) = run(check_only) {
        eprintln!("generate-docs: {error}");
        std::process::exit(1);
    }
}

fn run(check_only: bool) -> Result<(), String> {
    let root = repository_root()?;
    let mut pending: Vec<(String, Vec<u8>)> = Vec::new();

    for shell in SHELLS {
        let mut buffer = Vec::new();
        let mut command = command_model();
        clap_complete::generate(*shell, &mut command, "cargo", &mut buffer);
        pending.push((completion_path(shell), buffer));
    }

    pending.extend(render_manpages()?);

    let mut stale = Vec::new();
    for (relative, bytes) in &pending {
        let path = root.join(relative);
        if check_only {
            let existing = std::fs::read(&path).ok();
            if !existing
                .as_deref()
                .is_some_and(|existing| same_content(existing, bytes))
            {
                stale.push(relative.clone());
            }
        } else {
            write_if_changed(&path, bytes)?;
            println!("wrote {relative} ({} bytes)", bytes.len());
        }
    }

    if !stale.is_empty() {
        return Err(format!(
            "generated artifacts are stale or missing: {}. \
             Run: cargo run --quiet --bin generate-docs",
            stale.join(", ")
        ));
    }
    if check_only {
        println!(
            "generate-docs: {} artifacts match the clap model",
            pending.len()
        );
    }
    Ok(())
}

/// The clap command model, with the external-subcommand wrapper visible.
///
/// Cargo invokes this binary as `cargo cleanme ...`, so the completion and
/// manpage surface is the `cargo cleanme` spelling. Documenting `cargo-cleanme`
/// alone would hand users a command they cannot type after a registry install.
/// The command model the generated artifacts are derived from.
///
/// The model is wrapped so every generated file is reachable as
/// `cargo cleanme ...`: after a registry install the only spelling that works
/// is `cargo cleanme`, and a completion file keyed on `cargo-cleanme` would
/// offer a command the user cannot type. A hidden `cargo-cleanme` subcommand
/// under a `cargo` parent gives the generator a real command tree without
/// changing what the binary accepts.
fn command_model() -> clap::Command {
    let inner = cargo_cleanme::cli::Cli::command()
        .name(PRODUCT)
        // The binary's own help already shows every subcommand; a nested help
        // subcommand here would be noise in the generated manpage.
        .disable_help_subcommand(true);
    clap::Command::new("cargo")
        .bin_name("cargo")
        .subcommand_required(false)
        .subcommand(inner)
}

fn completion_path(shell: &clap_complete::Shell) -> String {
    let name = shell.to_string();
    let file = match shell {
        clap_complete::Shell::Bash => format!("{PRODUCT}.bash"),
        clap_complete::Shell::Zsh => format!("_{PRODUCT}"),
        clap_complete::Shell::Fish => format!("{PRODUCT}.fish"),
        clap_complete::Shell::PowerShell => format!("_{PRODUCT}.ps1"),
        clap_complete::Shell::Elvish => format!("{PRODUCT}.elv"),
        _ => format!("{PRODUCT}.{name}"),
    };
    format!("completions/{file}")
}

/// Render one man page per command node, the conventional multi-page layout.
///
/// `clap_mangen::Man::new` only renders the node it is given, so a single page
/// for the root would silently omit every subcommand. Recursing the same
/// command model keeps `scan`, `clean`, `config`, and `update` documented
/// without restating a single flag.
fn render_manpages() -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut root = command_model();
    let inner = root
        .find_subcommand_mut(PRODUCT)
        .ok_or_else(|| format!("the command model has no {PRODUCT} subcommand"))?;
    *inner = inner.clone().name(PRODUCT).bin_name(PRODUCT);

    let mut pages = Vec::new();
    render_node(inner, &mut pages)?;
    Ok(pages)
}

fn render_node(command: &clap::Command, pages: &mut Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let mut annotated = command.clone();
    if command.get_name() == PRODUCT {
        // The only generator-side prose: how the binary is actually spelled.
        // Flag and subcommand syntax still comes solely from the model.
        //
        // The description itself is the model's own, so the man page and
        // `--help` cannot drift apart. The long form is preferred because it
        // is the one that states what the product does; the short form is the
        // fallback for a model that declares no long description.
        let about = command
            .get_long_about()
            .map(|about| about.to_string())
            .or_else(|| command.get_about().map(|about| about.to_string()))
            .unwrap_or_default();
        annotated = annotated.long_about(format!(
            "{about}\n\nINVOCATION\n             After installation the binary is invoked by Cargo as an external\n             subcommand, so the supported spelling is:\n\n             \tcargo cleanme [SUBCOMMAND] [OPTIONS]\n\n             A directly downloaded binary is invoked as\n\
             \t{PRODUCT} [SUBCOMMAND] [OPTIONS]\n\n             Both spellings accept exactly the same arguments."
        ));
    }

    let man = clap_mangen::Man::new(annotated.clone())
        .title(command.get_name().to_owned())
        .section("1")
        .manual("General Commands Manual")
        .date(env!("CARGO_PKG_VERSION"))
        .source(env!("CARGO_PKG_REPOSITORY"));
    let mut buffer: Vec<u8> = Vec::new();
    man.render(&mut buffer)
        .map_err(|error| format!("could not render the manpage: {error}"))?;

    let mut path = format!("man/{PRODUCT}.1");
    if command.get_name() != PRODUCT {
        path = format!("man/{PRODUCT}-{}.1", command.get_name());
    }
    pages.push((path, buffer));

    for sub in command.get_subcommands() {
        render_node(sub, pages)?;
    }
    Ok(())
}

/// Compare a generated artifact with the file on disk.
///
/// The renderer emits LF, and LF is what git stores. A checkout that converts
/// line endings — `core.autocrlf=true`, which the hosted Windows lane has —
/// puts CRLF in the working tree, and a byte-exact comparison then reports
/// *every* artifact as stale on that lane and nowhere else. The comparison is
/// therefore made over the normalised text: it still fails when the model
/// changed and the artifact was not regenerated, which is what it is for.
///
/// The *write* path deliberately does not normalise: there, LF is what the
/// artifact must end up as, so a CRLF file is rewritten rather than kept.
fn same_content(existing: &[u8], generated: &[u8]) -> bool {
    normalise_line_endings(existing) == normalise_line_endings(generated)
}

/// CRLF and lone CR become LF; every other byte is left alone.
fn normalise_line_endings(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) == Some(&b'\n') {
                out.push(b'\n');
                index += 2;
                continue;
            }
            out.push(b'\n');
            index += 1;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    out
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(existing) = std::fs::read(path)
        && existing == bytes
    {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    }
    std::fs::write(path, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn repository_root() -> Result<std::path::PathBuf, String> {
    // CARGO_MANIFEST_DIR is the repository root for this workspace, so no
    // `git` invocation and no current-directory assumption is needed.
    Ok(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C045 test 9: the generated artifacts are current.
    ///
    /// The man pages and the shell completions are derived from the clap
    /// model, so a change to the model that is not regenerated ships a stale
    /// artifact. The `--check` mode of this binary is the CI gate; this test
    /// is the same check inside the test suite, so a stale artifact fails
    /// `cargo test` and not only the documentation job.
    ///
    /// *Fails if:* the rendered help changed and the committed artifacts were
    /// not regenerated — the input is any edit to the command model.
    #[test]
    fn the_generated_artifacts_match_the_clap_model() {
        let root = repository_root().expect("the repository root");
        let mut pending: Vec<(String, Vec<u8>)> = Vec::new();
        for shell in SHELLS {
            let mut buffer = Vec::new();
            let mut command = command_model();
            clap_complete::generate(*shell, &mut command, "cargo", &mut buffer);
            pending.push((completion_path(shell), buffer));
        }
        pending.extend(render_manpages().expect("the manpages render"));

        let mut stale = Vec::new();
        for (relative, bytes) in &pending {
            let existing = std::fs::read(root.join(relative)).ok();
            if !existing
                .as_deref()
                .is_some_and(|existing| same_content(existing, bytes))
            {
                stale.push(relative.clone());
            }
        }
        assert!(
            stale.is_empty(),
            "the generated artifacts are stale: {}. \
             Run: cargo run --quiet --features dev-tools --bin generate-docs",
            stale.join(", ")
        );
    }

    /// A checkout that converted the line endings is not a stale artifact.
    ///
    /// The hosted Windows lane checks out with `core.autocrlf=true`, so every
    /// artifact in its working tree carries CRLF while the renderer emits LF.
    /// The byte-exact comparison this replaced reported *all thirteen*
    /// artifacts as stale there, and only there — the gate was green on every
    /// other lane and red on the one it was supposed to hold.
    ///
    /// *Fails if:* the comparison stops tolerating a CRLF checkout, or starts
    /// tolerating a *changed* artifact.
    #[test]
    fn a_crlf_checkout_is_not_reported_as_a_stale_artifact() {
        let generated = b".TH CARGO-CLEANME 1\n.SH DESCRIPTION\n";
        let checked_out = b".TH CARGO-CLEANME 1\r\n.SH DESCRIPTION\r\n";
        assert!(
            same_content(checked_out, generated),
            "a CRLF checkout holds the same artifact"
        );
        assert!(
            !same_content(b".TH CARGO-CLEANME 2\r\n.SH DESCRIPTION\r\n", generated),
            "a changed artifact is still stale"
        );
        assert!(
            !same_content(b"", generated),
            "a missing artifact is still stale"
        );
    }
}
