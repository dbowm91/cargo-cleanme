# Distribution, Release, and Update M011B Status

Plan: `plans/implementation/distribution-release-update/011b-staged-release-validation-workflow-gate.md`

Disposition: **conditionally closed — implementation complete; operational closure requires a real future staged draft**

Implementation commit: the `phase11-hardening` branch commit that adds
`.github/workflows/validate-staged-release.yml` and
`scripts/check-staged-validation-contract.py`.

Repository baseline at implementation: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Date: 2026-10-05

## Executive finding

`scripts/validate-staged-release.py` is this repository's only end-to-end check
that **staged bytes equal qualified bytes** — exact inventory, served-byte
SHA-256, release-manifest agreement, contract agreement, measured Linux ABI
floors, and a real installer run. It has repeatedly caught real release-process
defects. `architecture/14-testing-and-verification.md` recorded it as
**"wired into nowhere"**: a documented manual command that ran only when an
operator remembered.

Hosting it is the easy half. The milestone's real content is proving it cannot
be made to run on the wrong input, because an unsound gate is worse than no
gate — it converts an unavailable check into a documented pass.

## What was built

| Artifact | Role |
|---|---|
| `.github/workflows/validate-staged-release.yml` | Least-privilege validation workflow; subscribes to `Eggpack candidate builds` completion |
| `scripts/check-staged-validation-contract.py` | Static premise checker with `--self-test`, wired into `ci.yml` and `release-check.sh` |
| `docs/RELEASING.md` | Publication gate now names all four prerequisites, in order |

The workflow is a **separate file** because `release-binaries.yml` is generated
by Eggpack and protected by a drift gate; hand-editing it would fail `eggpack
ci check`. Eggpack remains producer authority. No Eggpack source change was
required, as the plan anticipated.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| `workflow_run` on `Eggpack candidate builds`, type `completed` | Workflow triggers; checker asserts both | Pass |
| Manual `workflow_dispatch` fallback taking an exact tag | `tag` and `source_run_id` inputs; checker requires dispatch to remain | Pass |
| `contents: read` only, no write scope | `permissions:` block; checker rejects `contents: write` *and* a missing block | Pass |
| Bounded timeout, actions pinned by full SHA | `timeout-minutes`; four actions pinned | Pass |
| Requires upstream `workflow_dispatch` event | Step compares `$UPSTREAM_EVENT`; checker requires the comparison, not the mention | Pass |
| Requires upstream success conclusion | Step compares `$UPSTREAM_CONCLUSION`; checker rejects a guard that is never compared | Pass |
| Checkout bound to upstream `head_sha` | `ref: ${{ github.event.workflow_run.head_sha || ... }}`; checker requires the `head_sha` expression | Pass |
| Derives the exact `vX.Y.Z` tag from the trusted checkout | Step reads `Cargo.toml` and rejects a manual tag that disagrees | Pass |
| Proves tag/source via existing release-identity logic | `check-release-identity.py --tag "$TAG"` invoked; checker requires it | Pass |
| Runs the existing validator, not a reimplementation | `validate-staged-release.py --tag`; checker rejects inlining and requires `PIPESTATUS[0]` | Pass |
| Does not execute downloaded code | Checker rejects download-then-execute; validator and policy come from the trusted checkout | Pass |
| Failure leaves the draft unpublished | No publication command anywhere; checker enforces | Pass |
| Validation script stays locally reusable | Unchanged; `--tag` is its only addition to the hosted path | Pass |

## The quietest failure, and why it is guarded specifically

A checkout of the default branch would run the real validator, which really does
pass, against whatever `main` currently contains — and the run would attest to
nothing about the staged bytes. It is green, correct, and worthless.

This is why the self test includes a control for *every* mutation: a mutation
that silently became a no-op reports "rejected" for a workflow that is still the
good one, which is strictly worse than no guard. That is not hypothetical — see
"Unresolved findings" below.

`check-staged-validation-contract.py --self-test` proves **35** cases, each in
the failing direction, with a control asserting the mutation actually applied.
Premise negatives include: wrong upstream workflow name, `push` /
`pull_request` / `schedule` / `workflow_run` / `pull_request_target` triggers,
`contents: write`, missing `permissions:`, a publication command, a
default-branch checkout, a missing event guard, an upstream conclusion that is
never compared, a missing identity check, an inlined validator, a swallowed exit
status, downloaded code execution, and an unpinned action.

## Verification run

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (261 tests).
- `python3 scripts/check-staged-validation-contract.py` — **passed**.
- `python3 scripts/check-staged-validation-contract.py --self-test` — **passed**,
  35 cases.
- Workflow YAML parses and its triggers/jobs are as intended
  (`workflow_run` + `workflow_dispatch`; one `validate` job).
- `python3 scripts/check-doc-citations.py` — passed.

## Limitations and unresolved findings

1. **Not operationally closed.** The milestone's acceptance criteria require "a
   real future Eggpack draft automatically triggers it" and "the existing
   validator passes against the actual staged bytes". This branch does not
   dispatch a release and must not. The hosted run is outstanding, and closure
   must not be upgraded before that evidence exists.
2. **The `ref:` expression carries a manual-dispatch fallback** to
   `github.sha`. On the `workflow_run` path — the only path that matters for
   automatic enforcement — it evaluates to the upstream head SHA. The fallback
   exists because `workflow_dispatch` has no upstream run to read one from, and
   the manual path re-proves identity with `check-release-identity.py` plus a
   version match against the checkout, so it is not a bypass. This is stated
   rather than hidden because it is the one place the binding is weaker.
3. **The `ref:` is on one line deliberately.** A folded `>-` scalar splitting
   the expression is valid to GitHub but unreadable to the contract checker, and
   a guard that cannot read its own subject is a guard that stopped guarding. The
   comment in the workflow says so.
4. **Checker iterations during implementation.** Three checkers were wrong before
   the workflow was: a checkout-block regex that captured nothing because the
   `uses:` line ends in a `# v4` comment; a `PIPESTATUS` pattern that required a
   braced form; and a "beyond the bound" assertion that tested an arbitrary
   detail rather than a property. Each was found by the checker failing against
   the good file, which is the direction that matters.

## Closure status and unblocking

**Conditionally closed.** Implementation, static guarantees, and documentation
are complete and verified. The outstanding item is the hosted end-to-end run
against a real staged draft.

**Downstream effect:** M011B blocks nothing that was ready. It is a prerequisite
for *publishing* the next release, in the sense that `docs/RELEASING.md` now
requires a green validation run before publication — but publication was already
a human action, so no plan is newly unblocked by it. M011A and M011C are
independent of this workflow and are closed on their own terms.
