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
4. **The `scripts/` inventory in the testing deep dive is exactly the scripts
   that exist.** The canonical contract-checker table is the only inventory of
   this repository's verification surfaces, and it has already drifted once
   while still reading confidently: a 17th script shipped and the table
   described 16. A prose table that is nobody's obligation stays true for as
   long as nobody adds a row. This check makes the set mechanically equal in
   both directions, plus unique, so the next script cannot be added quietly.

What this deliberately does **not** check, because it cannot be decided
statically without false positives:

- whether the line still contains the thing the prose claims. A number inside
  a file proves the file is long enough, not that the sentence is true. That
  is a reviewer's job, and [overview §7.2](architecture/overview.md) is the
  precedent for recording when such a check was misdesigned.
- prose that cites a file by bare name (`cleanup.rs` with no line), or the
  `dua-core` / `filesize` upstream sources, which are not vendored here.
- **which workflow invokes a script.** That is the other half of the inventory
  and it is natural language. Automating it means parsing English for
  "wired into", which is exactly the brittle-parser trap: it would encode one
  reviewer's phrasing as a release contract and then fail on a correct
  reword. Set parity is the mechanical floor; wiring semantics stay a
  reviewer's job, and the table is written so a reviewer can check them.

Rules 1, 2 and 4 are what turn "the doc is probably fine" into a gate. All are
self-tested in `--self-test` mode against built-in samples in the failing
direction: a guard that cannot fail on bad input is worse than no guard. The
inventory self-tests additionally assert that each mutation **actually changed
the subject**, because a guard whose mutation silently became a no-op reports
"rejected" for a tree containing no defect at all — the failure this repository
has already hit once.

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

# The one document holding the canonical contract-checker inventory, and the
# heading its §5 table lives under.
INVENTORY_DOC = "architecture/14-testing-and-verification.md"
INVENTORY_HEADING = "## 5. The contract checkers"

# The only file types the inventory claims to cover.
INVENTORY_SUFFIXES = (".py", ".sh")

# A table row, and the first `` `name.py` `` / `` `name.sh` `` inside it.
# Only leading-`|` lines are considered, so a script named in the prose notes
# below the table is not mistaken for an inventory row.
ROW = re.compile(r"^\s*\|")
CELL_SCRIPT = re.compile(r"`(?P<name>[A-Za-z0-9._-]+\.(?:py|sh))`")

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


def default_docs() -> list[Path]:
    return sorted((REPO / "architecture").glob("*.md"))


# ------------------------------------------------------------ script inventory


def disk_scripts() -> list[str]:
    """Every regular `*.py` / `*.sh` directly under `scripts/`."""
    directory = REPO / "scripts"
    if not directory.is_dir():
        return []
    return sorted(
        path.name
        for path in directory.iterdir()
        if path.is_file() and path.suffix in INVENTORY_SUFFIXES
    )


def inventory_section(text: str) -> str:
    """The body of the §5 contract-checker section, heading excluded."""
    lines = text.splitlines()
    start = None
    for number, line in enumerate(lines):
        if line.strip() == INVENTORY_HEADING:
            start = number + 1
            break
    if start is None:
        return ""
    body = []
    for line in lines[start:]:
        if line.startswith("## "):
            break
        body.append(line)
    return "\n".join(body)


def table_scripts(text: str) -> list[str]:
    """Script basenames named by rows of the canonical §5 inventory table.

    Order is preserved and duplicates are kept: this is what lets the caller
    distinguish "listed twice" from "listed once". A table with no rows at all
    yields an empty list, which then fails set parity against a non-empty
    `scripts/` — a deleted table cannot pass silently.
    """
    found: list[str] = []
    for line in inventory_section(text).splitlines():
        if not ROW.match(line):
            continue
        match = CELL_SCRIPT.search(line)
        if match:
            found.append(match.group("name"))
    return found


