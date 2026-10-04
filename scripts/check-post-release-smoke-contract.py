#!/usr/bin/env python3
"""Keep the post-release smoke matrix honest against the release contract.

C014 requires the live smoke to cover the five release targets "when the
existing release runner matrix can execute them", and forbids creating "an
unguarded second target matrix". A hand-written matrix in a second workflow is
exactly that risk: it is plausible to add, drop, or repoint a runner here without
touching the release contract, and the rehearsal would then quietly cover
something the release does not ship, or stop covering something it does.

This check makes the second matrix a *projection* of the first:

1. the target set is exactly the release workflow's five build jobs;
2. each target's runner is identical to the release workflow's runner for it;
3. the smoke workflow is manually dispatched only, so it cannot run on a push;
4. it holds no write permission and names no release-publishing capability,
   because a rehearsal needs no publication credentials;
5. it dispatches the rehearsal script rather than reimplementing it.

Usage:
    python3 scripts/check-post-release-smoke-contract.py
    python3 scripts/check-post-release-smoke-contract.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RELEASE_WORKFLOW = ROOT / ".github/workflows/release-binaries.yml"
SMOKE_WORKFLOW = ROOT / ".github/workflows/post-release-smoke.yml"
REHEARSAL = ROOT / "scripts/post-release-smoke.sh"


def job_blocks(workflow: str) -> dict[str, str]:
    jobs: dict[str, str] = {}
    current: str | None = None
    buffer: list[str] = []
    for line in workflow.splitlines():
        match = re.match(r"^  ([A-Za-z_][\w-]*):\s*$", line)
        if match:
            if current is not None:
                jobs[current] = "\n".join(buffer)
            current, buffer = match.group(1), [line]
            continue
        if current is not None:
            buffer.append(line)
    if current is not None:
        jobs[current] = "\n".join(buffer)
    return jobs


def release_runner_map(workflow: str) -> dict[str, str]:
    """`release target -> runner` from the generated workflow's build jobs.

    The target is read from the build step's `--target` rather than derived from
    the job name, because the job name spells the triple with `_` where the
    triple has `-` and guessing that conversion produces `x86-64-apple-darwin`.
    The two Linux targets additionally carry a `.2.17` zigbuild glibc suffix that
    is not part of the target name, so it is stripped.
    """
    mapping: dict[str, str] = {}
    for job, body in job_blocks(workflow).items():
        if not job.startswith("build_"):
            continue
        runner = re.search(r'runs-on:\s*"?([^"\n]+)"?', body)
        target = re.search(r"--target'? '?([\w.-]+)", body)
        if runner is None or target is None:
            continue
        triple = re.sub(r"\.\d+\.\d+$", "", target.group(1))
        mapping[triple] = runner.group(1).strip()
    return mapping


def smoke_matrix(workflow: str) -> dict[str, str]:
    """`target -> runner` from the smoke workflow's `matrix.include`."""
    mapping: dict[str, str] = {}
    for match in re.finditer(
        r"-\s*target:\s*([A-Za-z0-9_.-]+)\s*\n\s*runner:\s*([A-Za-z0-9_.-]+)", workflow
    ):
        mapping[match.group(1)] = match.group(2)
    return mapping


def on_block(workflow: str) -> set[str]:
    """The top-level trigger keys of a workflow.

    Read structurally rather than by pattern: the `on:` block runs until a line
    that is not indented and not blank, so a `push:` nested under a job name
    cannot be mistaken for a trigger.
    """
    triggers: set[str] = set()
    in_block = False
    for line in workflow.splitlines():
        if not in_block:
            if line.rstrip() in ("on:", "on: "):
                in_block = True
            continue
        if not line.strip():
            continue
        if not line[0].isspace():
            break
        key = line.strip().rstrip(":").strip()
        if key and not key.startswith("-") and key not in ("{"):
            triggers.add(key)
    return triggers


def check(release: str, smoke: str) -> list[str]:
    problems: list[str] = []

    release_map = release_runner_map(release)
    if not release_map:
        problems.append("no build jobs with a runner were found in the release workflow")
        return problems
    smoke_map = smoke_matrix(smoke)
    if not smoke_map:
        problems.append("no matrix entries were found in the smoke workflow")
        return problems

    missing = sorted(set(release_map) - set(smoke_map))
    extra = sorted(set(smoke_map) - set(release_map))
    for target in missing:
        problems.append(
            f"smoke matrix omits release target {target}; the rehearsal would "
            f"not cover a target the release ships"
        )
    for target in extra:
        problems.append(
            f"smoke matrix rehearses {target}, which the release workflow does "
            f"not build; the rehearsal would cover something unpublished"
        )
    for target in sorted(set(release_map) & set(smoke_map)):
        if smoke_map[target] != release_map[target]:
            problems.append(
                f"smoke matrix runs {target} on {smoke_map[target]} but the "
                f"release builds it on {release_map[target]}; the rehearsal "
                f"would not be the same host"
            )

    if "workflow_dispatch" not in smoke:
        problems.append("smoke workflow is not manually dispatched")
    triggers = on_block(smoke)
    for trigger in ("push", "pull_request", "schedule"):
        if trigger in triggers:
            problems.append(
                f"smoke workflow is triggered by `{trigger}` rather than by hand; "
                f"a networked rehearsal must be deliberate"
            )
    if "fail-fast: false" not in smoke:
        problems.append(
            "smoke workflow sets fail-fast; one platform's network failure would "
            "cancel the other platforms' evidence"
        )

    write_scoped = re.search(r"contents:\s*write", smoke)
    if write_scoped:
        problems.append(
            "smoke workflow holds `contents: write`; a rehearsal reads public "
            "endpoints and needs no write scope"
        )
    for pattern, description in (
        (r"softprops/action-gh-release", "a release publishing action"),
        (r"gh\s+release\s+(create|edit|upload)", "the `gh release` client"),
        (r"cargo\s+publish", "a crates.io publish"),
    ):
        if re.search(pattern, smoke):
            problems.append(f"smoke workflow contains {description}")

    if "post-release-smoke.sh" not in smoke:
        problems.append(
            "smoke workflow does not dispatch scripts/post-release-smoke.sh; the "
            "rehearsal must not be reimplemented inline"
        )
    return problems


