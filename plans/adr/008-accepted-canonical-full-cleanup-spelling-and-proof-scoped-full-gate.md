# ADR 008 (accepted) — Canonical Full Cleanup Spelling and a Proof-Scoped Full Gate

Status: **accepted 2026-10-21** — owner-directed. §4 resolved to **4b**
(coherent); §5 resolved to **5b, route 5b-i** (proof-scoped Full gate).
Implementation authority is granted to **M018** (the gate) and **M019** (the
spelling); this record authorises **no release**, and neither milestone is
implemented.
Date: 2026-10-19 (proposed); 2026-10-21 (accepted).
Baseline: `main` at 0.2.9 (unpublished; latest published tag `v0.2.8`) — the
tree this proposal was written against. The acceptance was decided against a
later tree: `main` at `8b96f86` (0.2.10 published), after ADR 009 / C040.
Related: ADR 003 §§4–5 (canonical invocation — §4 amended by this decision),
ADR 006 §5 (explicit `scan` selector), ADR 007 §1 and *Residuals*
(whole-system fail-closed for `clean --full` — amended by this decision),
ADR 009 (the working directory joins the maintenance scope),
M012A, M014, M017, C028, C029, C032, C036 D7.
Scope: the **public CLI spelling** of the Full cleanup operation, and the
**gate** that decides whether a Full reconciliation is allowed to authorize
cleanup. Not Cargo resolution, not ownership proof, not learned-state schema.

## Problem

Two defects, one visible and one latent.

**1. The Full cleanup operation is unreachable for ordinary users.**
`cargo cleanme clean --full` refuses, on every non-root Linux workstation,
before it cleans anything:

~~~text
cargo-cleanme: configuration error: full reconciliation reported an incomplete
scan (exit 1); no cleanup commands were run against its unproven learned roots
~~~

Exit code 2. The refusal is unconditional in practice. The chain, each link
verified against the built binary on a live machine:

1. `clean --full` runs a Full reconciliation first (`src/main.rs:765`,
   `run_scan(ScanIntent::Full, …)`).
2. A Full reconciliation walks the **platform root** — `/` on unix. On this
   machine that walk produced **127 permission-denied traversal errors**,
   aggregated into one diagnostic whose `path` is `null` and whose message ends
   *"skipped region location unknown"*.
3. Any traversal error sets `coverage_complete = false`
   (`src/discovery.rs:638`), and `main.rs` derives `full_traversal_complete`
   from it, so the Full scan is **incomplete**.
4. An incomplete Full scan exits **1** (`full_incomplete`).
5. `policy::full_cleanup_roots` refuses any non-zero reconciliation code
   (`src/policy.rs:523-528`), so `clean --full` exits **2**.

The 127 unreadable directories are `/var` (63), `/snap` (42), `/run` (26),
`/etc` (6), `/root` (1), `/opt` (1), `/media` (1) and a tail deeper than the
measurement depth — **system-owned trees that contain no Rust development**.
`/home` has **zero** unreadable directories: the operator's own tree is fully
readable and fully covered. A bounded `scan /home/<user>` on the same machine
returns exit **0** (complete), while the platform-wide `scan --all` returns
exit **1**.

So the operation refuses because regions that are *irrelevant to the cleanup*
were unreadable. The learned roots it would have cleaned were, in the
ordinary case, perfectly proven.

**2. The canonical spelling is longer than the operation.**
The user-facing spelling for "reconcile the machine, then clean what that
reconciliation retained" is `cargo cleanme clean --full` — two words where one
suffices. `cargo cleanme --full` is a **usage error** (clap, exit 2). The
extra `clean` token is the cost of a subcommand that exists only because the
policy flags (`--older-than`, `--include`, `--exclude`, `--profile`,
`--package`) live on `clean` and not on the root.

## Why this is not simply a bug

ADR 007 is explicit and deliberate. §1:

> `clean --full` proves the full system and retains whole-system fail-closed
> blocking — the operation whose job is system-wide reconciliation keeps the
> old rule.

and *Residuals*:

> `clean --full` still blocks whole-scope on any unprovable participant: that
> operation asserts system-wide proof and keeps the old rule deliberately.