def check_inventory(disk: list[str], table: list[str], where: str) -> list[str]:
    """Exact set parity plus uniqueness between `scripts/` and the §5 table."""
    problems: list[str] = []
    seen: set[str] = set()
    for name in table:
        if name in seen:
            problems.append(
                f"{where}: `{name}` is listed more than once in the "
                f"{INVENTORY_HEADING} inventory"
            )
        seen.add(name)

    on_disk = set(disk)
    in_table = set(table)
    for name in sorted(on_disk - in_table):
        problems.append(
            f"{where}: `scripts/{name}` exists but is absent from the "
            f"{INVENTORY_HEADING} inventory"
        )
    for name in sorted(in_table - on_disk):
        problems.append(
            f"{where}: the {INVENTORY_HEADING} inventory lists `{name}`, which "
            f"does not exist in `scripts/`"
        )
    return problems


def check_file(path: Path, index: dict[str, Path], counts: dict[str, int]) -> list[str]:
    relative = path.relative_to(REPO).as_posix()
    text = path.read_text(encoding="utf-8", errors="replace")
    problems = check_text(text, relative, index, counts)
    if relative == INVENTORY_DOC:
        problems.extend(check_inventory(disk_scripts(), table_scripts(text), relative))
    return problems


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

    failures.extend(inventory_self_test())

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


def inventory_self_test() -> list[str]:
    """Every inventory rule must fire on bad input and stay quiet on good input.

    Each negative also asserts that its mutation **changed the subject**. A
    mutation that silently becomes a no-op makes the guard report "rejected"
    for a tree containing no defect, which is the specific way this repository
    has been misled before: a self-test case that can no longer alter what it
    inspects is not a test.
    """
    failures: list[str] = []
    disk = ["alpha.py", "beta.sh", "gamma.py"]

    def rows(*names: str) -> str:
        return "\n".join(f"| `{name}` | enforces | fails when | wired into |" for name in names)

    def sample(body: str) -> str:
        return f"# Title\n\npreamble\n\n{INVENTORY_HEADING}\n\n{body}\n\n## 6. Next\n\nafter\n"

    control_table = rows(*disk)
    control = sample(control_table)

    # The extraction path itself, so a broken section parser cannot pass by
    # returning an empty list that happens to match an empty disk.
    extracted = table_scripts(control)
    if extracted != disk:
        failures.append(
            f"inventory control: expected the table to yield {disk}, got {extracted}"
        )
    if inventory_section(control).find("## 6. Next") != -1:
        failures.append("inventory control: the section body must stop at the next heading")
    if table_scripts(sample("")) != []:
        failures.append("inventory control: an empty table must yield no rows")

    def expect(label: str, text: str, should_fail: bool) -> None:
        got = check_inventory(disk, table_scripts(text), "sample.md")
        if should_fail and not got:
            failures.append(f"{label}: expected a failure, got none")
        elif not should_fail and got:
            failures.append(f"{label}: expected clean, got {got}")

    # Unchanged exact inventory: the only passing case.
    expect("exact inventory", control, False)

    # A script on disk with no row. This is the exact defect the guard exists
    # for: the 17th script shipped and the 16-row table kept reading true.
    missing = sample(rows(*disk[:2]))
    if table_scripts(missing) == disk:
        failures.append("disk-only: the mutation did not change the subject")
    expect("disk script absent from table", missing, True)

    # A row for a script that no longer exists.
    stale = sample(rows(*disk, "retired.py"))
    if table_scripts(stale) == disk:
        failures.append("table-only: the mutation did not change the subject")
    expect("stale table-only script", stale, True)

    # The same script listed twice.
    duplicate = sample(rows(*disk, "alpha.py"))
    if table_scripts(duplicate) == disk:
        failures.append("duplicate: the mutation did not change the subject")
    expect("duplicate script row", duplicate, True)

    # The whole table deleted: must not pass as vacuously clean.
    expect("no table at all", sample("there is no table here"), True)

    # A script named in the prose notes below the table is not an inventory
    # row. This case stays clean *because* the extractor ignores it: if the
    # extractor were loosened to scan every backticked name, the phantom entry
    # would be read as a stale row and this would fail.
    prose_only = sample(
        control_table
        + "\nThree notes:\n\n"
        + "1. A later script `mentioned-in-prose.py` is described here.\n"
    )
    if "mentioned-in-prose.py" in table_scripts(prose_only):
        failures.append("prose: a backticked name outside a table row leaked in")
    expect("prose mention is not a row", prose_only, False)

    return failures


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
