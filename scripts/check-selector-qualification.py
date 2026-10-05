#!/usr/bin/env python3
"""Keep the Cargo selector support claim, the qualification matrix, and the
real-Cargo qualification script in exact agreement (M011D).

The product already fails closed for an unknown Cargo version:
`workspace::clean_capabilities_from_version` returns an all-false capability
struct, so `clean --profile`/`--package` degrade to whole-target scope instead
of guessing. That is the right runtime posture, and it is also why a stale
allowlist is invisible: nothing crashes when the list drifts from reality, the
product just quietly stops offering a selector.

Before M011D the allowlist lived only in `src/workspace.rs`, the real-Cargo
characterization script defaulted to `1.89 1.91 1.92 stable`, and nothing
connected the two. A capability claim with no evidence lifecycle is a claim
nobody can check. This check makes the claim a *projection* of one checked-in
policy file, exactly as `check-post-release-smoke-contract.py` makes the smoke
matrix a projection of the release runners.

Four artifacts must agree:

1. `release/selector-qualification.json` — the machine-readable authority.
   Profile-qualified, package-qualified, and exploratory versions are expressed
   separately, because a package-qualified version has stricter configured-target
   and shared-dependency conditions than a profile-qualified one.
2. `src/workspace.rs` — the runtime allowlist. This is what the product does,
   so it is the one that must be proved, not the one that gets to define the
   truth.
3. `.github/workflows/qualify-cargo-selectors.yml` — the hosted matrix. It
   names its toolchains explicitly rather than reading them at run time, so a
   dropped version is a detectable drift rather than a silent absence.
4. `scripts/qualify-cargo-selectors.sh` — the assertion-oriented real-Cargo
   qualification script, which must consume the policy instead of carrying a
   second, drifting copy of the version list.

Guards:

- every entry is an exact `X.Y.Z` release. `1.99` and `1.9` are *not* `1.99.0`,
  and a string match that conflated them would enable a selector for a Cargo
  whose behavior was never observed.
- `package_selector` is a subset of `profile_selector`; a package-qualified
  release that is not profile-qualified is an inconsistent claim.
- `exploratory` and the supported lists are disjoint. An exploratory version
  that leaked into the runtime allowlist would convert a research signal into a
  silent support claim, which is precisely the automatic promotion M011D
  forbids.
- the workflow runs the script, pins every action to an immutable revision,
  holds no write scope, never edits the allowlist, and keeps the exploratory
  lane out of the supported lane's pass/fail.
- the script carries no hardcoded version list of its own.

Every guard is exercised in the failing direction by `--self-test`, including a
control that proves each mutation actually changed the artifact. A guard that
stopped guarding is worse than no guard.

Usage:
    python3 scripts/check-selector-qualification.py
    python3 scripts/check-selector-qualification.py --self-test
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
POLICY = ROOT / "release/selector-qualification.json"
WORKSPACE_RS = ROOT / "src/workspace.rs"
WORKFLOW = ROOT / ".github/workflows/qualify-cargo-selectors.yml"
SCRIPT = ROOT / "scripts/qualify-cargo-selectors.sh"

CAPABILITY_FN = "clean_capabilities_from_version"
EXACT_VERSION = re.compile(r"^\d+\.\d+\.\d+$")


# ------------------------------------------------------------------ artifacts


def policy_from(text: str) -> dict:
    try:
        return json.loads(text)
    except json.JSONDecodeError as error:
        raise ValueError(f"selector policy is not valid JSON: {error}") from error


def _balanced_arg(text: str, start: int) -> str | None:
    """The text between the `(` at `start` and its matching `)`.

    Bracket matching rather than a regex to the next `)`, because the allowlist
    arms contain no nested parentheses today and a future comment or refactor
    that adds one would otherwise silently truncate the captured expression into
    something that parses but means something else.
    """
    open_at = text.find("(", start)
    if open_at == -1:
        return None
    depth = 0
    for index in range(open_at, len(text)):
        char = text[index]
        if char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return text[open_at + 1 : index]
    return None


def function_body(source: str) -> str | None:
    marker = f"pub fn {CAPABILITY_FN}("
    at = source.find(marker)
    if at == -1:
        return None
    brace = source.find("{", at)
    if brace == -1:
        return None
    depth = 0
    for index in range(brace, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[brace + 1 : index]
    return None


def _literals(expression: str | None) -> list[str] | None:
    if expression is None:
        return None
    return re.findall(r'"([^"]*)"', expression)


def rust_allowlist(source: str) -> tuple[list[str] | None, list[str] | None]:
    """`(profile, package)` version literals from the runtime capability table."""
    body = function_body(source)
    if body is None:
        return None, None
    profile_at = body.find("profile_selector = matches!(")
    if profile_at == -1:
        return None, None
    package_at = body.find("package_selector: matches!(")
    if package_at == -1:
        return None, None
    return (
        _literals(_balanced_arg(body, profile_at)),
        _literals(_balanced_arg(body, package_at)),
    )


# -------------------------------------------------------------------- parsing


def on_block(workflow: str) -> set[str]:
    """Top-level trigger keys. Structural, so a nested `push:` under a job name
    cannot be mistaken for a trigger."""
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
        if key and not key.startswith("-") and key not in ("{",):
            triggers.add(key)
    return triggers


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


def toolchain_list(job_body: str) -> list[str] | None:
    """Every Cargo release this job actually hands to the toolchain action.

    The action takes ONE singular `toolchain` input, so the workflow installs
    each version in its own step and the matrix is the union of those steps.
    Reading the values the action genuinely receives is stricter than reading a
    list in a comment or a variable: a version mentioned in the file but never
    passed to the action is not installed, and not installed means not
    qualified.
    """
    values = re.findall(r"^\s*toolchain:\s*'([^']*)'\s*$", job_body, re.MULTILINE)
    if not values:
        return None
    if any(not value.strip() for value in values):
        return None
    return values


def plural_toolchain_inputs(workflow: str) -> list[str]:
    """`toolchains:` keys — an input the toolchain action does not have.

    This is the defect that kept the M011D gate permanently red: passing a JSON
    list under a key the action does not declare leaves its singular `toolchain`
    input empty, and the action exits with "'toolchain' is a required input"
    before any qualification step runs. Nothing else in this file would notice,
    because a gate that never runs cannot report a drift.
    """
    return re.findall(r"^\s*toolchains:\s*\S+", workflow, re.MULTILINE)


def pinned_actions(workflow: str) -> list[str]:
    """Every `uses:` that is not pinned to a full 40-hex revision."""
    unpinned: list[str] = []
    for reference in re.findall(r"uses:\s*\"?([^\s\"']+)", workflow):
        if "@" not in reference:
            continue
        _, _, revision = reference.partition("@")
        if not re.fullmatch(r"[0-9a-f]{40}", revision):
            unpinned.append(reference)
    return unpinned


# --------------------------------------------------------------------- checks


def check(source: str, policy: dict, workflow: str, script: str) -> list[str]:
    problems: list[str] = []

    for key in ("profile_selector", "package_selector", "exploratory"):
        if not isinstance(policy.get(key), list):
            problems.append(f"policy has no `{key}` list; the three lists are the authority")
            return problems

    profile = list(policy["profile_selector"])
    package = list(policy["package_selector"])
    exploratory = list(policy["exploratory"])

    for label, versions in (("profile_selector", profile), ("package_selector", package)):
        for version in versions:
            if not EXACT_VERSION.match(version):
                problems.append(
                    f"policy {label} lists {version!r}, which is not an exact X.Y.Z release; "
                    "a partial version would match a Cargo whose behavior was never observed"
                )
        if len(set(versions)) != len(versions):
            problems.append(f"policy {label} repeats a version; the list is a set, not a tally")

    outside = sorted(set(package) - set(profile))
    if outside:
        problems.append(
            f"policy package_selector claims {', '.join(outside)} but profile_selector does not; "
            "a package-qualified release is also profile-qualified by construction"
        )
    leaked = sorted(set(exploratory) & (set(profile) | set(package)))
    if leaked:
        problems.append(
            f"policy lists {', '.join(leaked)} as both exploratory and supported; an exploratory "
            "result is a research signal and must never be a support claim"
        )
    if not profile:
        problems.append("policy claims no profile-qualified version at all")

    rust_profile, rust_package = rust_allowlist(source)
    if rust_profile is None or rust_package is None:
        problems.append(
            f"could not read the {CAPABILITY_FN} allowlist out of {WORKSPACE_RS.name}; the "
            "runtime table is what this check exists to prove, so an unreadable one is a "
            "failure rather than a skip"
        )
    else:
        if sorted(rust_profile) != sorted(profile):
            problems.append(
                "the runtime allowlist and the policy disagree about profile selection.\n"
                f"      runtime: {', '.join(rust_profile) or '(none)'}\n"
                f"      policy:  {', '.join(profile) or '(none)'}"
            )
        if sorted(rust_package) != sorted(package):
            problems.append(
                "the runtime allowlist and the policy disagree about package selection.\n"
                f"      runtime: {', '.join(rust_package) or '(none)'}\n"
                f"      policy:  {', '.join(package) or '(none)'}"
            )
        for version in rust_profile + rust_package:
            if not EXACT_VERSION.match(version):
                problems.append(
                    f"the runtime allowlist contains {version!r}, which is not an exact X.Y.Z "
                    "release; exact-version matching is the fail-closed property"
                )
        for version in exploratory:
            if version in rust_profile or version in rust_package:
                problems.append(
                    f"the runtime allowlist enables selectors for exploratory toolchain "
                    f"{version!r}; exploratory evidence is never a support claim"
                )

    jobs = job_blocks(workflow)
    if not jobs:
        problems.append("no jobs were found in the qualification workflow")
        return problems

    for offender in plural_toolchain_inputs(workflow):
        problems.append(
            f"the workflow passes {offender.strip()!r} to the toolchain action, which has no "
            "`toolchains` input; its singular `toolchain` input would be empty and the job "
            "would exit before qualifying anything"
        )

    supported_job = next(
        (body for body in jobs.values() if "supported" in body and "toolchain:" in body), None
    )
    if supported_job is None:
        supported_job = next((body for body in jobs.values() if "toolchain:" in body), None)
    exploratory_job = next((body for body in jobs.values() if "exploratory" in body), None)

    if supported_job is None:
        problems.append("the qualification workflow has no supported-matrix job that installs toolchains")
    else:
        matrix = toolchain_list(supported_job)
        if matrix is None:
            problems.append(
                "the supported matrix installs no toolchain: it has no non-empty singular "
                "`toolchain:` input for the toolchain action"
            )
        else:
            missing = sorted(set(profile) - set(matrix))
            extra = sorted(set(matrix) - set(profile))
            for version in missing:
                problems.append(
                    f"the hosted matrix omits profile-qualified Cargo {version}; the product "
                    "claims a selector no run ever exercised"
                )
            for version in extra:
                problems.append(
                    f"the hosted matrix qualifies Cargo {version}, which the policy does not "
                    "claim; a green run for an unclaimed version is not evidence for anything"
                )

    if exploratory_job is None:
        problems.append("the workflow has no separate exploratory lane for current stable Cargo")
    else:
        matrix = toolchain_list(exploratory_job)
        if matrix is None:
            problems.append("the exploratory lane installs no toolchain")
        else:
            unknown = sorted(set(matrix) - set(exploratory))
            if unknown:
                problems.append(
                    f"the exploratory lane runs {', '.join(unknown)}, which the policy does not "
                    "list as exploratory"
                )
        if "continue-on-error" not in exploratory_job:
            problems.append(
                "the exploratory lane is not marked continue-on-error; a stable-Cargo regression "
                "is a research signal and must not turn the supported matrix red"
            )

    triggers = on_block(workflow)
    if "workflow_dispatch" not in triggers:
        problems.append("the qualification workflow cannot be dispatched for explicit requalification")
    if "schedule" not in triggers:
        problems.append(
            "the qualification workflow has no schedule; toolchain availability drift is the "
            "failure mode a recurring lane exists to detect"
        )
    for trigger in ("pull_request_target", "workflow_run"):
        if trigger in triggers:
            problems.append(f"the qualification workflow is triggered by `{trigger}`, which it has no business running on")

    unpinned = pinned_actions(workflow)
    for reference in unpinned:
        problems.append(
            f"the qualification workflow uses {reference!r} without an immutable revision pin"
        )

    if re.search(r"contents:\s*write", workflow):
        problems.append("the qualification workflow holds `contents: write`; qualification is read-only")
    for pattern, description in (
        (r"\bgit\s+commit\b", "a git commit"),
        (r"contents:\s*write", "a write scope"),
        (r"gh\s+release\s+(create|edit|upload|publish)", "a release publication"),
    ):
        if re.search(pattern, workflow):
            problems.append(f"the qualification workflow contains {description}; it must never edit the support allowlist")

    if "qualify-cargo-selectors.sh" not in workflow:
        problems.append(
            "the qualification workflow does not run scripts/qualify-cargo-selectors.sh; the "
            "real-Cargo assertions must not be reimplemented in YAML"
        )
    if "check-selector-qualification.py" not in workflow:
        problems.append(
            "the qualification workflow does not run this checker first; a hosted run that "
            "qualifies a matrix the policy does not claim is evidence for nothing"
        )

    # The script must read the policy rather than carry a second copy of the
    # list. Before M011D it defaulted to `versions=(1.89 1.91 1.92 stable)`,
    # which was a second authority that disagreed with the first.
    if "selector-qualification.json" not in script:
        problems.append(
            "the qualification script does not read release/selector-qualification.json; a "
            "hardcoded version list is a second authority that will drift"
        )
    for match in re.finditer(r"versions=\(([^)]*)\)", script):
        literal = match.group(1).strip()
        if literal:
            problems.append(
                f"the qualification script hardcodes `versions=({literal})`; the policy file is "
                "the only source of the version matrix"
            )

    return problems


# ------------------------------------------------------------------ self-test


def self_test() -> int:
    source = WORKSPACE_RS.read_text(encoding="utf-8")
    workflow = WORKFLOW.read_text(encoding="utf-8")
    script = SCRIPT.read_text(encoding="utf-8")
    try:
        policy = policy_from(POLICY.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        print(f"check-selector-qualification: self test needs the policy: {error}", file=sys.stderr)
        return 1

    failures: list[str] = []

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            failures.append(name)
            print(f"  FAIL {name}: expected {'rejected' if should_fail else 'accepted'}, got {'rejected' if problems else 'accepted'}")
        else:
            print(f"  ok   {name}")

    def with_policy(**overrides) -> dict:
        clone = json.loads(json.dumps(policy))
        clone.update(overrides)
        return clone

    def drop_version(versions: list[str], version: str) -> list[str]:
        return [v for v in versions if v != version]

    baseline_profile = policy["profile_selector"][0]
    baseline_package = policy["package_selector"][0]

    expect("the checked-in set is accepted", check(source, policy, workflow, script), False)

    # Control: the mutations below must actually change what they target, or a
    # "rejected" verdict would only be proving that the mutation was a no-op.
    expect(
        "control: a dropped version really was dropped",
        check(source, with_policy(profile_selector=drop_version(policy["profile_selector"], baseline_profile)), workflow, script),
        True,
    )
    expect(
        "control: an unchanged policy is still accepted",
        check(source, with_policy(profile_selector=policy["profile_selector"]), workflow, script),
        False,
    )

    # Missing supported version from the hosted matrix. Each version is its own
    # step now, so dropping one means removing that step's `toolchain:` line
    # rather than editing a list.
    shrunk = re.sub(
        rf"^\s*toolchain:\s*'{re.escape(baseline_profile)}'\s*$\n",
        "",
        workflow,
        count=1,
        flags=re.MULTILINE,
    )
    if shrunk == workflow:
        failures.append("control: dropping a supported version actually removed its step")
        print("  FAIL control: the version-drop mutation was a no-op")
    else:
        print("  ok   control: dropping a supported version actually removed its step")
    expect("a supported version missing from the matrix is rejected", check(source, policy, shrunk, script), True)

    # The plural key. This is the exact defect that kept this gate red: the
    # action has no `toolchains` input, so its singular `toolchain` input is
    # empty and the job exits before any qualification runs. A gate that never
    # runs cannot report a drift, which is why it needs its own rule.
    plural = workflow.replace(
        f"          toolchain: '{baseline_profile}'",
        f"          toolchains: '[\"{baseline_profile}\"]'",
        1,
    )
    if plural == workflow:
        failures.append("control: the plural-input mutation actually applied")
        print("  FAIL control: the plural-input mutation was a no-op; the anchor no longer matches")
    else:
        print("  ok   control: the plural-input mutation actually applied")
    expect(
        "a plural `toolchains:` input to the toolchain action is rejected",
        check(source, policy, plural, script),
        True,
    )

    # An empty singular input is the same failure with a different spelling.
    blanked = workflow.replace(
        f"          toolchain: '{baseline_profile}'",
        "          toolchain: ''",
        1,
    )
    expect(
        "an empty singular `toolchain:` input is rejected",
        check(source, policy, blanked, script),
        True,
    )

    # Exploratory version promoted into the runtime allowlist. The anchor is a
    # `| "-prefixed` arm rather than the first entry, because the first entry of
    # a `matches!` arm list carries no leading pipe and a mutation built on it
    # would silently match nothing.
    anchored = policy["profile_selector"][1]
    promoted = source.replace(
        f'            | "{anchored}"',
        f'            | "{anchored}"\n            | "stable"',
        1,
    )
    # Control: if the anchor string were reformatted, this mutation would become
    # a no-op and the case below would report "rejected" for a tree that still
    # contains no promotion at all. Asserted directly, because the control is
    # "the mutation happened", not "the checker rejected something".
    if promoted == source:
        failures.append("control: the promotion actually reached the runtime allowlist")
        print("  FAIL control: the promotion mutation was a no-op; the anchor string no longer matches")
    else:
        print("  ok   control: the promotion actually reached the runtime allowlist")
    expect(
        "an unchanged allowlist is still accepted",
        check(source, policy, workflow, script),
        False,
    )
    expect(
        "an exploratory toolchain in the runtime allowlist is rejected",
        check(promoted, policy, workflow, script),
        True,
    )

    # Profile/package disagreement.
    expect(
        "package selection claimed without profile selection is rejected",
        check(source, with_policy(profile_selector=drop_version(policy["profile_selector"], baseline_package)), workflow, script),
        True,
    )
    expect(
        "profile and package lists disagreeing is rejected",
        check(
            source,
            with_policy(package_selector=drop_version(policy["package_selector"], baseline_package)),
            workflow,
            script,
        ),
        True,
    )

    # Exact-version parsing: 1.99 must not stand in for 1.99.0.
    partial = source.replace(f'"{baseline_profile}"', '"1.99"', 1)
    expect("a partial runtime version is rejected", check(partial, policy, workflow, script), True)
    expect(
        "a partial policy version is rejected",
        check(source, with_policy(profile_selector=["1.99"]), workflow, script),
        True,
    )

    # Workflow guards.
    expect(
        "an unpinned action is rejected",
        check(source, policy, workflow.replace("actions/setup-python@", "actions/setup-python@v5_") , script),
        True,
    )
    expect(
        "write scope is rejected",
        check(source, policy, workflow.replace("contents: read", "contents: write"), script),
        True,
    )
    expect(
        "dropping the schedule is rejected",
        check(source, policy, re.sub(r"^  schedule:\n(?:    .*\n)+", "", workflow, flags=re.MULTILINE), script),
        True,
    )
    expect(
        "dropping the dispatch trigger is rejected",
        check(source, policy, re.sub(r"^  workflow_dispatch:\n", "", workflow, flags=re.MULTILINE), script),
        True,
    )
    expect(
        "an allowlist edit inside the workflow is rejected",
        check(source, policy, workflow + "      - run: git commit -am 'add cargo 1.100'\n", script),
        True,
    )
    expect(
        "an inline reimplementation of qualification is rejected",
        check(source, policy, workflow.replace("scripts/qualify-cargo-selectors.sh", "cargo clean --profile dev"), script),
        True,
    )
    expect(
        "a hardcoded version list in the script is rejected",
        check(source, policy, workflow, script + "\nversions=(1.89 1.91 stable)\n"),
        True,
    )
    expect(
        "a script that no longer reads the policy is rejected",
        check(source, policy, workflow, script.replace("selector-qualification.json", "hardcoded-list.txt")),
        True,
    )
    expect(
        "an unreadable runtime allowlist is rejected rather than skipped",
        check("pub fn unrelated() {}", policy, workflow, script),
        True,
    )

    if failures:
        print(
            f"check-selector-qualification: self test FAILED ({len(failures)} case(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "check-selector-qualification: self test passed; the runtime allowlist, the "
        "policy, the hosted matrix, and the script are one claim"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove each guard still fails")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    missing = [p for p in (POLICY, WORKSPACE_RS, WORKFLOW, SCRIPT) if not p.is_file()]
    if missing:
        for path in missing:
            print(f"check-selector-qualification: FAILED: {path} is missing", file=sys.stderr)
        return 1
    try:
        policy = policy_from(POLICY.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        print(f"check-selector-qualification: FAILED: {error}", file=sys.stderr)
        return 1

    problems = check(
        WORKSPACE_RS.read_text(encoding="utf-8"),
        policy,
        WORKFLOW.read_text(encoding="utf-8"),
        SCRIPT.read_text(encoding="utf-8"),
    )
    if problems:
        print("check-selector-qualification: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        f"check-selector-qualification: {len(policy['profile_selector'])} profile-qualified and "
        f"{len(policy['package_selector'])} package-qualified Cargo release(s) agree across the "
        "runtime allowlist, the policy, the hosted matrix, and the script"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
