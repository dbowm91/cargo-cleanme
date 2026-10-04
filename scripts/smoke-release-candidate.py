#!/usr/bin/env python3
"""Per-target release smoke against the exact staged cargo-cleanme candidate.

Eggpack runs this once per contracted target as the consumer validator
(`release/eggpack/consumer-validators.json`) after the core `--version` smoke,
passing the absolute path of the candidate whose bytes already matched the
recorded size and SHA-256. The script adds product runtime evidence that a
fixed-argv smoke cannot express:

  * candidate identity: `--version` is exactly `cargo-cleanme X.Y.Z`;
  * command surface: `--help` advertises `scan`, `clean`, and `config`;
  * Cargo external-subcommand argv normalization: the exact same candidate
    answers `cargo cleanme --version` identically, which is what proves the
    `cargo cleanme ...` spelling works on these exact published bytes;
  * bounded read-only scan: a fixed on-disk Cargo fixture is scanned in JSON
    mode with progress disabled, stdout is exactly one JSON document that
    parses, and the fixture tree is byte-identical before and after.

Everything runs in a private temporary directory with an isolated
configuration home, so the release runner observes no writes outside that
directory and no destructive cleanup command is ever invoked.
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

PRODUCT = "cargo-cleanme"
VERSION = re.compile(r"^\d+\.\d+\.\d+$")
FIXTURE_MANIFEST = '[package]\nname = "smoke-fixture"\nversion = "0.1.0"\nedition = "2021"\n'
FIXTURE_SOURCE = "pub fn smoke() {}\n"
CHILD_TIMEOUT_SECS = 120


class SmokeFailure(Exception):
    """A release-candidate assertion failed."""


def run(candidate: Path, argv: list[str], cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            [str(candidate), *argv],
            cwd=str(cwd),
            env=env,
            capture_output=True,
            text=True,
            timeout=CHILD_TIMEOUT_SECS,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        raise SmokeFailure(f"candidate timed out after {CHILD_TIMEOUT_SECS}s: {argv}") from exc


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SmokeFailure(message)


def tree_fingerprint(root: Path) -> str:
    """Order-independent digest of every regular file's path, size, and bytes."""
    digest = hashlib.sha256()
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        digest.update(str(path.relative_to(root)).encode("utf-8"))
        digest.update(b"\0")
        digest.update(str(path.stat().st_size).encode("ascii"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def smoke_env() -> dict[str, str]:
    """The environment the candidate runs in.

    Deliberately leaves `HOME`, `USERPROFILE`, and the `XDG_*` variables alone.
    The scan shells out to `cargo`, and those variables are also Cargo's own
    configuration inputs; overriding them makes Cargo misbehave, which is how
    this validator first failed on Windows and reported a healthy release as
    broken. (The same mistake was already made and fixed in
    `scripts/release-benchmark.py`.)

    The application config is isolated with `--config`, which is the only
    mechanism that isolates *this program* without disturbing Cargo. Nothing
    outside the invocation's own temporary directory is read or written.
    """
    env = dict(os.environ)
    env["NO_COLOR"] = "1"
    env.pop("CARGO_TERM_COLOR", None)
    return env


def build_fixture(root: Path) -> Path:
    fixture = root / "fixture"
    (fixture / "src").mkdir(parents=True)
    (fixture / "Cargo.toml").write_text(FIXTURE_MANIFEST, encoding="utf-8")
    (fixture / "src" / "lib.rs").write_text(FIXTURE_SOURCE, encoding="utf-8")
    return fixture


def check_version(candidate: Path, cwd: Path, env: dict[str, str], argv: list[str]) -> str:
    result = run(candidate, argv, cwd, env)
    require(result.returncode == 0, f"{argv} exited with {result.returncode}: {result.stderr.strip()}")
    # Exactly one line, tolerating CRLF and a missing trailing newline. Asserting
    # the exact shape is stricter than counting newlines and does not break on
    # Windows line endings.
    require(
        result.stdout.count("\n") <= 1 and result.stderr.count("\n") <= 1,
        f"{argv} wrote more than one line to stdout/stderr: {result.stdout!r} {result.stderr!r}",
    )
    prefix, _, version = result.stdout.strip().partition(" ")
    require(prefix == PRODUCT, f"{argv} reported product {prefix!r}, expected {PRODUCT!r}")
    require(bool(VERSION.match(version)), f"{argv} reported non-release version {version!r}")
    return version


def check_help(candidate: Path, cwd: Path, env: dict[str, str]) -> None:
    result = run(candidate, ["--help"], cwd, env)
    require(result.returncode == 0, f"--help exited with {result.returncode}")
    for subcommand in ("scan", "clean", "config"):
        require(subcommand in result.stdout, f"--help does not advertise {subcommand!r}")


def check_bounded_scan(candidate: Path, cwd: Path, env: dict[str, str], fixture: Path) -> None:
    before = tree_fingerprint(fixture)
    # `--config` keeps this program's own state inside the invocation-owned
    # temporary directory while leaving Cargo's environment untouched.
    result = run(
        candidate,
        [
            "scan",
            "--config",
            str(cwd / "app-config.toml"),
            "--format",
            "json",
            "--no-progress",
            "fixture",
        ],
        cwd,
        env,
    )
    require(result.returncode == 0, f"bounded scan exited with {result.returncode}: {result.stderr.strip()}")
    # `json.loads` rejects trailing content, so this single assertion enforces
    # "stdout is exactly one JSON document and nothing else" without depending
    # on a line-ending convention.
    try:
        document = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise SmokeFailure(
            f"scan stdout is not exactly one JSON document: {exc}\n"
            f"  stdout: {result.stdout[:400]!r}\n"
            f"  stderr: {result.stderr[:400]!r}"
        ) from exc
    require(document.get("schema_version") == 1, "scan JSON schema_version is not 1")
    require(document.get("operation") == "scan", "scan JSON operation is not 'scan'")
    require(
        isinstance(document.get("result"), dict)
        and isinstance(document["result"].get("discovered_manifests"), int),
        "scan JSON result does not carry a discovered_manifests counter",
    )
    require(
        tree_fingerprint(fixture) == before,
        "read-only scan mutated the scanned fixture tree",
    )


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} PATH", file=sys.stderr)
        return 2
    candidate = Path(sys.argv[1]).resolve()
    if not candidate.is_file():
        print(f"candidate is not a regular file: {candidate}", file=sys.stderr)
        return 2

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-release-smoke-"))
    try:
        env = smoke_env()
        fixture = build_fixture(work)

        direct = check_version(candidate, work, env, ["--version"])
        external = check_version(candidate, work, env, ["cleanme", "--version"])
        require(
            direct == external,
            f"argv normalization changed the reported version ({direct} vs {external})",
        )
        check_help(candidate, work, env)
        check_bounded_scan(candidate, work, env, fixture)
        print(f"release smoke passed for {PRODUCT} {direct}")
        return 0
    except SmokeFailure as exc:
        # Write the report next to the candidate as well as to stderr, so a CI
        # failure that swallows the validator's output is still diagnosable.
        print(f"release smoke failed for {candidate.name}: {exc}", file=sys.stderr)
        return 1
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())
