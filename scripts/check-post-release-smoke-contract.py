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


def _event_block(workflow: str, event: str) -> str:
    """The body of one top-level event block, e.g. `release:` under `on:`."""
    lines: list[str] = []
    in_block = False
    capturing = False
    for line in workflow.splitlines():
        if not in_block:
            if line.strip() in ("on:", "on: "):
                in_block = True
            continue
        if not line.strip():
            continue
        if not line[0].isspace():
            break
        stripped = line.strip().rstrip(":").strip()
        if stripped == event:
            capturing = True
            lines.append(line)
            continue
        if capturing:
            if re.match(r"^[ ]+[A-Za-z_][\w-]*:", line) and (len(line) - len(line.lstrip(" "))) <= 2:
                break
            lines.append(line)
    return "\n".join(lines)


# A YAML comment cannot execute anything, so a comment that names a script or a
# forbidden command must not be able to satisfy (or trip) a content check.
YAML_COMMENT = re.compile(r"(?m)^\s*#.*$")


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

    # Exactly two entry points (M011C): the manual rehearsal, and a real
    # published stable release. Anything else -- a push, a schedule, a
    # pull_request -- would rehearse bytes that were never published, and a green
    # result would read as release evidence.
    triggers = on_block(smoke)
    if "workflow_dispatch" not in triggers:
        problems.append(
            "smoke workflow lost its manual dispatch; the explicit rehearsal surface must remain"
        )
    if "release" not in triggers:
        problems.append(
            "smoke workflow is not triggered by release publication; without it the smoke runs "
            "only when an operator remembers, which is how C016 and C017 stayed live as long as "
            "they did"
        )
    for trigger in ("push", "pull_request", "pull_request_target", "schedule", "workflow_run"):
        if trigger in triggers:
            problems.append(
                f"smoke workflow is triggered by `{trigger}`; a networked rehearsal of published "
                "bytes must run only for a published release or an explicit dispatch"
            )
    if "release" in triggers:
        release_block = _event_block(smoke, "release")
        if "published" not in release_block:
            problems.append(
                "the release trigger does not filter on the `published` type; other release "
                "activity (created, edited, prereleased) would launch a smoke for bytes that are "
                "not a published stable release"
            )
        for forbidden in ("created", "edited", "deleted", "prereleased", "released", "published "):
            if forbidden in release_block and forbidden.strip() != "published":
                problems.append(
                    f"the release trigger also fires on `{forbidden.strip()}`; only `published` "
                    "describes a release whose bytes an operator could actually install"
                )

    # A `release: types: [published]` trigger is necessary but not sufficient:
    # the payload must be re-validated, because a malformed or unexpected event
    # would otherwise move the smoke onto a draft or a pre-release.
    if "release" in triggers:
        if "github.event.release.draft" not in smoke or "github.event.release.prerelease" not in smoke:
            problems.append(
                "the smoke workflow does not re-check the release event's draft/prerelease state; "
                "trusting the trigger type alone is trusting a payload this workflow must verify"
            )
        if "TAG" not in smoke or "v[0-9]+" not in smoke:
            problems.append(
                "the smoke workflow does not assert that the release tag is a plain stable "
                "vX.Y.Z; a pre-release or floating tag would make the transition unreproducible"
            )

    # The transition must be resolved and waited for by the tested helper, not by
    # shell heuristics in YAML -- a resolver embedded in a workflow is a resolver
    # with no self test.
    if "resolve-smoke-transition.py" not in smoke:
        problems.append(
            "smoke workflow does not call scripts/resolve-smoke-transition.py; an embedded shell "
            "predecessor heuristic is a second implementation with no premise tests"
        )
    else:
        if "--wait-crates-io" not in smoke:
            problems.append(
                "the automatic path does not wait boundedly for crates.io; publication is ordered "
                "GitHub-release-first, so a smoke can fire before the version the updater resolves "
                "actually exists"
            )
        if "resolve-smoke-transition.py --self-test" not in smoke:
            problems.append(
                "the smoke workflow does not run the transition resolver's --self-test before "
                "using it; an unproved resolver is a shell heuristic wearing a test's name"
            )
        if not re.search(r"needs:\s*resolve-transition", smoke):
            problems.append(
                "the matrix job does not depend on the resolve-transition job; five independently "
                "resolved pairs are five chances to test different versions while the summary "
                "claims one"
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
        (r"gh\s+release\s+(create|edit|upload|delete)", "the `gh release` client"),
        # Command-shaped, so the workflow's own prose explaining that publication
        # is ordered GitHub-first and `cargo publish`-second does not trip the
        # guard. A bare word match would have rejected the file that documents
        # the constraint.
        (r"(?:^|[;&|]\s*|&&|\|\|)\s*(?:sudo\s+)?cargo\s+publish\b", "a crates.io publish"),
    ):
        if re.search(pattern, smoke, re.MULTILINE):
            problems.append(f"smoke workflow contains {description}")

    # Command-shaped for the same reason the publication patterns above are:
    # a comment that *names* the rehearsal is documentation, and a workflow that
    # only mentions the script in prose has still computed a transition and run
    # nothing. Comparing against comment-stripped content is what makes this the
    # false green it is meant to catch -- a bare substring test was defeated by a
    # comment explaining the tag-prefix handoff during M013.
    if "post-release-smoke.sh" not in YAML_COMMENT.sub("", smoke):
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

    # --- M011C: the automatic-path premises -------------------------------
    #
    # These are the guards that keep "automated" from meaning "a rehearsal that
    # runs on something that was never published, or that resolves 'latest', or
    # that skips when crates.io is slow".

    def expect_control(name: str, mutated: str, original: str) -> None:
        if mutated == original:
            failures.append(name)
            print(f"  FAIL {name}: the mutation was a no-op")
        else:
            print(f"  ok   {name}")

    control_dropped_release = smoke.replace("  release:\n    types: [published]\n", "")
    expect_control(
        "control: the missing-release-trigger mutation applied",
        control_dropped_release,
        smoke,
    )
    expect(
        "a missing release trigger is rejected",
        [p for p in check(release, control_dropped_release) if "release" in p],
        True,
    )

    widened = smoke.replace("types: [published]", "types: [created, edited, published]")
    expect_control("control: the widened-release-filter mutation applied", widened, smoke)
    expect(
        "a release trigger that fires on non-publication activity is rejected",
        check(release, widened),
        True,
    )

    for trigger in ("workflow_run", "pull_request_target"):
        expect(
            f"a {trigger} trigger is rejected",
            check(release, add_trigger(smoke, trigger)),
            True,
        )

    dropped_dispatch = re.sub(
        r"^  workflow_dispatch:\n(?:    .*\n|      .*\n)+", "", smoke, count=1, flags=re.MULTILINE
    )
    expect_control("control: the missing-dispatch mutation applied", dropped_dispatch, smoke)
    expect(
        "a workflow that cannot be dispatched by hand is rejected",
        check(release, dropped_dispatch),
        True,
    )

    no_crates_wait = smoke.replace("--wait-crates-io ", "")
    expect_control("control: the missing-crates-io-wait mutation applied", no_crates_wait, smoke)
    expect(
        "an automatic path with no crates.io wait is rejected",
        check(release, no_crates_wait),
        True,
    )

    no_resolver = smoke.replace("scripts/resolve-smoke-transition.py", "bash -c 'resolve()'")
    expect_control("control: the inlined-resolver mutation applied", no_resolver, smoke)
    expect(
        "a resolver reimplemented in the workflow is rejected",
        check(release, no_resolver),
        True,
    )

    no_resolver_selftest = smoke.replace(
        "python3 scripts/resolve-smoke-transition.py --self-test\n", ""
    )
    expect_control(
        "control: the missing-resolver-self-test mutation applied", no_resolver_selftest, smoke
    )
    expect(
        "a resolver used without its own --self-test is rejected",
        check(release, no_resolver_selftest),
        True,
    )

    untrusted_payload = smoke.replace(
        "          IS_DRAFT: ${{ github.event.release.draft }}", "          IS_DRAFT: 'false'"
    ).replace(
        "          IS_PRERELEASE: ${{ github.event.release.prerelease }}",
        "          IS_PRERELEASE: 'false'",
    )
    expect_control("control: the untrusted-payload mutation applied", untrusted_payload, smoke)
    expect(
        "an unvalidated release payload is rejected",
        check(release, untrusted_payload),
        True,
    )

    no_dependency = smoke.replace("    needs: resolve-transition\n", "")
    expect_control("control: the missing-dependency mutation applied", no_dependency, smoke)
    expect(
        "a matrix job that does not consume the resolved pair is rejected",
        check(release, no_dependency),
        True,
    )

    # A workflow that computes a transition but never runs the rehearsal is the
    # exact false green this milestone exists to prevent: the resolve job goes
    # green, the matrix job is removed, and the summary claims five lanes.
    no_rehearsal = smoke.replace("bash scripts/post-release-smoke.sh", "echo nothing to do")
    expect_control("control: the never-executed-rehearsal mutation applied", no_rehearsal, smoke)
    expect(
        "a workflow that computes but never executes the rehearsal is rejected",
        check(release, no_rehearsal),
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
