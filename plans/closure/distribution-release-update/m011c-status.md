# Distribution, Release, and Update M011C Status

Plan: `plans/implementation/distribution-release-update/011c-published-release-smoke-automation.md`

Disposition: **conditionally closed — implementation complete; operational closure requires a real `release: published` event**

Implementation commit: the `phase11-hardening` branch commit that extends
`.github/workflows/post-release-smoke.yml` and adds
`scripts/resolve-smoke-transition.py`.

Repository baseline at implementation: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Date: 2026-10-05

## Executive finding

`scripts/post-release-smoke.sh` rehearses the **real** self-update transaction
against two public releases, and it found C016 and C017 while every fixture
suite was green. That is the entire argument for keeping it: `cargo test` says
nothing about whether the shipped binary can update a real installation.

It was `workflow_dispatch`-only — a networked rehearsal that ran when an
operator remembered. This milestone made it automatic on real publication, which
is what removed the memory requirement, and added the two pieces of tested
machinery that automation needs to be honest.

## The two hazards automation introduced

Both are the same hazard in different clothes: **resolving a transition from
whatever happens to be available** instead of from a proven exact pair.

### 1. "latest" is not a version

The smoke must exercise the *released previous-version updater*. Asking the
updater to fetch "latest" and comparing afterwards would let a newer release
race the test into passing. So the transition is resolved explicitly by
`scripts/resolve-smoke-transition.py`:

- numeric version comparison, because `0.1.9 < 0.1.10` is a requirement here
  precisely because **string comparison gets it backwards** and would select a
  release that does not exist as the previous one;
- the greatest stable release strictly below the target, which must also ship the
  contracted asset for the matrix target;
- a predecessor missing that asset is a **failure**, never a skip: "do not
  silently skip a previous version because it is known-bad. A broken predecessor
  is precisely what the transition test should expose";
- candidates are sorted inside `select_predecessor` rather than trusting the
  caller's order — a bug the self test caught, since `enumerate_releases` sorts
  but a caller supplying a list directly would otherwise get whatever came
  first;
- drafts and pre-releases never enter the candidate set, and the target tag
  itself is never selected as its own predecessor.

Resolution is centralised in a `resolve-transition` job whose outputs the matrix
job consumes via `needs:`. Five independently-resolved pairs would be five
chances to test different versions while the summary claimed one.

### 2. GitHub publication is not crates.io publication

The process publishes the GitHub release **first** and runs `cargo publish`
**second**, so a `release: published` event can fire before the version the
updater resolves actually exists. Waiting unboundedly is not the alternative —
that converts infrastructure lag into a red build that is still red an hour
later.

`wait_for_crates_io` polls the same public version authority the updater relies
on, with a descriptive User-Agent, HTTPS-only, a finite body cap, per-request
timeouts, exponential backoff, and a finite total deadline.

**Exhausting the deadline is a failure, not a skip.** This is the milestone's
central semantic, and it is the reason the resolver is a tested module rather
than a shell loop: a closure record showing a skipped smoke and one showing a
passing smoke must never be confusable. A non-JSON response, a yanked version, a
wrong version number, and a non-stable channel are each distinct failures — none
of them satisfies the wait, and none of them becomes a pass.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| `release: types: [published]` plus manual dispatch retained | Workflow triggers; checker asserts exactly these two | Pass |
| Reject draft/prerelease/non-semver even if the payload changes | Step re-checks `github.event.release.draft` and `.prerelease`, and the tag pattern; checker requires the payload to be read | Pass |
| Tested resolver rather than shell heuristics in YAML | `resolve-smoke-transition.py`, `--self-test` run by the workflow before use | Pass |
| Exact `from`/`to` recorded | Emitted as workflow outputs and rendered into the job summary | Pass |
| No predecessor skipped because it is known-bad | `select_predecessor` raises instead | Pass |
| Bounded crates.io wait; timeout is a failed smoke | `wait_for_crates_io` deadline; `an authority that never advertises the target fails at the deadline` | Pass |
| Yanked/wrong/non-stable target does not satisfy the wait | Three dedicated self-test cases | Pass |
| Smoke matrix still tied to the five release targets | `check-post-release-smoke-contract.py` unchanged in that respect; passes | Pass |
| `fail-fast: false` retained | Asserted by the existing contract checker | Pass |
| No write scope, no publication, no mutation outside job temp dirs | `permissions: contents: read`; no publish pattern | Pass |
| Network failure is failure, never skip/pass | No `continue-on-error` on either job | Pass |
| Automated path exercises the *released* previous updater | `--from` is an exact release tag resolved from the release list, never a rebuilt binary | Pass |

## Verification run

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — passed (261 tests).
- `python3 scripts/resolve-smoke-transition.py --self-test` — **passed**, 28
  cases, including the crates.io wait driven by a fake clock and opener so the
  deadline behaviour is exercised without spending real time.
- `python3 scripts/resolve-smoke-transition.py --to v0.1.7` — **passed live**,
  resolving `from_version=v0.1.6`, `to_version=v0.1.7` against the real published
  release list.
- `python3 scripts/check-post-release-smoke-contract.py` — **passed**.
- `python3 scripts/check-post-release-smoke-contract.py --self-test` — **passed**,
  32 cases, 15 of them added for the automatic-path premises.
- Workflow YAML parses; triggers are exactly `release` + `workflow_dispatch`.
- `python3 scripts/check-doc-citations.py` — passed.

## Limitations and unresolved findings

1. **Not operationally closed.** Acceptance requires "a real future
   `release: published` event to launch the five-target workflow automatically
   and all five lanes to pass after crates.io exposes that same version". This
   branch publishes nothing. The closure must not be upgraded before that run
   exists — and when it does happen, the deadline behaviour will be observed for
   the first time under real crates.io lag.
2. **The crates.io deadline default is 900 s.** Chosen to absorb normal
   `cargo publish` lag. If a release's crate publication is routinely slower than
   that, the smoke will fail for a reason that is not a product defect. The
   failure is deliberately still a failure, and `--deadline-seconds` is the
   operator's lever; changing it is a judgement call, not a workaround.
3. **First-release state is not automated.** `select_predecessor` fails when no
   predecessor exists, and the plan asked for that to be "an explicitly handled
   first-release state". It is handled by *not automating* it: there is no
   release after v0.1.6 that lacks a predecessor, and inventing a
   skip-the-first-release path would create exactly the "smoke did not run" hole
   this milestone exists to close. Recorded as a deliberate omission, not an
   oversight.
4. **`enumerate_releases` reads one page of 100.** Sufficient while the project
   has seven releases. It is a bound, not an exhaustive crawl, and a repository
   past 100 releases would need paging — at which point "greatest stable below
   the target" would silently start resolving against the wrong window.

## Closure status and unblocking

**Conditionally closed.** Implementation, tested machinery, static guards, and
docs are complete. The outstanding item is the hosted five-target run after a
real publication.

**Downstream effect:** none newly unblocked. M011A is a post-publication
verification that may run in the same workflow but keeps a separately visible
verdict, as the plan requires; the two share no code path and no failure
disposition.
