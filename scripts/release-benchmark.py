#!/usr/bin/env python3
"""Release benchmark baseline for cargo-cleanme.

Two kinds of evidence, kept deliberately separate because they have different
failure meanings:

**Semantic counters are hard regression evidence.** Manifests discovered and
resolved, Cargo process counts, output groups, and cleanup-unit counts are
deterministic for a fixed fixture, so a change is a real behavior change and
fails the release gate.

**Wall-clock phase timings are trend evidence only.** They are recorded and
diffed for visibility, never used as a hosted pass/fail threshold, because a
shared runner's timing is dominated by noise that has nothing to do with this
code. A timing regression is a question to ask, not a failure to report.

The fixture is built here rather than being borrowed from the test suite so the
numbers describe a documented, reproducible workload instead of whatever a test
happens to construct. Run:

    cargo build --release
    python3 scripts/release-benchmark.py            # write/refresh the baseline
    python3 scripts/release-benchmark.py --check    # compare, fail on counter drift
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASELINE = ROOT / "release" / "baseline-benchmark.json"
BINARY = ROOT / "target" / "release" / "cargo-cleanme"

# Phase timings are recorded but never gated.
TIMING_KEYS = (
    "discovery",
    "locate",
    "metadata",
    "source_activity",
    "output_sizing",
    "elapsed",
)

# Deterministic semantic counters, keyed by the names `--stats` actually emits.
# These are the hard gate.
#
#   manifests / workspaces  discovery and resolution reach
#   locate / metadata       Cargo process invocations; a filtered or simulated
#                           path must spawn zero of them
#   groups_measured         output groups that produced a measurement
#   reportable              cleanup units a destructive run would consider
#   bytes                   inventory estimate
COUNTER_KEYS = (
    "manifests",
    "workspaces",
    "locate",
    "metadata",
    "groups_measured",
    "reportable",
    "bytes",
    "failures",
    "unresolved_ownership",
)


def build_fixture(root: Path) -> Path:
    """A fixed Cargo-shaped workspace with two inactive projects."""
    projects = root / "projects"
    for index, (package, artifacts, size) in enumerate(
        [("alpha", 3, 4096), ("beta", 5, 8192)], start=1
    ):
        project = projects / package
        (project / "src").mkdir(parents=True)
        (project / "Cargo.toml").write_text(
            f'[package]\nname = "{package}"\nversion = "0.1.0"\nedition = "2021"\n',
            encoding="utf-8",
        )
        (project / "src" / "lib.rs").write_text("pub fn f() {}\n", encoding="utf-8")
        for slot in range(artifacts):
            target = project / "target" / "debug" / f"dep-{slot}.o"
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(bytes([(index + slot) % 256]) * size)
    return projects


UNIT_SCALE = {"ms": 1e-3, "s": 1.0, "us": 1e-6, "ns": 1e-9}


def parse_stats(stderr: str) -> dict:
    """Extract the semantic counters and phase timings from `--stats` output.

    `--stats` writes one `scan stats: k=v ...` line where every phase carries
    its own unit, so timings are normalized to seconds rather than compared as
    bare numbers.
    """
    line = next((l for l in stderr.splitlines() if l.startswith("scan stats:")), "")

    # `locate` and `metadata` each appear twice: once as a Cargo process count
    # and once as a phase duration. One combined key/value map silently loses
    # one of each, so a counter can be recorded as a float. Unit-suffixed values
    # are the timings; unitless values are the counters.
    counters: dict[str, int] = {
        key: int(value)
        for key, value in re.findall(r"(\w+)=(\d+)(?![0-9.]*(?:ms|s|us|ns)\b)", line)
    }
    timings: dict[str, float] = {}
    for key, value, unit in re.findall(r"(\w+)=([0-9.]+)(ms|s|us|ns)\b", line):
        timings[key] = float(value) * UNIT_SCALE.get(unit, 1.0)
    # `locate` and `metadata` are both a Cargo process count and a phase
    # duration in the same line. They are returned in separate maps with
    # distinct names, because merging them would let the duration silently
    # overwrite the count and turn a hard counter into a float.
    observed = {f"counter_{key}": counters.get(key, 0) for key in COUNTER_KEYS}
    observed.update({f"timing_{key}": timings.get(key, 0.0) for key in TIMING_KEYS})
    return observed


def run_once(fixture: Path) -> tuple[dict, int, str]:
    env = dict(os.environ)
    env["NO_COLOR"] = "1"
    # The app's own config is isolated with `--config`, not by overriding
    # HOME or XDG_CONFIG_HOME. Both of those are also Cargo's configuration
    # inputs, and this Cargo build emits nothing for `locate-project` when
    # XDG_CONFIG_HOME points somewhere new. Overriding them would make the
    # benchmark measure a broken scan instead of the real one.
    app_config = fixture.parent / "app-config.toml"
    result = subprocess.run(
        [
            str(BINARY),
            "scan",
            "--config",
            str(app_config),
            "--format",
            "json",
            "--no-progress",
            "--stats",
            str(fixture),
        ],
        capture_output=True,
        text=True,
        env=env,
        timeout=600,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(f"scan failed with {result.returncode}: {result.stderr.strip()[:400]}")
    stats = parse_stats(result.stderr)
    return stats, result.returncode, result.stdout


def binary_size() -> int:
    return BINARY.stat().st_size


def dependency_count() -> int | None:
    """`cargo tree` normal-edge count, or None when Cargo could not answer.

    A failed `cargo tree` must not be recorded as a count of zero: the baseline
    and the printed comparison would then carry a number that came from a
    broken tool rather than from the dependency graph. It is not a hard gate
    (only the semantic counters are), but a reported figure has to be a real
    figure.
    """
    result = subprocess.run(
        ["cargo", "tree", "--edges", "normal", "--prefix", "none"],
        capture_output=True,
        text=True,
        cwd=str(ROOT),
        check=False,
    )
    if result.returncode != 0:
        return None
    return len({line.strip() for line in result.stdout.splitlines() if line.strip()})


def collect() -> dict:
    if not BINARY.is_file():
        raise SystemExit(
            f"{BINARY} does not exist; run `cargo build --release` first"
        )
    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-benchmark-"))
    try:
        fixture = build_fixture(work)
        stats, _, stdout = run_once(fixture)
        # The JSON contract must stay exactly one document; a benchmark that
        # silently produced two would make every number here meaningless.
        document = json.loads(stdout)
        if document.get("schema_version") != 1:
            raise SystemExit("scan JSON schema_version is not 1")
        return {
            "fixture": {
                "projects": 2,
                "artifacts": 8,
                "artifact_bytes": 4096 * 3 + 8192 * 5,
            },
            "counters": {key: stats[f"counter_{key}"] for key in COUNTER_KEYS},
            "timings_observed_seconds": {key: stats[f"timing_{key}"] for key in TIMING_KEYS},
            "release_binary_bytes": binary_size(),
            "normal_dependency_count": dependency_count(),
        }
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare against the checked-in baseline and fail on counter drift",
    )
    args = parser.parse_args()

    current = collect()

    if not args.check:
        BASELINE.write_text(json.dumps(current, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {BASELINE.relative_to(ROOT)}")
        for key in COUNTER_KEYS:
            print(f"  {key}: {current['counters'][key]}")
        print(f"  release_binary_bytes: {current['release_binary_bytes']}")
        print(f"  normal_dependency_count: {current['normal_dependency_count']}")
        return 0

    if not BASELINE.is_file():
        print(
            f"no baseline at {BASELINE.relative_to(ROOT)}; run without --check",
            file=sys.stderr,
        )
        return 1
    recorded = json.loads(BASELINE.read_text(encoding="utf-8"))

    drift = []
    for key in COUNTER_KEYS:
        before = recorded.get("counters", {}).get(key)
        after = current["counters"][key]
        if before != after:
            drift.append(f"{key}: {before} -> {after}")

    print("semantic counters (hard gate):")
    for key in COUNTER_KEYS:
        mark = "  " if recorded.get("counters", {}).get(key) == current["counters"][key] else "!!"
        print(f" {mark} {key}: {recorded.get('counters', {}).get(key)} -> {current['counters'][key]}")
    print("wall-clock timings (trend only, never gated):")
    for key in TIMING_KEYS:
        before = recorded.get("timings_observed_seconds", {}).get(key, 0.0)
        after = current["timings_observed_seconds"][key]
        print(f"    {key}: {before:.4f}s -> {after:.4f}s")
    print(f"release binary: {recorded.get('release_binary_bytes')} -> {current['release_binary_bytes']} bytes")
    observed_dependencies = current["normal_dependency_count"]
    if observed_dependencies is None:
        print(
            f"normal dependencies: {recorded.get('normal_dependency_count')} -> "
            f"unavailable (cargo tree did not succeed; not a gate)"
        )
    else:
        print(
            f"normal dependencies: {recorded.get('normal_dependency_count')} -> "
            f"{observed_dependencies}"
        )

    if drift:
        print("\nsemantic counter drift:", file=sys.stderr)
        for line in drift:
            print(f"  {line}", file=sys.stderr)
        return 1
    print("\nno semantic counter drift")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
