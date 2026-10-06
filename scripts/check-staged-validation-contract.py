#!/usr/bin/env python3
"""Guard the staged-release validation workflow's premises (M011B).

`scripts/validate-staged-release.py` is the repository's only end-to-end check
that staged bytes equal qualified bytes: exact inventory, served-byte SHA-256,
release-manifest agreement, contract agreement, measured Linux ABI floors, and a
real installer run. It has caught real release-process defects repeatedly, and
until M011B it was wired into nowhere.

Hosting it is not the hard part. Hosting it *unsoundly* would be worse than not
hosting it, because an unsound gate converts an unavailable check into a
documented pass. The dangerous failure shapes, in order of how quietly they fail:

- It triggers on `push`, so an arbitrary contributor branch runs the validator
  and a green result reads as release evidence.
- It checks out the default branch instead of the upstream run's head SHA, so it
  validates whatever `main` currently contains rather than the source the draft
  was staged from. This one is especially quiet: the validator really does run,
  really does pass, and attests to nothing.
- It drops the upstream `workflow_dispatch` guard, so a re-run of an arbitrary
  workflow_run is treated as a release run.
- It drops the upstream conclusion guard, so a failed stage run with no draft
  produces a validation claim about bytes that were never staged.
- It gains `contents: write` or a `gh release publish`, which would hand a
  product-owned validation job the publication authority that
  `release-binaries.yml` deliberately withholds.
- It reimplements staged validation in YAML instead of calling the validator, so
  the drift gate on the real script stops meaning anything.

Each of those is a premise, and this check fails in every one of its failure
directions under `--self-test`, including a control that proves each mutation
actually changed the workflow.

Usage:
    python3 scripts/check-staged-validation-contract.py
    python3 scripts/check-staged-validation-contract.py --self-test
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/validate-staged-release.yml"
EGGPACK_WORKFLOW = "Eggpack candidate builds"
VALIDATOR = "validate-staged-release.py"
IDENTITY_CHECK = "check-release-identity.py"

# A bare command word followed by whitespace and an argument that could name a
# tag. `gh release publish v0.1.7` matches; `gh release view` does not. The
# alternative -- scanning for the bare word `publish` -- also matches the
# workflow's own comments explaining that it must never publish, which would
# make this check reject the very file that documents the constraint.
# Every way this file could actually publish. Since M011B the validation job
# holds `contents: write` -- GitHub serves drafts only to push-level
# identities, so read scope cannot fetch the draft at all -- which moves the
# guarantee from "this job cannot publish" to "this file does not publish".
# The second is the one that can be enforced, so it is enforced completely:
# the direct CLI, the API under any verb spelling, and the registry.
PUBLICATION = re.compile(
    r"\bgh\s+release\s+(?:publish|create|edit|delete|upload)\b"
    r"|\bcurl\b[^\n]*releases[^\n]*-(?:X-HTTP-Method-Override:\s*)?PATCH"
    # `gh api` puts the verb and the path in either order, and the first
    # version of this rule only matched one of them -- which is why the
    # self-test's API case fails rather than passing.
    r"|\bgh\s+api\b[^\n]*(?:-X\s*(?:POST|PATCH|PUT|DELETE)\b[^\n]*releases"
    r"|releases[^\n]*-X\s*(?:POST|PATCH|PUT|DELETE)\b)"
    r"|\bcargo\s+publish\b"
    r"|\bgit\s+push\b[^\n]*--tags\b"
)
# A YAML comment is not executed, so it cannot publish anything. This file's own
# comments name the forbidden commands in order to explain why they are
# forbidden, and scanning comments would reject the documentation of the rule
# along with violations of it -- a guard that cannot state its own rule.
YAML_COMMENT = re.compile(r"(?m)^\s*#.*$")


def executed_lines(workflow: str) -> str:
    """The workflow with its full-line YAML comments removed."""
    return YAML_COMMENT.sub("", workflow)


PENDING_REVISION = re.compile(r"\$\{\{[^}]*\}\}")
UPCASE = re.compile(r"\$UPSTREAM_(?:CONCLUSION|EVENT)\}|\$\{UPSTREAM_[A-Z_]+\}")


def _dedent_block(text: str) -> str:
    """`run: |` / `run: >-` script bodies, with YAML block indentation removed."""
    blocks: list[str] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        match = re.match(r"^(\s*)run:\s*\|+\s*$", lines[index])
        if not match:
            index += 1
            continue
        base = len(match.group(1))
        index += 1
        body: list[str] = []
        while index < len(lines):
            line = lines[index]
            if line.strip() and (len(line) - len(line.lstrip(" "))) <= base:
                break
            body.append(line[base + 2 :] if line.strip() else "")
            index += 1
        blocks.append("\n".join(body))
    return "\n".join(blocks)


def _dedent_within(text: str, key: str) -> list[str]:
    blocks: list[str] = []
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        match = re.match(rf"^(\s*){re.escape(key)}:\s*\|+\s*$", lines[index])
        if not match:
            index += 1
            continue
        base = len(match.group(1))
        index += 1
        body: list[str] = []
        while index < len(lines):
            line = lines[index]
            if line.strip() and (len(line) - len(line.lstrip(" "))) <= base:
                break
            body.append(line[base + 2 :] if line.strip() else "")
            index += 1
        blocks.append("\n".join(body))
    return blocks


def _step_body(workflow: str, step_line: str) -> str:
    """The lines belonging to one workflow step: the `- uses:`/`- name:` entry
    plus every line indented deeper than it."""
    lines = workflow.splitlines()
    try:
        start = lines.index(step_line)
    except ValueError:
        return ""
    indent = len(step_line) - len(step_line.lstrip(" "))
    body = [step_line]
    for line in lines[start + 1 :]:
        if not line.strip():
            body.append(line)
            continue
        if (len(line) - len(line.lstrip(" "))) <= indent:
            break
        body.append(line)
    return "\n".join(body)


def on_block(workflow: str) -> set[str]:
    """Top-level trigger keys, structurally.

    A nested `push:` under a job name is not a trigger, and a trigger list
    written inline (`on: [push]`) has a different shape entirely; both are
    handled so the checker cannot be defeated by a reformat.
    """
    triggers: set[str] = set()
    in_block = False
    for line in workflow.splitlines():
        stripped = line.strip()
        if not in_block:
            if stripped in ("on:", "on: "):
                in_block = True
            elif re.match(r"^on:\s*\[", stripped):
                triggers |= {
                    part.strip().strip("'\"")
                    for part in stripped[len("on:") :].strip("[] ").split(",")
                    if part.strip()
                }
            continue
        if not stripped:
            continue
        if not line[0].isspace():
            break
        key = stripped.rstrip(":").strip()
        if key and not key.startswith("-") and key != "{":
            triggers.add(key)
    return triggers


def on_block_text(workflow: str) -> str:
    lines: list[str] = []
    in_block = False
    for line in workflow.splitlines():
        if not in_block:
            if line.strip() in ("on:", "on: "):
                in_block = True
            continue
        if not line.strip():
            continue
        if not line[0].isspace():
            break
        lines.append(line)
    return "\n".join(lines)


def check(workflow: str) -> list[str]:
    problems: list[str] = []

    triggers = on_block(workflow)
    if "workflow_run" not in triggers:
        problems.append(
            "the validation workflow does not subscribe to workflow_run; it must observe the "
            "Eggpack run that staged the draft, not a source push"
        )
    if "workflow_dispatch" not in triggers:
        problems.append(
            "the validation workflow has no manual dispatch path; an operator must be able to "
            "re-run validation for an exact tag after a transient failure"
        )
    for forbidden in ("push", "pull_request", "pull_request_target", "schedule"):
        if forbidden in triggers:
            problems.append(
                f"the validation workflow is triggered by `{forbidden}`; a validation run on an "
                "arbitrary branch would read as release evidence for a release nobody staged"
            )

    header = on_block_text(workflow)
    if not re.search(rf"workflows:\s*\['{re.escape(EGGPACK_WORKFLOW)}'\]", header):
        if EGGPACK_WORKFLOW not in header:
            problems.append(
                f"the workflow_run trigger does not name the upstream workflow "
                f"{EGGPACK_WORKFLOW!r} exactly; a typo would leave this workflow subscribed to "
                "nothing and silently never run"
            )
    if not re.search(r"types:\s*\[completed\]", header):
        problems.append(
            "the workflow_run trigger does not require the completed type; without it the "
            "workflow races the upstream run instead of observing its result"
        )
    if re.search(r"types:\s*\[[^\]]*requested[^\]]*\]", header):
        problems.append(
            "the workflow_run trigger fires on `requested`, which is a claim that the upstream "
            "run was accepted, not that it succeeded"
        )

    # The job must READ a draft release, and GitHub serves drafts only to
    # identities with push access, so `contents: read` cannot do that and the
    # narrowest scope that can is `contents: write`. That was discovered the
    # hard way during M013: the read scope made the validator structurally
    # unable to fetch the artifact it exists to validate.
    #
    # So the invariant is no longer "this job cannot publish" -- it cannot be
    # expressed, since the job must hold write. It is "this file does not
    # publish", which is enforceable and is enforced below. The remaining
    # permissions stay forbidden: a read-and-validate job has no business with
    # actions, id-token (which would let it mint a credential), packages, or
    # deployments.
    permissions_block = re.search(r"^permissions:\s*\n((?:\s+.*\n)*)", workflow, re.MULTILINE)
    if permissions_block is None:
        problems.append(
            "the validation workflow declares no `permissions:` block; a workflow without an "
            "explicit scope can inherit the repository default, which may be read/write"
        )
    else:
        declared = permissions_block.group(1)
        if not re.search(r"^\s+contents:\s*(?:read|write)\s*$", declared, re.MULTILINE):
            problems.append(
                "the validation workflow does not narrow `contents` explicitly to read or "
                "write; an undeclared scope inherits the repository default"
            )
        for capability in ("actions:", "id-token:", "packages:", "deployments:"):
            if capability in declared:
                problems.append(
                    f"the validation workflow requests `{capability}`; least privilege for a "
                    "read-and-validate job is `contents` and nothing else"
                )

    if PUBLICATION.search(executed_lines(workflow)):
        problems.append(
            "the validation workflow contains a publication command; publication is a separate "
            "human action, and since the job must hold `contents: write` to read a draft, the "
            "absence of such a command is the only thing keeping publication out of its hands"
        )

    # The checkout must bind to the upstream head SHA. Checking out the default
    # branch is the quietest failure in this file: the validator runs, passes,
    # and describes bytes that were never staged.
    # A pinned `actions/checkout`, located by line rather than by one
    # whole-step regex. The other workflows in this repository put a trailing
    # `# v4` comment on the `uses:` line, and a regex that ends the match at the
    # newline captured nothing at all -- a checker that silently matches zero
    # steps would then report "no checkout" forever.
    checkout_lines = [
        line
        for line in workflow.splitlines()
        if re.search(r"uses:\s*[\"']?actions/checkout@[0-9a-f]{40}(?![\w.-])", line)
    ]
    if not checkout_lines:
        problems.append("the validation workflow has no pinned actions/checkout step")
    else:
        joined = _step_body(workflow, checkout_lines[0])
        if "ref:" not in joined:
            problems.append(
                "the checkout does not pin a `ref`; without one it checks out the default "
                "branch, which is not the source the draft was staged from"
            )
        else:
            if "head_sha" not in joined:
                problems.append(
                    "the checkout `ref` does not bind to the upstream run's `head_sha`; "
                    "validating a checkout of the default branch attests to nothing about the "
                    "staged bytes"
                )
            if not re.search(r"github\.event\.workflow_run\.head_sha", joined):
                problems.append(
                    "the checkout `ref` never evaluates the upstream `head_sha` expression, so "
                    "the binding is syntactic rather than effective"
                )
            # `github.sha` may legitimately appear as a *fallback* in the same
            # expression, so it is only a problem when it is the only ref
            # source. Rejecting every occurrence would have flagged the checked-in
            # workflow, which uses it exactly for the manual-dispatch path where
            # there is no upstream run to read a head SHA from.
            ref_lines = [line for line in joined.splitlines() if line.strip().startswith("ref:")]
            if ref_lines:
                for line in ref_lines:
                    if "github.event.workflow_run.head_sha" in line:
                        continue
                    if re.search(r"\$\{\{\s*github\.(ref|sha)\s*\}\}", line):
                        problems.append(
                            "a checkout `ref` uses this workflow's own ref or sha with no upstream "
                            "head SHA binding; on a workflow_run trigger that checks out the "
                            "default branch rather than the staged source"
                        )

    unpinned = [
        reference
        for reference in re.findall(r"uses:\s*\"?([^\s\"']+)", workflow)
        if "@" in reference and not re.fullmatch(r"[0-9a-f]{40}", reference.partition("@")[2])
    ]
    for reference in unpinned:
        problems.append(
            f"the validation workflow uses {reference!r} without an immutable revision pin"
        )

    # The upstream event/conclusion guards must be *evaluated*, not merely
    # mentioned in a comment. `UPCASE` is the check for that: a comparison
    # against a literal string is a guard, a mention in prose is not.
    scripts = _dedent_block(workflow)
    if "UPSTREAM_EVENT" not in scripts or "UPSTREAM_CONCLUSION" not in scripts:
        problems.append(
            "the validation workflow does not read the upstream run's event and conclusion; "
            "a validation claim must be conditioned on a deliberate, successful upstream run"
        )
    else:
        for variable, expected in (("UPSTREAM_EVENT", "workflow_dispatch"), ("UPSTREAM_CONCLUSION", "success")):
            if not re.search(
                rf'"{variable}"\s*!=\s*"{re.escape(expected)}"', scripts
            ) and not re.search(rf'\${variable}"?\s*!=\s*"', scripts):
                problems.append(
                    f"the workflow never compares the upstream {variable} against "
                    f"{expected!r}; a guard that is not compared guards nothing"
                )

    if not re.search(rf"timeout-minutes:\s*\d+", workflow):
        problems.append(
            "the validation workflow declares no timeout; a hung validator must not hold a "
            "concurrency slot for the repository default duration"
        )

    if VALIDATOR not in workflow:
        problems.append(
            f"the validation workflow never invokes scripts/{VALIDATOR}; the single "
            "end-to-end staged-bytes validator must be the thing that runs"
        )
    if IDENTITY_CHECK not in workflow:
        problems.append(
            f"the validation workflow never invokes scripts/{IDENTITY_CHECK}; deriving a tag "
            "from the checkout is not the same as re-proving the tag/source binding, and only "
            "the existing exact-identity gate is the authority for that"
        )
    if "--tag" not in workflow:
        problems.append(
            "the validation workflow does not pass an exact `--tag`; a validator invoked without "
            "one cannot bind itself to a specific release"
        )

    # A validator failure must propagate. `run: python3 …` alone would stop on
    # failure anyway, but the plan requires the gate be explicit, and a
    # validator piped into `tee` without `pipefail` reports tee's status.
    if VALIDATOR in scripts:
        # Match the actual shell form, not the bare word. A substring check on
        # `pipefail` would be satisfied by a comment mentioning it, and would
        # also make any test mutation awkward to write without accidentally
        # leaving the word behind — a guard whose subject it cannot see is a
        # guard that stopped guarding.
        if not re.search(r"set\s+-[a-zA-Z]*o[a-zA-Z]*\s+pipefail|set\s+-o\s+pipefail", scripts):
            problems.append(
                "the validator's output is not run under `set -o pipefail`; a validator failure "
                "piped into `tee` would report the pipeline's success instead of its own"
            )
        # Either spelling is fine: a braced `${PIPESTATUS[0]}` or a bare
        # `$PIPESTATUS[0]`. Requiring the braced form alone would have rejected a
        # correct workflow, which is the failure mode of a checker that guards
        # syntax instead of behaviour.
        if not re.search(r"\bPIPESTATUS\s*\[\s*0\s*\]", scripts):
            problems.append(
                "the validation step reads no pipeline status, so the validator's own exit "
                "code is discarded in favour of the last command in the pipeline"
            )
        if re.search(r"\|\s*tee\b", scripts) and not re.search(r"\bexit\b[^\n]*status", scripts):
            problems.append(
                "the validator is piped into `tee` but its status is never propagated to an "
                "`exit`; the step would report success whenever `tee` succeeds"
            )

    # Executing downloaded code. A `download && execute` on one line, or across
    # a small window, is the shape that matters: the validator and the policy
    # must come from the trusted tag checkout, never from a release asset or a
    # workflow artifact.
    for line in workflow.splitlines():
        if not re.search(r"\b(gh\s+release\s+download|actions/download-artifact|curl\b.*releases)", line):
            continue
        window = "\n".join(
            workflow.splitlines()[workflow.splitlines().index(line) : workflow.splitlines().index(line) + 3]
        )
        if re.search(r"\|\s*(sudo\s+)?(ba|z|k|da)?sh\b", window) or re.search(
            r"&&\s*(sudo\s+)?(ba|z|k|da)?sh\b", window
        ):
            problems.append(
                "the validation workflow downloads release or artifact content and executes it; "
                "the validator and the policy must come from the trusted tag checkout"
            )
            break

    return problems


def self_test() -> int:
    try:
        workflow = WORKFLOW.read_text(encoding="utf-8")
    except OSError as error:
        print(f"check-staged-validation-contract: self test needs the workflow: {error}", file=sys.stderr)
        return 1

    failures: list[str] = []

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            failures.append(name)
            print(
                f"  FAIL {name}: expected {'rejected' if should_fail else 'accepted'}, got "
                f"{'rejected' if problems else 'accepted'}"
            )
        else:
            print(f"  ok   {name}")

    def control(name: str, mutated: str, original: str) -> None:
        """A mutation that did not change anything would make the case below
        report 'rejected' for a workflow that is still the good one."""
        if mutated == original:
            failures.append(name)
            print(f"  FAIL {name}: the mutation was a no-op")
        else:
            print(f"  ok   {name}")

    expect("the checked-in workflow is accepted", check(workflow), False)

    # Wrong upstream workflow name. Removing the name entirely (rather than
    # replacing it with a different one) is the shape an Eggpack rename leaves
    # behind, and it is the case that silently subscribes to nothing.
    renamed = re.sub(
        rf"workflows:\s*\['{re.escape(EGGPACK_WORKFLOW)}'\]",
        "workflows: ['Eggpack candidate build']",
        workflow,
    )
    control("control: the wrong-upstream-name mutation applied", renamed, workflow)
    expect("a wrong upstream workflow name is rejected", check(renamed), True)

    # Push/pull/schedule triggers.
    for trigger in ("push", "pull_request", "schedule"):
        injected = workflow.replace(
            "  workflow_run:\n",
            f"  {trigger}:\n  workflow_run:\n",
            1,
        )
        control(f"control: the {trigger} trigger mutation applied", injected, workflow)
        expect(f"a {trigger} trigger is rejected", check(injected), True)

    # Scope and publication. `contents: write` is required now (GitHub serves
    # drafts only to push-level identities), so the cases that matter are the
    # ones it replaced: an undeclared scope, a forbidden extra capability, and
    # every shape this file could use to publish.
    undeclared = workflow.replace("permissions:\n  contents: write\n", "", 1)
    control("control: the undeclared-scope mutation applied", undeclared, workflow)
    expect("an undeclared permissions scope is rejected", check(undeclared), True)

    extra = workflow.replace("  contents: write\n", "  contents: write\n  id-token: write\n", 1)
    control("control: the extra-capability mutation applied", extra, workflow)
    expect("an id-token capability is rejected", check(extra), True)

    published = workflow.replace(
        "      # The draft is the authority being validated",
        "      - run: gh release publish \"${{ steps.identity.outputs.tag }}\"\n"
        "      # The draft is the authority being validated",
        1,
    )
    control("control: the publication mutation applied", published, workflow)
    expect("a publication command is rejected", check(published), True)

    # With write scope held, an API-level publish must be caught too: it needs
    # no `gh release` verb to work, and that is exactly the gap a verb-based
    # rule would leave.
    api_publish = workflow.replace(
        "      # The draft is the authority being validated",
        "      - run: gh api -X POST repos/dbowm91/cargo-cleanme/releases\n"
        "      # The draft is the authority being validated",
        1,
    )
    control("control: the API-publication mutation applied", api_publish, workflow)
    expect("an API-level publication is rejected", check(api_publish), True)

    registry = workflow.replace(
        "      # The draft is the authority being validated",
        "      - run: cargo publish --locked\n"
        "      # The draft is the authority being validated",
        1,
    )
    control("control: the registry-publication mutation applied", registry, workflow)
    expect("a registry publication is rejected", check(registry), True)

    # Default-branch checkout: the quietest failure in this file.
    ref_line = next(
        (line for line in workflow.splitlines() if line.strip().startswith("ref:")), ""
    )
    if not ref_line:
        print(
            "  FAIL precondition: the checked-in workflow has no `ref:` line to mutate",
            file=sys.stderr,
        )
        return 1
    # Still an expression, still a pinned checkout -- just the wrong SHA source.
    # A checker that only looked for "no ref at all" would accept this, which is
    # precisely the quiet failure the head_sha guard exists to prevent: the
    # validator runs, passes, and describes bytes that were never staged.
    default_branch = workflow.replace(ref_line, "          ref: ${{ github.sha }}", 1)
    control("control: the default-branch checkout mutation applied", default_branch, workflow)
    problems = check(default_branch)
    expect("a default-branch checkout is rejected", problems, True)
    if not any("head_sha" in p for p in problems):
        failures.append("the default-branch checkout is reported as not binding to head_sha")
        print("  FAIL the default-branch checkout is reported as not binding to head_sha")
    else:
        print("  ok   the default-branch checkout is reported as not binding to head_sha")

    # Missing upstream event/conclusion guards.
    no_event = workflow.replace("UPSTREAM_EVENT", "UNUSED_EVENT")
    control("control: the event-guard mutation applied", no_event, workflow)
    expect("a missing upstream event guard is rejected", check(no_event), True)

    # A guard that is mentioned in prose but never compared is not a guard.
    never_compared = workflow.replace(
        'if [[ "$UPSTREAM_CONCLUSION" != "success" ]]; then',
        'if [[ "always" ]]; then',
    )
    control("control: the never-compared-guard mutation applied", never_compared, workflow)
    expect("an upstream conclusion that is never compared is rejected", check(never_compared), True)

    # Missing exact release-identity check.
    no_identity = workflow.replace(IDENTITY_CHECK, "true")
    control("control: the missing-identity-check mutation applied", no_identity, workflow)
    expect("a missing release-identity check is rejected", check(no_identity), True)

    # Missing staged validator invocation.
    inlined = workflow.replace(VALIDATOR, "cargo clean --package app-a")
    control("control: the inlined-validation mutation applied", inlined, workflow)
    expect("an inlined reimplementation of staged validation is rejected", check(inlined), True)

    # A validator failure must propagate rather than being swallowed.
    # Removing the option entirely. A mutation that replaced the line with text
    # still containing the word would pass the guard, which is the reason the
    # guard matches the shell form rather than the word.
    no_pipefail = workflow.replace("          set -o pipefail\n", "          true\n", 1)
    control("control: the swallowed-exit-status mutation applied", no_pipefail, workflow)
    expect("a validator exit status that is not re-raised is rejected", check(no_pipefail), True)

    # Executing downloaded code.
    exec_artifact = workflow.replace(
        "      # The draft is the authority being validated",
        "      - run: gh release download \"$TAG\" && bash install.sh\n"
        "      # The draft is the authority being validated",
        1,
    )
    control("control: the downloaded-code mutation applied", exec_artifact, workflow)
    expect("executing downloaded release code is rejected", check(exec_artifact), True)

    # Unpinned action.
    unpinned = workflow.replace(
        "actions/checkout@11d5960a326750d5838078e36cf38b85af677262",
        "actions/checkout@v4",
        1,
    )
    control("control: the unpinned-action mutation applied", unpinned, workflow)
    expect("an unpinned action is rejected", check(unpinned), True)

    # No explicit permissions block at all.
    no_permissions = re.sub(r"^permissions:\n(?:  .*\n)+", "", workflow, flags=re.MULTILINE)
    control("control: the missing-permissions mutation applied", no_permissions, workflow)
    expect("a workflow with no explicit permissions block is rejected", check(no_permissions), True)

    # An inline trigger list. The real guards (`types`, the upstream name) live in
    # a nested block, so this shape is not equivalent to the checked-in file and
    # the *guards* are what must fail -- not the trigger-key parser. Both are
    # asserted separately: the parser still sees `workflow_run`, and the missing
    # `types`/name requirements are still caught.
    inline_trigger = re.sub(
        r"^on:\n(?:  [^\n]*\n|(?<=\n)  )+",
        "on: [workflow_run]\n",
        workflow,
        count=1,
        flags=re.MULTILINE,
    )
    control("control: the inline-trigger mutation applied", inline_trigger, workflow)
    inline_problems = check(inline_trigger)
    if "workflow_run" not in on_block(inline_trigger):
        failures.append("an inline trigger list is still read as a trigger")
        print("  FAIL an inline trigger list is still read as a trigger")
    else:
        print("  ok   an inline trigger list is still read as a trigger")
    expect(
        "an inline trigger list cannot hide the missing upstream guards",
        [p for p in inline_problems if "completed" in p or "Eggpack candidate builds" in p],
        True,
    )

    if failures:
        print(
            f"check-staged-validation-contract: self test FAILED ({len(failures)} case(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "check-staged-validation-contract: self test passed; the validation workflow's "
        "premises are checked in the failing direction"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove each guard still fails")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not WORKFLOW.is_file():
        print(
            f"check-staged-validation-contract: FAILED: {WORKFLOW} is missing", file=sys.stderr
        )
        return 1
    problems = check(WORKFLOW.read_text(encoding="utf-8"))
    if problems:
        print("check-staged-validation-contract: FAILED", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        f"check-staged-validation-contract: the validation workflow is a read-only, "
        f"upstream-bound enforcement point for scripts/{VALIDATOR}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
