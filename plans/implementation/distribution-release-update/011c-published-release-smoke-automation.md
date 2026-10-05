# M011C — Published Release Smoke Automation

Status: ready

Repository baseline: `c3feae01fe5b684826b77b4d32afcb2e3bd4e3d0`

Source roadmap: Phase 11 — hardening, release trust, and qualification automation

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: operational hardening / live external evidence

Dependencies:

- no implementation dependency on M011A/M011B;
- operational closure requires a future public release event;
- the automated workflow must preserve C016/C017 five-target semantics.

## 1. Objective

Turn the real published-release updater rehearsal from a manually dispatched check into an automatic post-publication evidence workflow, while retaining manual dispatch for recovery and explicit historical transitions.

This is the only test surface that has caught multiple real updater defects. Its execution should be a release invariant, not operator memory.

## 2. Current evidence

At the baseline:

- `.github/workflows/post-release-smoke.yml` is `workflow_dispatch` only;
- it covers all five contracted release targets and has `contents: read` only;
- `scripts/check-post-release-smoke-contract.py` proves the smoke matrix matches the generated release runner/target matrix and currently enforces manual-only behavior;
- `scripts/post-release-smoke.sh` downloads and verifies two public releases, performs a real self-managed update, verifies the final digest, reruns dry-run, and proves Cargo-managed refusal with unchanged bytes;
- this path found C016 and C017 when fixture suites were green.

The publication order is GitHub release first, crates.io second. An automatic workflow triggered by GitHub publication therefore must wait boundedly for crates.io to advertise the target version before asking the updater to transition.

## 3. Invariants

- Keep `workflow_dispatch` as an explicit fallback/rehearsal surface.
- Add automatic execution only for a real published stable GitHub release.
- No write permissions, publication credentials, or mutation outside per-job temporary directories.
- Network failure is failure, never skip/pass.
- The target release must be a plain stable `vX.Y.Z` tag whose source identity matches the repository release contract.
- The transition source must be deterministically selected and recorded; never use a floating "latest" binary without resolving the exact version first.
- The smoke matrix remains mechanically tied to the five release targets.
- The automated path must exercise the released previous-version updater, not a binary rebuilt from source.
- Crates.io availability is a bounded prerequisite, not an infinite wait.
- A failed automatic smoke does not rewrite the already-public release; it blocks release closure and triggers corrective handling.

## 4. Work package A — Add the release-published trigger

Extend `post-release-smoke.yml` with:

~~~text
release:
  types: [published]
~~~

while retaining manual `workflow_dispatch`.

Automatic mode must derive the target version from `github.event.release.tag_name`, validate it as a plain stable tag, and bind the release tag to the expected source.

Reject draft/prerelease/non-semver events even if the event payload is malformed or future GitHub behavior changes.

## 5. Work package B — Resolve an exact previous release

Add a small tested resolver rather than embedding shell heuristics in YAML.

For automatic mode it must:

- enumerate public stable cargo-cleanme releases;
- select the greatest stable version strictly less than the target;
- require both source and target releases have the contracted asset for the matrix target;
- record the exact `from` and `to`;
- fail if no valid predecessor exists, except for an explicitly handled first-release state.

Manual mode continues to use explicit `from_version` / `to_version`.

Do not silently skip a previous version because it is known-bad. A broken predecessor is precisely what the transition test should expose.

## 6. Work package C — Wait boundedly for the crates.io authority

Because the GitHub `published` event can occur before `cargo publish` completes, automatic mode must poll the same public version authority the updater relies on until the target stable version appears.

Requirements:

- descriptive User-Agent;
- HTTPS only;
- finite request size/timeouts;
- finite total deadline/backoff;
- target version must equal the release tag exactly;
- yanked/prerelease state must not satisfy the wait;
- timeout is a failed smoke, not a skip.

Do not sleep unboundedly and do not infer success from GitHub alone.

## 7. Work package D — Update the workflow contract guard

Change `scripts/check-post-release-smoke-contract.py` from "manual only" to an exact allowed trigger contract:

- `workflow_dispatch`;
- `release: types: [published]`;
- no `push`, `pull_request`, or `schedule`;
- `contents: read` only;
- no release create/edit/upload or `cargo publish`;
- calls the tested transition resolver/wait helper;
- calls `post-release-smoke.sh`;
- fail-fast remains false so every target produces evidence.

Extend `--self-test` with premise negatives for missing release filter, accidental write scope, unbounded/no crates.io wait, and a workflow that computes but never executes the rehearsal.

Wire the guard through existing CI/release-check.

## 8. Work package E — Release closure integration

Update the release process:

- publication is still explicit/manual;
- once GitHub release + crates.io version exist, the automatic five-target smoke must complete;
- release closure records the workflow run id and all five target results;
- M011A attestation verification may run in the same post-publication workflow if already implemented, but its failure must remain separately visible.

If the smoke finds a defect in immutable published bytes, follow the existing patch-release corrective process. Never replace the published asset.

## 9. Failure semantics

- GitHub release published but crates.io never reaches target before deadline: fail.
- One platform network/runner failure: that job fails; `fail-fast: false` allows the other four to finish.
- Previous release missing a target asset: fail.
- Previous updater refuses/fails: fail and record actual reason.
- Cargo-managed scenario mutates bytes: critical failure.
- Self-managed result digest differs from target sidecar: critical failure.
- Manual re-run may prove transient infrastructure recovery but does not erase the failed original run; closure record includes both.

## 10. Required tests

- stable-version predecessor ordering including `0.1.9 < 0.1.10`;
- prerelease/draft exclusion;
- no-predecessor behavior;
- crates.io authority eventually reaches target;
- crates.io timeout/outage;
- yanked/wrong target does not satisfy wait;
- workflow guard accepts exactly the two intended triggers;
- all existing smoke-script premise assertions remain intact.

## 11. Verification

~~~text
python3 scripts/check-post-release-smoke-contract.py --self-test
python3 scripts/check-post-release-smoke-contract.py
bash scripts/release-check.sh
~~~

Operational closure requires a real future `release.published` event to launch the five-target workflow automatically and all five lanes to pass after crates.io exposes that same version.

## 12. Compatibility

No CLI/config behavior changes.

No recurring schedule is added: the purpose is release-event continuation, not continuous monitoring.

No new secret is required.

## 13. Acceptance criteria

M011C closes only when:

- a public stable release automatically triggers the smoke workflow;
- the exact previous stable public release is selected deterministically;
- the workflow waits boundedly for crates.io to expose the target;
- all five targets execute against real public bytes;
- self-managed replacement digest equals the target public asset;
- Cargo-managed install refuses with bytes unchanged;
- manual dispatch remains available;
- the workflow is statically guarded against write scope/trigger/matrix drift;
- release closure requires and records the automatic run.

## 14. Stop conditions

Stop if:

- deriving the predecessor requires ambiguous release policy not already defined;
- automatic GitHub release publication occurs before a safely bounded crates.io wait can be implemented;
- a target lacks a hosted runner equivalent to the release qualification runner;
- automation would require publication credentials.

Do not turn a post-release verifier into a publisher.

## 15. Closure evidence

Record:

- implementation commit;
- resolver/wait tests;
- updated contract-guard self-tests;
- triggering public release/tag;
- exact predecessor;
- crates.io authority timing/result;
- automatic workflow run id;
- five target outcomes;
- post-update digests and Cargo-managed refusal evidence;
- unresolved findings;
- disposition.
