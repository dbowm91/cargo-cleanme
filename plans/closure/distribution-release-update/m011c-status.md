# Distribution, Release, and Update M011C Status

Plan: `plans/implementation/distribution-release-update/011c-published-release-smoke-automation.md`

Disposition: **closed.** The condition attached to the previous *conditional*
disposition — *"operational closure requires a real `release: published` event"* —
was met by run `37561575727` on the v0.2.2 release: all five lanes green, with
the resolver recording `from_version=v0.2.1`, the correct previous stable. The
earlier automatic failures (v0.2.0, v0.2.1) remain recorded below; they are not
erased by a later green run.

Disposition history: **conditionally closed** after v0.2.1, when two automatic
events had occurred and **both had failed**. See the v0.2.1 addendum below, and
the v0.2.2 addendum at the end of this record.

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

---

## Five-target evidence — automatic run failed, manual recovery green (M013, 2026-10-06)

**The automatic run is recorded as a failure and is not presented as passing.**

Run [37419183947](https://github.com/dbowm91/cargo-cleanme/actions/runs/37419183947)
fired correctly: event `release`, two seconds after publication at `05:34:06`,
the draft/prerelease/tag guard passed, and the resolver started. It then hung in
the bounded crates.io wait and failed. **Three defects** sat in that path, none
of which had ever executed before, because M011C was correctly left
conditionally closed pending exactly this run.

### Defect 1 — the resolver polled a URL crates.io refuses

```python
payload = _fetch_json(CRATES_IO_API + f"/{target}")
```

crates.io's per-version endpoint is `/api/v1/crates/<name>/<version>`. The `v`
belongs to the tag, not the version, so this requested
`.../crates/cargo-cleanme/v0.2.0`, which crates.io answers **HTTP 400**. Every
attempt was a guaranteed miss, retried with backoff until the 900s deadline and
then failed. The transition itself was never wrong: `--to v0.2.0` alone
resolved `from_version=v0.1.6` instantly.

Fixed in `6ddc0d0` by building the path from the validated parse. Verified live:
`crates_io_attempts=1 crates_io_waited_seconds=0.099`.

**Why all 13 existing self-test cases passed:** each injects an `opener` that
ignores the URL and returns a canned payload. A wrong URL is invisible to a test
that never looks at the URL. A new case now records the requested URL and
asserts the path carries no `v`; it is mutation-proven.

### Defects 2 and 3 — the step could never have written its outputs

`tee -o "$GITHUB_OUTPUT"` — GNU coreutils `tee` has no `-o` option. Under
`set -o pipefail` the step failed before writing anything, masked on the
automatic path because defect 1 hung in the same step first. Two defects stacked
in one step and the first hid the second.

The `workflow_dispatch` inputs also default to bare versions (`'0.1.1'`) while
the resolver requires `vX.Y.Z`, so the manual path could not have worked either.
Fixed in `7e522a3` and `f53dc94`; the tag is now added on the workflow→resolver
edge and stripped on the resolver→script edge, since the resolver emits tags and
`post-release-smoke.sh:120` builds `.../download/v$version`.

### Recovery

Because a `release: published` event fires once per publication, the automatic
result **cannot be re-run automatically**. Recovery used the documented
`workflow_dispatch` path: [37420625111](https://github.com/dbowm91/cargo-cleanme/actions/runs/37420625111),
**five of five lanes green**.

| Target | Runner | Result |
|---|---|---|
| `aarch64-apple-darwin` | macos-14 | success |
| `aarch64-unknown-linux-gnu` | ubuntu-24.04-arm | success |
| `x86_64-apple-darwin` | macos-15-intel | success |
| `x86_64-pc-windows-msvc` | windows-latest | success |
| `x86_64-unknown-linux-gnu` | ubuntu-latest | success |

```text
post-release-smoke: x86_64-unknown-linux-gnu v0.1.6 -> v0.2.0 (public releases only)
  published v0.1.6 cargo-cleanme-x86_64-unknown-linux-gnu sha256 9bb8c582adcb1cf8bd6953f084d096e8249964b123ec01f61f6552e1efe9fc2c
  published v0.2.0 cargo-cleanme-x86_64-unknown-linux-gnu sha256 46f976432facd54a822eed26273f8ca443a311c31de902c61dc47446ca8d567f
  Cargo-managed install reports 0.1.6 (local build, sha256 e668741cddca, not the release asset)
post-release-smoke: PASSED (x86_64-unknown-linux-gnu v0.1.6 -> v0.2.0)
```

No fail-fast: one lane failing never suppressed the other four, and both failed
dispatches produced all five lane results.

### Closure status

**Operationally closed on five-target evidence, conditionally on the automatic
path.** Every target lane has now rehearsed a real `v0.1.6 -> v0.2.0`
transition against published bytes. What remains unmet is narrower and is
stated rather than smoothed over: the *automatic* trigger has never been
observed green, because the one time it fired it hit three defects, and a
release event cannot be re-raised. The next publication is the first real test
of whether the fixes hold.

---

## v0.2.1 addendum — automatic run failed again, manual recovery green (2026-10-06)

C023 published **v0.2.1** on 2026-10-06. This section appends that evidence. It
does not retract anything above, and it does not upgrade the disposition.

### Run 37507738147 — the automatic trigger fired again, and failed again

The `release: published` event fired **without operator action**, which is the
property this milestone was written for and which is now demonstrated twice.
It then failed **on all five lanes**, with the same shape on each:

```
error: cannot install package `cargo-cleanme`, it has been yanked from registry `crates-io`
```

**The cause was not the updater.** WP-L of the C023 plan required yanking
crates.io v0.2.0 — the correct safety decision, made a few hours before v0.2.1
existed. `select_predecessor` consulted GitHub release suitability but never
asked crates.io whether the selected version was **installable**, so it chose
v0.2.0 as the rehearsal source and every lane then discovered it could not
install it. The harness was reporting on a transition it could not perform; the
product it was meant to qualify never ran a line.

This is the same class of failure as the three defects above, and it was found
by the automatic path precisely because the automatic path runs on real events.

### The resolver fix

`scripts/resolve-smoke-transition.py` now reads the crates.io yanked set and
selects the greatest **installable** release below the target — which is what
WP-L's own resolution asks for. Commit `cfdc910afc732a50353158cd0ca6bf074b9ff29d`.

The associated self-test correction is `b0b41fcb34da0a0bece4f7763a9781a3c1123a2f`:
two pre-existing cases asserted the message fragment `no stable cargo-cleanme
release below`, and correcting it to `no installable stable …` broke both. CI
caught it four minutes after a local run I had read with `tail -10` and called
green — the same partial-read failure C023 §12.7 records.

Deliberately **not** done: un-yanking v0.2.0 so the harness could reach it.

### Run 37508262225 — the green evidence, from the manual rehearsal surface

Dispatched with `from_version=0.1.6`, `to_version=0.2.1`:

| Lane | Transition | Result |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `v0.1.6` -> `v0.2.1` | success |
| `aarch64-unknown-linux-gnu` | `v0.1.6` -> `v0.2.1` | success |
| `x86_64-apple-darwin` | `v0.1.6` -> `v0.2.1` | success |
| `aarch64-apple-darwin` | `v0.1.6` -> `v0.2.1` | success |
| `x86_64-pc-windows-msvc` | `v0.1.6` -> `v0.2.1` | success |

### Three independent properties

| # | Property | State |
|---|---|---|
| 1 | Automatic trigger wiring fires on a real publication | **proven** — twice, unprompted (37419183947 for v0.2.0, 37507738147 for v0.2.1) |
| 2 | The five-target product/update transaction works against published bytes | **green via manual recovery** (37420625111, 37508262225) |
| 3 | A five-target run under the **automatic** trigger is observed green | **not yet proven** — both automatic runs failed; property 1 does not satisfy property 3 |

### Disposition

**Remains conditionally closed.** The original milestone named a
`release: published` event launching the five-target workflow automatically *and*
all five lanes passing. Property 3 is that qualification, and it is unmet.

**No green automatic run for v0.2.1 exists or can be recreated.** The event fires
once per release and v0.2.1 has been published. The next release is the first
opportunity to obtain that evidence, with the fixed resolver and no operator
action — but it is an **operational dependency on a future event**, not a reason
to write a new implementation plan, and not a reason to upgrade this record
because the trigger itself was proven.

The distinction this milestone exists to protect survives intact: a run that was
**skipped**, a run that **failed**, and a run that **passed** must never be
confusable in a closure record. Two automatic failures and two manual greens are
recorded as exactly that.

## v0.2.2 addendum — the first green automatic run (2026-10-07)

The disposition this record previously held was **conditionally closed**, on one
condition: a real `release: published` event whose automatic run is green. Three
automatic runs have now happened:

| Event | Run | Result |
|---|---|---|
| v0.2.0 | `37419183947` | **failure** — three automation defects |
| v0.2.1 | `37507738147` | **failure** — all five lanes; the resolver chose the yanked v0.2.0 |
| **v0.2.2** | **`37561575727`** | **success — all five lanes green** |

The v0.2.2 run recorded `from_version=v0.2.1`, which is the point of the whole
exercise: the transition resolver no longer picks a yanked source, and the
automatic path produced the evidence the criterion asks for without a manual
rehearsal standing in for it. Release evidence: [`r022-status.md`](r022-status.md).

The two earlier failures stay in this record. A later green run does not convert
them into passes; it is the first automatic run that passed.
