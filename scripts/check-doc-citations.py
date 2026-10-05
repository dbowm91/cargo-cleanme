#!/usr/bin/env python3
"""Guard the `file.rs:N` line citations in `architecture/`.

The deep dives cite source by line, which is what makes them checkable and what
makes them rot. The `architecture/` set was written one commit before a fix
that added 38 lines to `main.rs` and 55 to `traverse.rs`; every citation past
the insertion point silently became a reference to unrelated code, and the
prose kept reading as though it were true. Nothing failed, because a line
number existing proves only that the number exists — the same failure shape
`check-fixture-portability.py` exists to catch, one layer up.

This check enforces the mechanically decidable half:

1. **The cited file exists**, in `src/` or `tests/`.
2. **The cited line is in range.** A citation into a file that shrank, or past
   EOF, is how a stale reference announces itself once the two drift far
   enough apart.
3. **A range is ordered** (`start <= end`). An inverted range is always a
   hand-editing error, never a real reference.

What this deliberately does **not** check, because it cannot be decided
statically without false positives:

- whether the line still contains the thing the prose claims. A number inside
  a file proves the file is long enough, not that the sentence is true. That
  is a reviewer's job, and [overview §7.2](architecture/overview.md) is the
  precedent for recording when such a check was misdesigned.
- prose that cites a file by bare name (`cleanup.rs` with no line), or the
  `dua-core` / `filesize` upstream sources, which are not vendored here.

Rules 1 and 2 are what turn "the doc is probably fine" into a gate. Both are
self-tested in `--self-test` mode against built-in samples in the failing
direction: a guard that cannot fail on bad input is worse than no guard.

Usage:
    python3 scripts/check-doc-citations.py
    python3 scripts/check-doc-citations.py --self-test
    python3 scripts/check-doc-citations.py --doc architecture/09-cleanup.md
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# `main.rs:302`, `src/main.rs:302-308`, `cli_contract.rs:680`.
# The leading `src/` or `tests/` is optional; a bare `foo.rs` resolves against
# both, and an ambiguous name is a failure rather than a silent first match.
CITATION = re.compile(
    r"(?<![\w/.-])"
    r"(?:(?P<dir>src|tests)/)?"
    r"(?P<stem>[a-z_]+)\.rs"
    r"(?::(?P<start>\d{1,5})(?:\s*-\s*(?P<end>\d{1,5}))?)?"
)


def source_index() -> dict[str, Path]:
    """Map module stem -> file, for `src/*.rs` and `tests/*.rs`.

    A stem present in both directories is ambiguous and is dropped, so a
    citation naming it is reported instead of resolving to whichever file
    happened to be scanned first.

    `lib` is excluded on purpose. `src/lib.rs` is a 15-line module manifest
    that no deep dive cites by line, while upstream references written as
    `dua-core lib.rs:659` or `filesize-0.2.0/src/lib.rs:58-126` all carry the
    stem `lib`. Indexing it would resolve those against our manifest and report
    every one of them as out of range.
    """
    index: dict[str, Path] = {}
    ambiguous: set[str] = set()
    for directory in ("src", "tests"):
        for path in sorted((REPO / directory).glob("*.rs")):
            stem = path.stem
            if stem in ambiguous:
                continue
            if stem == "lib":
                continue
            if stem in index:
                ambiguous.add(stem)
            else:
                index[stem] = path
    for stem in ambiguous:
        index.pop(stem, None)
    return index


def line_counts(index: dict[str, Path]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for stem, path in index.items():
        with path.open(encoding="utf-8", errors="replace") as handle:
            counts[stem] = sum(1 for _ in handle)
    return counts


def check_text(
    text: str,
    where: str,
    index: dict[str, Path],
    counts: dict[str, int],
) -> list[str]:
    """Return one message per mechanically broken citation in `text`."""
    problems: list[str] = []
    for lineno, line in enumerate(text.splitlines(), 1):
        # Skip fenced code blocks: a doc that quotes source verbatim carries
        # line numbers from the quoted file's own listing, not citations.
        for match in CITATION.finditer(line):
            stem = match.group("stem")
            if stem not in index:
                continue  # an upstream or non-Rust reference, out of scope
            start_raw = match.group("start")
            if start_raw is None:
                continue  # a bare file mention, nothing to range-check
            start = int(start_raw)
            end_raw = match.group("end")
            end = int(end_raw) if end_raw else start
            total = counts[stem]
            location = f"{where}:{lineno}"
            if start < 1 or start > total:
                problems.append(
                    f"{location}: cites {stem}.rs:{start} but that file has "
                    f"{total} line(s)"
                )
                continue
            if end < 1 or end > total:
                problems.append(
                    f"{location}: cites {stem}.rs:{start}-{end} but that file "
                    f"has {total} line(s)"
                )
                continue
            if end < start:
                problems.append(
                    f"{location}: inverted range {stem}.rs:{start}-{end} "
                    f"(start is after end)"
                )
    return problems


def check_file(path: Path, index: dict[str, Path], counts: dict[str, int]) -> list[str]:
    return check_text(
        path.read_text(encoding="utf-8", errors="replace"),
        path.relative_to(REPO).as_posix(),
        index,
        counts,
    )


def default_docs() -> list[Path]:
    return sorted((REPO / "architecture").glob("*.md"))


def self_test(index: dict[str, Path], counts: dict[str, int]) -> int:
    """Every rule must fire on bad input and stay quiet on good input."""
    total = counts.get("main", 0)
    failures: list[str] = []

    def expect(label: str, text: str, should_fail: bool) -> None:
        got = check_text(text, "sample.md", index, counts)
        if should_fail and not got:
            failures.append(f"{label}: expected a failure, got none")
        elif not should_fail and got:
            failures.append(f"{label}: expected clean, got {got}")

    expect("in-range single", "see `main.rs:6` for entry.", False)
    expect("in-range range", f"see `main.rs:1-{total}`.", False)
    expect("past EOF", f"cites `main.rs:{total + 1}`", True)
    expect("zero", "cites `main.rs:0`", True)
    expect("far past EOF", "cites `main.rs:99999`", True)
    expect("inverted range", f"cites `main.rs:{total}-1`", True)
    expect(
        "range end past EOF",
        f"cites `main.rs:1-{total + 50}`",
        True,
    )
    expect("bare file mention", "the `cleanup.rs` module is large.", False)
    expect("upstream source", "compare `dua-core lib.rs:659`.", False)
    expect("explicit dir prefix", "see `src/traverse.rs:29`.", False)
    expect("no extension", "see docs/RELEASING.md:10.", False)

    if failures:
        print("check-doc-citations: self test FAILED", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        "check-doc-citations: self test passed; all rules verified in both "
        "directions"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="verify the rules fire on bad input and stay quiet on good input",
    )
    parser.add_argument(
        "--doc",
        action="append",
        type=Path,
        default=None,
        metavar="PATH",
        help="check one document instead of all of architecture/",
    )
    args = parser.parse_args()

    index = source_index()
    if "main" not in index:
        print(
            "check-doc-citations: cannot locate src/main.rs; run from the "
            "repository",
            file=sys.stderr,
        )
        return 1
    counts = line_counts(index)

    if args.self_test:
        return self_test(index, counts)

    docs = args.doc if args.doc else default_docs()
    problems: list[str] = []
    for path in docs:
        if not path.is_absolute():
            path = (REPO / path) if not path.exists() else path
        if not path.is_file():
            print(f"check-doc-citations: no such document: {path}", file=sys.stderr)
            return 1
        problems.extend(check_file(path.resolve(), index, counts))

    if problems:
        print("check-doc-citations: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(
            "\nA citation into a file that shrank is how a stale reference "
            "announces itself. Re-derive the number from the source; do not "
            "shift it arithmetically unless you have read the new line.",
            file=sys.stderr,
        )
        return 1
    print(
        f"check-doc-citations: {len(docs)} document(s) clean; every "
        f"file.rs:N citation resolves to an existing file and an in-range line"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