The unpublished 0.2.9 changelog repeats it: *"the full system for `clean
--full` (which keeps whole-system fail-closed blocking)"*.

So the refusal is **not an accident**. It is a safety rule meeting an
environment invariant: on unix, a platform-root walk by a non-root user is
*never* complete. The two cannot both hold. Resolving it is a
safety-boundary decision and therefore requires owner acceptance, not an
implementation choice.

## Why previous verification missed it

The gate is unit-tested, and the test is green — but it tests the *function*,
never the *reachability* of the function from a real invocation.

`src/policy.rs:1150`:

~~~rust
// An incomplete reconciliation never authorizes stale roots.
assert!(full_cleanup_roots(1, Some(state())).is_err());
~~~

The `1` is a **literal**. The test asserts that a non-zero code is refused;
it never asserts that a real walk produces a non-zero code on a real machine.
Nothing in the suite reaches `full_cleanup_roots` with a code produced by
actual traversal.

This gap is already recorded in the repository, in the deep dive:
`architecture/13-orchestration.md:1288` marks the `clean --full` fail-closed
as **"partial — … no live `clean --full` run exists, because it walks the
whole machine"**, and `architecture/13-orchestration.md:432` states
**"no test runs `clean --full` at any format"**. The evidence gap was known
and documented. It was not closed, and the live run it warned about is the
one that shows the operation is *always* refused.

The same shape of failure appears in `tests/cli_contract.rs:3097-3105`: the
one-document-per-invocation invariant for `clean --full` is guarded by a
comment explaining that the test **cannot** run `clean --full` because it is a
full filesystem walk. The guard is a placeholder that no case can satisfy.

This is the repository's own recurring lesson (`AGENTS.md` §4): *a green test
proves only that its own premises hold.* The premise here was "code 1 is
refused", never "a real machine produces code 1".

## Proposed decision (requires acceptance)

### 1. Canonical spelling

`cargo cleanme --full` becomes the canonical spelling of the Full cleanup
operation. `cargo cleanme clean --full` is **retained as a hidden
compatibility alias**, following the pattern ADR 003 §4 already establishes
for `--yes` and `--dryrun` (*"they must not remain the documented canonical
spellings"*, retained "through the pre-1.0 line").

`clean --full` has been published since 0.2.4, so it may not be removed
outright; it is hidden, not deleted. The M016 precedent — *"removal is total —
no alias, no deprecation path"* — applies to flags that **never shipped**
(`--partitioned`, removed in unreleased 0.2.9), not to a published spelling.

### 2. Flag placement and the clap identity strategy

`--full` is a **root-level, non-global** flag, exactly like `--dry-run`.

It must not be `global = true`: the `scan` subcommand already owns a `full`
field (the hidden alias of `--all`, `src/cli.rs:105-106`) and `clean` owns one
too (`src/cli.rs:121-122`). A blanket global would collide with those
argument ids, the same collision `architecture/02-cli.md:92-96` records for
`--dry-run` versus `update --dry-run`.

Non-global root args and identically-named subcommand args coexist — that is
precisely how `dry_run` already exists at `src/cli.rs:56-57` and on `Clean`
and `Update` today.

`Cli::invocation()` then resolves `root.full || clean.full` to
`CleanupScope::Full` at the existing arm (`src/cli.rs:374-380`).

### 3. `--full` with a read-only or local-only subcommand is a usage error

`cargo cleanme --full scan --all`, `cargo cleanme --full config path`, and
`cargo cleanme --full update` are **usage errors (exit 2)**, not no-ops.

Rationale: `architecture/02-cli.md:97-104` records the worst failure mode this
tool has — a root-level flag that clap accepts and the code silently
discards, so a *dry run deleted the bytes and reported success*. `--full` is a
**scope selector**, not a mode; silently discarding it would leave the
operator believing a machine-wide cleanup ran when it did not. The existing
`Invocation::UsageError` arm (`src/cli.rs:276-284`) is the correct home for
it, and it is the arm whose doc comment already explains that "defaulting to
Full would silently run the machine-wide reconciliation the flag exists to
gate".

The `--dry-run` precedent differs and is not a counterexample: `scan` and
`config` *ignore* a global `--dry-run` because for those operations a
simulation **is** the same read-only behaviour (`src/cli.rs:50-57`). Full
cleanup is not the same behaviour as a Full scan.

### 4. The advanced policy flags — resolved: 4b (coherent)

The `clean` subcommand exists because the policy flags live there. The
question is whether `--full` alone moves, or the policy flags with it.

**Resolved 2026-10-21: 4b.** The policy flags become root-level, the root is
the single cleanup front door, and `clean` survives as a **hidden compatibility
alias** for published scripts. 4a is retained below for the record.

**4a. Minimal.** Only `--full` moves to the root. `--older-than`,
`--include`, `--exclude`, `--min-reclaimable-bytes`, `--profile`, and
`--package` stay on `clean`.

- `cargo cleanme --full` — works.
- `cargo cleanme --full --older-than 3600` — **usage error**.
- Advanced Full cleanup still requires `cargo cleanme clean --full --older-than 3600`.

The extra word is dropped for the common case only, and the published
spelling survives as the *only* way to combine Full cleanup with a policy
override. This is the status quo with a shorter alias, not a cleanup.

**4b. Coherent (recommended).** The policy flags become root-level as well, so
the root is the single cleanup front door and `clean` survives only as a
compatibility alias.

- `cargo cleanme --full --older-than 3600` — works.
- `cargo cleanme --older-than 3600` — works, equivalent to today's
  `cargo cleanme clean --older-than 3600`.
- `clean` remains accepted, hidden, for published scripts.

Cost: the root command's flag set grows to the union of today's root and
`clean` sets, and every policy flag needs an explicit decision for the
read-only subcommands (reject, or ignore as `--dry-run` does). Rejecting is
recommended: `--profile` and `--package` select Cargo targets and are
meaningless for a read-only scan, and a silently ignored selector is the
discarded-flag failure mode again.

### 5. The Full cleanup gate — resolved: 5b, route 5b-i

**Resolved 2026-10-21: 5b, implemented by route 5b-i** (a verification pass
over the learned roots). 5a is folded in as the message improvement that ships
*with* 5b rather than as a substitute for it; 5c is rejected and is retained
below for the record.

**5a. Keep the refusal; make it actionable (recommended as the immediate step).**
The gate stays exactly as it is — fail closed on any non-zero reconciliation
code. The refusal message changes to name the cause (the platform walk was
incomplete because of unreadable paths), state the consequence (no cleanup
ran), and point at the working alternatives (`cargo cleanme scan --all` for
the read-only inventory; `cargo cleanme clean ROOT` for a bounded scope that
is provable). Cost: near zero, no safety change, and the operation remains
unusable for non-root users.

**5b. Scope the gate to the roots actually proven (recommended as the target).**
The Full reconciliation reports, per root, whether that root's traversal
completed. `clean --full` then cleans the learned roots whose coverage
completed and **skips** (with a typed reason) those that did not.

This is ADR 007's own principle applied to the Full scope: §3 already
establishes that *out-of-boundary unprovables are reported, never blocking*,
and §1 already makes the **selected scope** the proof boundary. Extending
"boundary-scoped proof" from the CWD anchor and explicit roots to the Full
scope's learned roots is the same rule, not a new one.

It requires **new discovery machinery** that does not exist today, and the
precise obstacle is now identified:

- Per-walk-root attribution **already exists**.
  `src/discovery.rs:632` keeps `walk_errors: HashMap<usize, (u64, u64, u64)>`
  keyed by walk-root index, and `src/discovery.rs:772-783` drains it into
  **one diagnostic per walk root**, naming the root in the message. It is
  covered by `more_than_one_thousand_walk_errors_stay_one_bounded_root_record`
  (`src/discovery.rs:1008`).
- But a Full scan has exactly **one walk root** — the platform root `/` — so
  that machinery cannot separate `/var` from `/home` within it. That is why
  the observed diagnostic reads *"127 traversal error(s) under / … skipped
  region location unknown"*.
- The deeper obstacle is upstream: `dua-core` 4.1's
  `RootEvent::Entry(io::Result<Entry>)` (`dua-core-4.1.0/src/lib.rs:276-281`)
  carries a **bare `io::Error` with no path**. The repository already records
  this: *"M-11: dua-core's error event carries no child path, only the root
  index"* (`src/discovery.rs:775-778`). A traversal error therefore cannot be
  attributed to the subtree that produced it, at any layer above dua-core.

Two implementation routes follow, and the choice between them is a
maintenance-cost decision, not a safety one:

- **5b-i — a verification pass over the learned roots.** After the platform
  walk produces the fresh generation, re-walk each learned root as **its own
  walk root**. The existing per-root attribution then applies directly, and
  a root whose subtree is complete is provable while a root that is not is
  skippable. The learned-root set is small (15 on the reference machine) and
  the subtrees are far smaller than the platform, so the cost is bounded and
  measurable. This requires **no dependency change**.
- **5b-ii — an upstream `dua-core` change** that surfaces the failing path in
  the error event, making single-pass subtree attribution possible. Cleaner in
  principle, but it is a dependency change to a traversal library whose
  event shape the repository has already had to work around, and it puts the
  fix outside this repository's release boundary.

5b-i is recommended: it is implementable inside this repository, it reuses
machinery that already exists and is tested, and it keeps the fix on the same
release train as the spelling change it enables.

It also requires an **amendment to ADR 007 §1 and *Residuals***, which
currently reserve whole-system fail-closed for `clean --full`.

**5c. `--full --allow-partial`.** An explicit opt-in that cleans the proven
roots and reports the unproven ones, with a loud warning. Weakest safety of
the three and the most likely to be misread as "Full cleanup"; it is listed
here to be rejected on those grounds, not because it is unimplementable.

**Recommendation.** Ship 5a now, because it is honest and costs nothing;
pursue 5b as the real fix. 5a alone leaves the canonical `--full` spelling
pointing at an operation that refuses — which is why the spelling change
(§1–§4) and the gate change (§5) must be sequenced together, not separately.

**Ratified 2026-10-21.** The recommendation is adopted with one refinement: 5a
is **not** a separate release. It is the refusal-message half of 5b — under 5b
the operation no longer refuses wholesale, but a root it cannot prove is still
reported with a typed reason, and that message must name the cause and the
alternative. Shipping 5a alone would have left the canonical spelling pointing
at an operation that refuses, which is the failure the recommendation itself
warns about. The sequencing rule is therefore binding: **the spelling change
(§1–§4) and the gate change (§5) are sequenced together, never separately
released.**

## Consequences

- The canonical Full cleanup spelling becomes one word shorter, matching
  ADR 003's "bare invocation is the canonical maintenance operation" posture.
- No published spelling is removed: `clean --full` becomes hidden, and
  `scan --full` stays hidden.
- The root command's flag set grows (4b) or stays small (4a). Either way
  `cargo cleanme --help` becomes the place an operator looks for the Full
  cleanup operation, so the root `long_about` must be rewritten to describe
  it. The stale "`cargo cleanme scan` with no ROOT is the full read-only
  machine reconciliation" sentence that used to sit at `src/cli.rs:27-28` has
  already been retired by **C040 (ADR 009)**, which rewrote the same
  `long_about`; this decision adds the Full-cleanup description to it, and the
  root man page renders from that same `long_about`, so the two surfaces
  cannot drift.
- Under 5b, the Full operation becomes usable for non-root users and the
  cleanup boundary stops depending on the readability of `/var` and `/snap`.
- ADR 007 §1 and *Residuals* are amended (done at acceptance, effective when
  M018 ships), and the 0.2.9 changelog sentence about `clean --full` keeping
  whole-system fail-closed blocking is **superseded** — not edited, because
  0.2.9 was published on 2026-10-19 and its changelog entry is the historical
  record of what 0.2.9 did. The rule changes in the release that carries
  M018.
- Generated artifacts (`man/`, `completions/`) and the drift gate
  (`generate-docs -- --check`) must be regenerated.
- The architecture deep dives that cite the spelling
  (`architecture/02-cli.md`, `04`, `06`, `09`, `10`, `13`, `16`) and the
  `docs/USAGE.md` advertisement of `clean --full --dry-run`
  (`docs/USAGE.md:166`) require reconciliation, with citations re-derived
  from the new source rather than shifted arithmetically.

## Rejected alternatives

- **Make `--full` global.** Rejected: it collides with the `scan` and
  `clean` argument ids already named `full`, the same failure
  `architecture/02-cli.md:92-96` records for `--dry-run`.
- **Remove `clean --full` outright rather than hiding it.** Rejected: it has
  been published since 0.2.4. The M016 total-removal precedent applies to a
  flag that never shipped, not to a published spelling.
- **Let `clean --full` proceed whenever *any* fresh generation exists,
  ignoring the reconciliation code.** Rejected: it discards the distinction
  between a complete and an incomplete proof, which is the entire point of
  the gate, and it would resurrect stale learned roots — the C028 defect.
- **Keep the spelling as `clean --full` and only improve the message.**
  Rejected: it does not address the user's request, and it leaves the
  canonical spelling pointing at an operation that refuses for every
  ordinary user.
- **Make the platform walk skip unreadable system trees instead of
  recording them as coverage errors.** Rejected as a silent narrowing: an
  unreadable tree is genuinely unknown, and dropping it from the coverage
  record would make an incomplete scan look complete. If the platform prune
  list is to change, that is a separate decision about what `/var` and
  `/snap` mean for a Cargo-output inventory, and it needs its own evidence.

## Acceptance and evidence gate

Before this ADR is accepted:

1. A **live** `clean --full` run must exist on a non-root unix machine, and
   its result must be recorded. The absence of one is the finding, and
   "the test passes" is not a substitute (`architecture/13-orchestration.md:1288`).
2. The refusal path must be exercised with a **real** non-zero reconciliation
   code produced by a **real** traversal, not a literal — a fixture that
   provokes a permission-denied path under a bounded root.
3. Under 5b-i, a **verification pass** must show that a learned root whose
   subtree is entirely readable is provable while a sibling learned root
   containing an unreadable subtree is skipped with a typed reason, and that
   the pass attributes the error to the correct root.
4. The generated `man/` and `completions/` artifacts must be regenerated and
   the drift gate green.
5. `cargo cleanme --help` must describe the canonical Full cleanup spelling,
   and the stale `scan`-with-no-ROOT sentence must be gone.
6. The `plans/adr/003` §4 text, the unpublished 0.2.9 changelog sentence, and
   ADR 007 §1/*Residuals* must be amended or explicitly superseded — not
   edited to imply the original decision was never made.

If the owner declines 5b, ADR 007's whole-system rule stands unchanged, 5a
ships as a message improvement, and the canonical `--full` spelling must
**not** be introduced: the product would then advertise a one-word spelling
for an operation that refuses on every ordinary machine, which is the
C034-class "green test, broken product" failure this repository keeps
re-learning.

### Disposition of the six items — 2026-10-21

The owner accepted this ADR on 2026-10-21. The six items are dispositioned
below. Two of them are *outputs of the implementation* rather than
preconditions of the decision, so they cannot exist at acceptance; they are
carried as **acceptance criteria of the milestones named against them**, and
this ADR is not closed until they are met.

| # | Disposition | Owner of the remainder |
|---|---|---|
| 1 | **Discharged at acceptance** by the live run recorded below. | — |
| 2 | **Partially discharged.** The live run produced a real non-zero code (1) from a real traversal, not a literal. The *fixture* — a bounded root with a permission-denied path, exercised as a contract case — is still open. | M018 |
| 3 | **Carried.** Requires the 5b-i verification pass, which does not exist yet. | M018 |
| 4 | **Carried.** The spelling change is what moves the CLI surface, so the artifacts move with it. | M019 |
| 5 | **Carried.** The root `long_about` is rewritten by the spelling change; the stale sentence was already removed by C040 (ADR 009). | M019 |
| 6 | **Discharged at acceptance** by the amendments in the same pass — see "Amendments this decision requires" below. | — |

Item 1's live run, 2026-10-21, on a non-root Linux host (uid 1000), using the
published 0.2.10 binary resolved by absolute path:

~~~text
$ cargo-cleanme scan --all --format log
cargo-cleanme: discovery state was not reconciled: Full traversal was incomplete
cargo-cleanme op=scan status=failed scope=full manifests=2486 groups=17 bytes=97955815424 diagnostics=1232
exit 1     (38.0 s)

$ cargo-cleanme clean --full
cargo-cleanme: discovery state was not reconciled: Full traversal was incomplete
1232 filesystem diagnostics; rerun with a bounded root if needed
  … 1211 additional Cargo failure cause(s)
  warning: 128 traversal error(s) under /: 128 permission-denied, 0 vanished, 0 other; skipped region location unknown
cargo-cleanme: discovery state was not saved; cleaning the reconciled in-memory generation only
cargo-cleanme: configuration error: full reconciliation reported an incomplete scan (exit 1); no cleanup commands were run against its unproven learned roots
exit 2     (stdout: 0 bytes)
~~~

The refusal is **unconditional in practice**: the platform walk by a non-root
user is never complete, so `clean --full` refuses on every ordinary machine.
The count differs from the one in *Problem* above (128 traversal errors here,
127 there) because these are different machines; the shape is identical — one
aggregate diagnostic naming the platform root, no child path, and the
`skipped region location unknown` suffix.

Two facts from this run are recorded because they are load-bearing for the
implementation:

- **stdout was empty.** The refusal is a pre-report fatal error, so the
  one-document-per-invocation contract is not violated by it.
- **The exit code was 2, not 1.** The Full scan's incompleteness is exit 1;
  the *cleanup* refusal is a configuration error, which is exit 2.

## Acceptance record — 2026-10-21

**Decision.** The owner accepted ADR 008 on 2026-10-21, resolving the two
open forks at the options this document recommends: **§4 → 4b** (the policy
flags become root-level and `clean` survives as a hidden compatibility alias)
and **§5 → 5b, route 5b-i** (a verification pass over the learned roots).

**What is decided.**

1. `cargo cleanme --full` becomes the canonical spelling of the Full cleanup
   operation. `cargo cleanme clean --full` is retained as a **hidden
   compatibility alias** — published since 0.2.4, so it is hidden, not
   deleted.
2. `--full` is a **root-level, non-global** flag. It is not `global = true`:
   `scan` and `clean` already own a `full` argument id
   (`src/cli.rs:106`, `src/cli.rs:122`), and a blanket global would collide
   with them.
3. `--full` with a read-only or local-only subcommand (`scan`, `config`,
   `update`) is a **usage error, exit 2** — never a silently discarded flag.
4. Under 4b, the policy flags (`--min-reclaimable-bytes`, `--older-than`,
   `--include`, `--exclude`, `--profile`, `--package`) become root-level, and
   a policy flag supplied to a read-only subcommand is a **usage error, exit
   2** — rejected, not ignored as `--dry-run` is.
5. The Full cleanup's proof boundary becomes the **learned root**, not the
   platform root: a learned root whose traversal completed is proven and
   cleaned; a learned root whose traversal did not complete is **skipped with
   a typed reason**, and the operation proceeds with the roots it can prove.
6. 5c (`--full --allow-partial`) is **rejected**.

**Sequencing — binding.** The spelling change (§§1–4) and the gate change
(§5) are **sequenced together, never separately released**. The canonical
spelling must never point at an operation that refuses. Concretely: M019 (the
spelling) **may not be published before M018 (the gate) has shipped**.

**Amendments this decision requires.** ADR 003 §4 (the explicit cleanup
command) and ADR 007 §1 and *Residuals* (the whole-system fail-closed
reservation for `clean --full`) are amended in the same pass as this
acceptance, each with an explicit amendment block that names ADR 008 and its
date, so that the original decision remains visible. The 0.2.9 changelog
sentence about `clean --full` keeping whole-system fail-closed blocking is
**explicitly superseded, not edited**: 0.2.9 was published on 2026-10-19, so
its changelog entry is the historical record of what 0.2.9 did, and the rule
it describes changes in the release that carries M018.

**What this acceptance does not do.**

- It does not implement anything. M018 and M019 are unimplemented; no
  behaviour described here exists in the tree.
- It does not authorise a release. Publication remains the maintainer's
  decision under `docs/RELEASING.md`.
- It does not change any ownership proof, any Cargo resolution rule, the
  learned-state schema, or the JSON envelope's shape. M018 adds one
  **additive** reason code; per the `json-and-exit-contract` skill's rule,
  an additive field needs no envelope version bump.
- It does not resolve ADR 008 §4/§5's *implementation* questions — the
  per-root attribution mechanism, the cost bound, and the disclosure wording
  are M018's work, and the CLI surface details are M019's.

**The residual this decision accepts.** Under the whole-system rule, an
unreadable region *anywhere on the machine* — including system trees that
contain no Rust development — blocked the whole operation. Under 5b, an
unreadable region inside a learned root blocks *that root*, and an unreadable
region outside every learned root blocks nothing. An unknown participant hidden
in such an out-of-scope region cannot be disproven; that is the same residual
ADR 007's *Residuals* already disclose for out-of-boundary unprovables, and it
must be disclosed in the M018 closure record and in the user documentation
rather than left implicit.