def self_test() -> int:
    """Prove the check rejects a drifted matrix rather than trusting the tree."""
    release = RELEASE_WORKFLOW.read_text(encoding="utf-8") if RELEASE_WORKFLOW.is_file() else ""
    smoke = SMOKE_WORKFLOW.read_text(encoding="utf-8") if SMOKE_WORKFLOW.is_file() else ""
    if not release or not smoke:
        print("check-post-release-smoke-contract: self test needs both workflows", file=sys.stderr)
        return 1

    failures: list[str] = []

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            verdict = "rejected" if problems else "accepted"
            wanted = "rejected" if should_fail else "accepted"
            failures.append(f"{name}: expected {wanted}, got {verdict}")
            print(f"  FAIL {name}: expected {wanted}, got {verdict}")
        else:
            print(f"  ok   {name}")

    def drop_first_target(text: str) -> str:
        # Structural, so the mutation cannot silently stop matching when a
        # runner name changes: a stale mutation string would make the case
        # report "accepted" and the self test would pass without testing.
        return re.sub(
            r"^\s*-\s*target:\s*\S+\n\s*runner:\s*\S+\n", "", text, count=1, flags=re.MULTILINE
        )

    def add_trigger(text: str, trigger: str) -> str:
        return re.sub(
            r"^on:\s*$", f"on:\n  {trigger}:\n    branches: [main]", text, count=1, flags=re.MULTILINE
        )

    expect("the real pair is accepted", check(release, smoke), False)
    expect(
        "a dropped target is rejected",
        check(release, drop_first_target(smoke)),
        True,
    )
    expect(
        "a dropped target really was dropped",
        check(release, smoke + "\n"),
        False,
    )
    expect(
        "an added target is rejected",
        check(
            release,
            smoke.replace(
                "          - target: x86_64-unknown-linux-gnu\n            runner: ubuntu-latest\n",
                "          - target: x86_64-unknown-linux-gnu\n            runner: ubuntu-latest\n"
                "          - target: riscv64gc-unknown-linux-gnu\n            runner: ubuntu-latest\n",
                1,
            ),
        ),
        True,
    )
    expect(
        "a repointed runner is rejected",
        check(
            release,
            re.sub(r"runner: windows-latest", "runner: macos-14", smoke, count=1),
        ),
        True,
    )
    for trigger in ("push", "pull_request", "schedule"):
        expect(f"a {trigger} trigger is rejected", check(release, add_trigger(smoke, trigger)), True)
    expect("write scope is rejected", check(release, smoke.replace("contents: read", "contents: write")), True)
    expect("fail-fast is rejected", check(release, smoke.replace("fail-fast: false", "fail-fast: true")), True)
    expect(
        "an inline rehearsal is rejected",
        check(release, smoke.replace("scripts/post-release-smoke.sh", "curl -O https://example.invalid/x")),
        True,
    )
    expect(
        "a publish step is rejected",
        check(release, smoke + "      - run: gh release create v0.1.2\n"),
        True,
    )

    if failures:
        print(
            f"check-post-release-smoke-contract: self test FAILED ({len(failures)} case(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "check-post-release-smoke-contract: self test passed; the matrix is "
        "mechanically bound to the release contract"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help=__doc__)
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not REHEARSAL.is_file():
        print(f"check-post-release-smoke-contract: FAILED: {REHEARSAL} is missing", file=sys.stderr)
        return 1
    problems = check(
        RELEASE_WORKFLOW.read_text(encoding="utf-8"),
        SMOKE_WORKFLOW.read_text(encoding="utf-8"),
    )
    if problems:
        print("check-post-release-smoke-contract: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    mapping = smoke_matrix(SMOKE_WORKFLOW.read_text(encoding="utf-8"))
    print(
        f"check-post-release-smoke-contract: {len(mapping)} target(s) match the "
        f"release runners ({', '.join(f'{t}={r}' for t, r in sorted(mapping.items()))})"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
