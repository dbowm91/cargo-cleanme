#!/usr/bin/env python3
"""Static guard against the two fixture-portability defects that are decidable.

C009, C011, and C012 were all the same failure: a fixture that did not run, in a
lane that reported green. Two of those shapes are mechanically decidable from
the source alone, and this check refuses them:

1. **Literal PATH separator assembly in cross-platform fixture code.** `:` is
   the POSIX separator and `;` is the Windows one. Code that runs on both
   platforms must build PATH with `os.pathsep` (Python) or
   `std::env::join_paths` (Rust). A literal separator is wrong on one of them,
   and C012 shipped exactly that. Shell and YAML are excluded on purpose: a
   `shell: bash` step may only run under bash, where `:` is correct.

2. **An unjustified POSIX shebang fixture.** A `#!/bin/sh` file written as a
   stand-in for an executable is not a Windows executable, so a fixture that
   substitutes one cannot run on the Windows lane. That is acceptable *when the
   scope is explicit* — either the enclosing case is platform-gated, or the
   fixture is justified in a comment. This is the source-level evidence the
   corrective plan requires for a deliberately platform-scoped fixture, and it
   is what keeps "scoped" from quietly decaying into "not executed".

What this deliberately does **not** check, because it cannot be decided
statically without false positives:

- whether a substituted tool is actually the one the subject resolves. That
  needs a runtime premise assertion, which is why
  `packaging/tests/test_installers.py --self-test` and the Cargo premise probe in
  `scripts/smoke-release-candidate.py` exist.
- whether a case's pass condition proves the intended branch. Reviewing that is
  the point of the C013 inventory.
- whether a stub is executable on the host. Only running it proves that.

A guard that cannot fail on the real tree is worse than no guard, so both rules
are self-tested against built-in samples in `--self-test` mode: if a rule stops
firing on bad input, the self-test fails.

Usage:
    python3 scripts/check-fixture-portability.py
    python3 scripts/check-fixture-portability.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Fixture surfaces only. Production code and the installers are out of scope:
# a cleanup tool that shells out to cargo is not a fixture, and the corrective
# that opened this audit found no production defect to fix.
SCANNED_GLOBS = (
    "tests/**/*.rs",
    "src/**/*.rs",
    "packaging/tests/**/*.py",
    "scripts/**/*.py",
)

# Rule 1. A `PATH` name given as a string literal, or assigned at the start of a
# line, combined with a literal separator. Requiring the quoted form keeps prose
# and unrelated identifiers out of the match.
PATH_NAME = re.compile(r"""^\s*PATH\s*[=:]|["']PATH["']""")

# Rule 2. A shebang literal that is being *written into a fixture*, as opposed
# to one mentioned in prose. Both halves are required: the shebang, and a write
# call or an explicit bytes/format literal on the same line.
SHEBANG = re.compile(r"#!\s*(?:/usr/bin/env\s+)?(?:/bin/|/usr/bin/)?(?:ba)?sh\b")
FIXTURE_WRITE = re.compile(
    r"""write_text|write_bytes|\bwrite\s*\(|create\s*\(|b["']|rb?["']|format!\s*\("""
)


def quoted_runs(line: str) -> list[str]:
    """The string literals on `line`, paired left to right.

    A single regex cannot do this. `["'][^"']*[:;][^"']*["']` happily pairs a
    *closing* quote with a later *opening* one and reports the code between them
    as one literal, so `{"PATH": str(d / "fallback")}` would look like a literal
    containing a `:` separator, and the real `":"` after it would never be
    examined. Walking the line and pairing each quote with the next identical
    quote is exact for the well-formed source a fixture is written in; an
    escaped quote inside a literal would end a run early, which surfaces as a
    finding to review rather than as a silent miss.
    """
    runs: list[str] = []
    index = 0
    while index < len(line):
        quote = line[index]
        if quote not in "\"'":
            index += 1
            continue
        end = line.find(quote, index + 1)
        if end == -1:
            break
        runs.append(line[index : end + 1])
        index = end + 1
    return runs


def literal_separators(line: str) -> list[str]:
    """Quoted runs in `line` that contain a `:` or `;` separator.

    A `://` run is a URL, not a PATH, so it is not a separator.
    """
    return [
        run
        for run in quoted_runs(line)
        if (";" in run or ":" in run) and "://" not in run
    ]

# Patterns that make a fixture's platform scope explicit. Matched over the
# enclosing function *and* the attribute lines above it, because Rust puts
# `#[cfg(unix)]` above the `fn`, not inside the body. Quote style is not
# significant, so these are patterns rather than substrings.
SCOPE_PATTERNS = tuple(
    re.compile(pattern)
    for pattern in (
        r"cfg!\s*\(\s*(?:not\s*\(\s*)?(?:unix|windows)",
        r"#\s*\[\s*cfg\s*\(\s*(?:not\s*\(\s*)?(?:unix|windows)",
        r"os\.name\s*[!=]=\s*[\"']nt[\"']",
        r"os\.name\s*[!=]=",
        r"platform\.system\s*\(",
        r"sys\.platform",
    )
)

# The explicit justification marker. Prose is not enough; a fixture that needs
# defending has to say so in a form this check can find.
JUSTIFICATION = "fixture-scope:"

# How far above a fixture a justification may sit. Generous enough for a
# paragraph, small enough that it cannot silently cover an unrelated case.
JUSTIFICATION_WINDOW = 40


def scanned_files() -> list[Path]:
    files: list[Path] = []
    for pattern in SCANNED_GLOBS:
        files.extend(sorted(ROOT.glob(pattern)))
    # This checker is excluded from its own scan, and the reason is recorded
    # here rather than hidden: its self-test corpus is deliberately full of
    # violations, so scanning it would always report its own samples.
    return [path for path in files if path.is_file() and path != Path(__file__).resolve()]


def rule_literal_path_separator(path: Path, lines: list[str]) -> list[str]:
    api = "std::env::join_paths" if path.suffix == ".rs" else "os.pathsep"
    problems = []
    for index, line in enumerate(lines, start=1):
        separators = literal_separators(line)
        if PATH_NAME.search(line) and separators:
            problems.append(
                f"line {index}: a PATH value is assembled with a literal separator "
                f"({separators[0]}); use {api}: {line.strip()[:120]}"
            )
    return problems


def scope_is_explicit(lines: list[str], index: int) -> bool:
    """True when the fixture's enclosing case is platform-gated.

    Scans upward from the fixture to the start of the enclosing function, then
    keeps going through the attribute block above it, so a Rust `#[cfg(unix)]`
    written above `fn` still counts.
    """
    start = 0
    for cursor in range(index, -1, -1):
        stripped = lines[cursor].lstrip()
        if re.match(r"(pub(\([^)]*\))?\s+)?(async\s+)?fn\s+\w|def\s+\w", stripped):
            start = cursor
            break
    while start > 0 and re.match(r"\s*(#\[|\[|@)", lines[start - 1]):
        start -= 1
    span = lines[start : index + 1]
    return any(
        pattern.search(line) for line in span for pattern in SCOPE_PATTERNS
    )


def rule_unjustified_shebang_fixture(path: Path, lines: list[str]) -> list[str]:
    problems = []
    for index, line in enumerate(lines, start=1):
        if index == 1:
            # A script's own shebang says how to run the script. It is never a
            # substituted fixture.
            continue
        if not SHEBANG.search(line) or not FIXTURE_WRITE.search(line):
            continue
        window = lines[max(0, index - JUSTIFICATION_WINDOW) : index]
        if JUSTIFICATION in line or any(JUSTIFICATION in above for above in window):
            continue
        if scope_is_explicit(lines, index - 1):
            continue
        problems.append(
            f"line {index}: a POSIX shebang fixture has no explicit platform scope. "
            f'Gate the enclosing case (#[cfg(unix)] / os.name == "nt" / cfg!(windows)) '
            f"or mark it with a `{JUSTIFICATION}` comment saying why it is safe: "
            f"{line.strip()[:120]}"
        )
    return problems


RAW_BYTE_MARKERS = ("from_bytes(", "from_vec(")

# A filesystem write is what makes the premise real. Constructing a raw-byte
# `PathBuf` in memory and handing it to a formatter is portable: the name never
# has to survive a filesystem.
WRITE_MARKERS = (
    "fs::write",
    "std::fs::write",
    "File::create",
    "fs::create_dir",
    "OpenOptions",
    "fs::hard_link",
    "fs::symlink",
)


def function_blocks(lines: list[str]) -> list[tuple[int, int]]:
    """`(attribute_start, body_end)` for every `fn`, by brace matching.

    Rust test bodies nest, so counting braces is the only reliable way to tell
    that a write belongs to the same function as a raw-byte name.
    """
    blocks: list[tuple[int, int]] = []
    for index, line in enumerate(lines):
        match = re.search(r"\bfn\s+[A-Za-z0-9_]+", line)
        if not match or not line.rstrip().endswith("{"):
            continue
        depth = 0
        end = index
        for cursor in range(index, len(lines)):
            depth += lines[cursor].count("{") - lines[cursor].count("}")
            if depth <= 0 and cursor > index:
                end = cursor
                break
            if depth == 0 and cursor == index and "{" in lines[cursor] and "}" in lines[cursor]:
                end = cursor
                break
        else:
            end = len(lines) - 1
        # The attribute block directly above `fn` is the gate.
        start = index
        while start > 0 and lines[start - 1].strip().startswith("#["):
            start -= 1
        blocks.append((start, end))
    return blocks


def rule_raw_byte_name_needs_linux(path: Path, lines: list[str]) -> list[str]:
    """A raw-byte file name written to disk must be gated to Linux, not `unix`.

    APFS and NTFS reject a name that is not valid UTF-8, so a test that writes
    one fails on macOS and Windows *at the write*, before the behaviour under
    test runs. That is the worst shape of lane disagreement: the lane reports a
    product failure for a premise the product never touched.

    This shipped in `update.rs`, where `#[cfg(unix)]` looked correct because the
    fixture is genuinely POSIX-shaped in every other respect. Linux is the
    honest gate: it is the filesystem family that can actually hold the name.

    The rule deliberately requires a **write** in the same function. Building a
    raw-byte `PathBuf` in memory and formatting it is portable, and
    `report.rs::report_escapes_non_utf8_paths` is correctly `#[cfg(unix)]`
    because nothing ever puts that name on disk.
    """
    problems: list[str] = []
    for start, end in function_blocks(lines):
        body = lines[start : end + 1]
        raw = [i for i, line in enumerate(body) if any(m in line for m in RAW_BYTE_MARKERS)]
        if not raw:
            continue
        if not any(any(m in line for m in WRITE_MARKERS) for line in body):
            continue
        # The gate can sit in the attribute block above `fn` or on an inner
        # block, which is how `update.rs` writes it. Both are searched, because
        # a rule that understands only one of them misses the real file — and a
        # rule that misses the real file guards nothing.
        head = " ".join(line for line in lines[start : start + 12] if "cfg(" in line)
        if 'cfg(target_os = "linux")' in head:
            continue
        if not re.search(r"cfg\(unix\)", head):
            continue
        for offset in raw:
            problems.append(
                f"{start + offset + 1}: a raw-byte file name is written to disk here, which "
                'only Linux can hold; gate the test with #[cfg(target_os = "linux")] instead '
                f"of #[cfg(unix)] ({body[offset].strip()[:80]})"
            )
    return problems


def check_lines(path: Path, lines: list[str]) -> list[str]:
    relative = path.relative_to(ROOT)
    problems = []
    for problem in rule_literal_path_separator(path, lines):
        problems.append(f"{relative}:{problem}")
    for problem in rule_unjustified_shebang_fixture(path, lines):
        problems.append(f"{relative}:{problem}")
    for problem in rule_raw_byte_name_needs_linux(path, lines):
        problems.append(f"{relative}:{problem}")
    return problems


def check_file(path: Path) -> list[str]:
    return check_lines(path, path.read_text(encoding="utf-8").splitlines())


def self_test() -> list[str]:
    """Each rule must fire on bad input and stay quiet on good input."""
    failures: list[str] = []

    def case(name: str, relative: str, body: str, expect_flagged: bool) -> None:
        path = ROOT / relative
        if not path.is_relative_to(ROOT):
            failures.append(f"{name}: sample escaped the repository")
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
        try:
            problems = check_file(path)
        finally:
            path.unlink()
        if bool(problems) != expect_flagged:
            failures.append(
                f"{name}: expected {'a finding' if expect_flagged else 'no finding'}, "
                f"got {problems or 'none'}"
            )

    case(
        "a raw-byte name that never reaches the filesystem is accepted",
        "target/fixture-portability-selftest/in_memory.rs",
        "#[cfg(unix)]\n"
        "#[test]\n"
        "fn formats_a_raw_byte_path_without_writing_it() {\n"
        "    use std::os::unix::ffi::OsStringExt;\n"
        "    let p = std::path::PathBuf::from(std::ffi::OsString::from_vec(b\"/ws/bad-\\xff\".to_vec()));\n"
        "    assert!(render(&p).contains(\"bad\"));\n"
        "}\n",
        False,
    )
    case(
        "a raw-byte file name gated to unix instead of Linux is flagged",
        "target/fixture-portability-selftest/bad_unix_gate.rs",
        "#[test]\n"
        "#[cfg(unix)]\n"
        "fn writes_a_raw_byte_name() {\n"
        "    use std::os::unix::ffi::OsStrExt;\n"
        "    let name = std::ffi::OsStr::from_bytes(b\"weird-\\xff\");\n"
        "    std::fs::write(name, b\"\").unwrap();\n"
        "}\n",
        True,
    )
    case(
        "the same fixture gated to Linux is accepted",
        "target/fixture-portability-selftest/good_linux_gate.rs",
        "#[test]\n"
        '#[cfg(target_os = "linux")]\n'
        "fn writes_a_raw_byte_name() {\n"
        "    use std::os::unix::ffi::OsStrExt;\n"
        "    let name = std::ffi::OsStr::from_bytes(b\"weird-\\xff\");\n"
        "    std::fs::write(name, b\"\").unwrap();\n"
        "}\n",
        False,
    )
    case(
        "a raw-byte name with no platform gate at all is accepted (it cannot run anywhere)",
        "target/fixture-portability-selftest/ungated.rs",
        "fn writes_a_raw_byte_name() {\n"
        "    use std::os::unix::ffi::OsStrExt;\n"
        "    std::fs::write(std::ffi::OsStr::from_bytes(b\"x\"), b\"\").unwrap();\n"
        "}\n",
        False,
    )
    case(
        "literal POSIX separator in a Python PATH is flagged",
        "target/fixture-portability-selftest/bad_sep.py",
        'env = {"PATH": str(stub) + ":" + os.environ["PATH"]}\n',
        True,
    )
    case(
        "os.pathsep is accepted",
        "target/fixture-portability-selftest/good_sep.py",
        'env["PATH"] = os.pathsep.join([str(stub), os.environ["PATH"]])\n',
        False,
    )
    case(
        "literal separator in a Rust PATH is flagged",
        "target/fixture-portability-selftest/bad_sep.rs",
        'fn path_with(dir: &Path) -> OsString {\n'
        '    let existing = std::env::var("PATH").unwrap_or_default();\n'
        '    let joined = ("PATH", format!("{}:{}", dir.display(), existing));\n'
        "    joined.1\n"
        "}\n",
        True,
    )
    case(
        "std::env::join_paths is accepted",
        "target/fixture-portability-selftest/good_sep.rs",
        'fn path_with(dir: &Path) -> OsString {\n'
        '    let mut paths = vec![dir.to_path_buf()];\n'
        '    if let Some(existing) = std::env::var_os("PATH") {\n'
        "        paths.extend(std::env::split_paths(&existing));\n"
        "    }\n"
        "    std::env::join_paths(paths).expect(\"join PATH\")\n"
        "}\n",
        False,
    )
    case(
        "an ungated POSIX shebang fixture is flagged",
        "target/fixture-portability-selftest/bad_stub.py",
        "def build(tmp):\n"
        "    stub = tmp / 'cargo'\n"
        "    stub.write_text('#!/bin/sh\\nexit 0\\n')\n",
        True,
    )
    case(
        "a shebang fixture inside an os.name gate is accepted",
        "target/fixture-portability-selftest/gated_stub.py",
        "def build(tmp):\n"
        "    if os.name == 'nt':\n"
        "        return 'cargo.cmd'\n"
        "    stub.write_text('#!/bin/sh\\n')\n",
        False,
    )
    case(
        "a shebang fixture behind a Rust #[cfg(unix)] is accepted",
        "target/fixture-portability-selftest/gated_stub.rs",
        "#[cfg(unix)]\n"
        "#[test]\n"
        "fn commits() {\n"
        "    fs::write(&fake, \"#!/bin/sh\\nexit 0\\n\").unwrap();\n"
        "}\n",
        False,
    )
    case(
        "a shebang fixture with a fixture-scope marker is accepted",
        "target/fixture-portability-selftest/marked_stub.py",
        "# fixture-scope: never executed; only its path is resolved.\n"
        "def build(tmp):\n"
        "    stub.write_text('#!/bin/sh\\n')\n",
        False,
    )
    case(
        "a shebang mentioned in prose is not a fixture",
        "target/fixture-portability-selftest/prose.py",
        "def build(tmp):\n"
        "    # an extensionless `#!/bin/sh` file is invisible to Windows\n"
        "    return tmp\n",
        False,
    )

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove each rule fires on bad input and stays quiet on good input",
    )
    args = parser.parse_args()

    if args.self_test:
        failures = self_test()
        if failures:
            print("check-fixture-portability: self test FAILED", file=sys.stderr)
            for failure in failures:
                print(f"  - {failure}", file=sys.stderr)
            return 1
        print(
            "check-fixture-portability: self test passed; all three rules verified "
            "in both directions"
        )
        return 0

    problems: list[str] = []
    files = scanned_files()
    for path in files:
        problems.extend(check_file(path))

    if problems:
        print("check-fixture-portability: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        f"check-fixture-portability: {len(files)} fixture file(s) clean "
        f"(no literal PATH separator, no unjustified POSIX shebang fixture)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
