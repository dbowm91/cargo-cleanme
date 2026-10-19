# C036 — CLI Surface Truthfulness and Documentation Drift Corrective

Status: **proposed; not active; no implementation authority**.
Date: 2026-10-19.
Repository baseline: `main` at 0.2.9 (unpublished; latest published tag `v0.2.8`).
Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.
Corrects: M012A (canonical maintenance CLI), M014 (CWD-anchored default),
M017 (scoped proof default), C028/C029 (learned-root admission and partial
Full coverage).
Related: `plans/adr/008-proposed-canonical-full-cleanup-spelling-and-proof-scoped-full-gate.md`.

## Objective

Make every user-visible statement about the CLI **true**, and make the
repository's operational documentation match the behaviour the code actually
has. Thirteen defects are in scope (D1–D13); all are documentation-,
message-, or evidence-level. **No production behaviour change is authorised by
this plan**, with one exception: D8 adds a bounded *diagnostic* to the
bare-invocation path, which changes no exit code and no cleanup behaviour
(see "Production changes" §3).

D2 was resolved by the architecture documentation pass of 2026-10-19, which
rewrote `architecture/02-cli.md` against the current source. It is retained in
the table for the record. Twelve defects remain open.

## Non-goals

- Changing the canonical Full cleanup spelling. That is a durable decision and
  belongs to ADR 008; this plan neither proposes nor pre-empts it.
- Changing the Full cleanup gate. Also ADR 008.
- Widening the Routine scope to include the working directory. ADR 003 §1 and
  ADR 006 §1 define the scope as seed + learned roots with the CWD as a
  *filter*; widening it is a direction change, not a corrective.
- Any change to Cargo resolution, ownership proof, activity gates, cleanup
  safety, exit-code semantics, or the JSON envelope.

## Current implementation evidence

All findings below were reproduced against a locally built `v0.2.9` binary on
a non-root Linux workstation, unless stated otherwise.

| # | Defect | Evidence |
|---|---|---|
| D1 | The root `long_about` states that "`cargo cleanme scan` with no ROOT is the full read-only machine reconciliation". Bare `scan` is a **usage error** (clap, exit 2) requiring one of ROOT / `--known` / `--all`. | `src/cli.rs:27-28`; ADR 006 §5; `src/cli.rs:83`; `cargo cleanme scan` → exit 2 |
| D2 | The CLI deep dive's own status block repeats D1's stale claim. | `architecture/02-cli.md:8-10` |
| D3 | A user-facing diagnostic tells the operator to run `scan --full` — a **hidden** alias that does not appear in `scan --help`. The canonical spelling is `scan --all`. | `src/discovery.rs:373-380` (message at `:378`); `src/cli.rs:98-100` |
| D4 | The README's recovery advice for "scan reports 0 groups" is incomplete: it says to backdate *the sources*, but the activity guard also keys on **directory mtimes**. | `README.md:140`; `src/traverse.rs:461-468` |
| D5 | The cleanup-safety skill states that any unresolved manifest blocks the whole scope. Since ADR 007 §3–§4 and M017, out-of-boundary unprovables are **reported, never blocking**; only in-boundary byte-capable unknowns block. | `.skills/cleanup-safety/SKILL.md:93-97`; ADR 007 §3-§5 |
| D6 | `docs/USAGE.md` advertises `cargo cleanme clean --full --dry-run` with no note that the operation refuses on a non-root machine, and separately documents that a Full scan is "incomplete" on a typical workstation without connecting the two. | `docs/USAGE.md:115`, `docs/USAGE.md:643-645` |
| D7 | No **live** `clean --full` run exists in the evidence base, and the repository knows it: the deep dive marks the fail-closed rule "partial", and the contract test that would cover it is a placeholder no case can satisfy. | `architecture/13-orchestration.md:1219-1220`; `tests/cli_contract.rs:3080-3121` |
| D8 | A bare invocation whose working directory lies **outside** the resolved Routine scope reports `0 CleanupUnit(s)`, `0 would-clean`, and exits 0 — silence where ADR 006 §4 requires a typed report. | live run in `/tmp/<scratch>`; ADR 006 §4 |
| D9 | `--include` / `--exclude` are matched against the **workspace root**, not the physical output root, and neither the flag help nor the config key says so. A pattern written as `**/target` matches nothing and produces no warning. | `src/cleanup.rs:1467`, `src/cleanup.rs:1477`; `config.toml` `cleanup.include` / `cleanup.exclude` |
| D10 | A configured `scan.root` is **silently ignored by a rootless `scan`**: `policy::resolve` returns from its `full` arm before the configured root is read. The behaviour is deliberate — configuration must not narrow the canonical reconciliation command — but the config key's own documentation does not mention it. | `src/policy.rs:19`; `src/main.rs:985-988`; `config.toml` `scan.root` |
| D11 | `plans/output-schema-v1.md` lists **19** `reason_code` values; the code has **20**. The missing one is `skipped_outside_selection` — the value M014 actually emits for a workspace outside the selected scope. | `src/cleanup.rs:263-286`; `src/cleanup.rs:1431`; `plans/output-schema-v1.md:48` |
| D12 | `plans/output-schema-v1.md` describes a non-UTF-8 path as using "the platform escaped debug form". `output::path` now uses `to_string_lossy()`, so the human and machine renderings **agree** and both render `U+FFFD`. The schema's own wording ("are UTF-8 strings when representable") is also wrong: a path is always a UTF-8 `String`, representable or not. | `src/output.rs:262-266`; `plans/output-schema-v1.md:51` |
| D13 | `plans/registry.md` contradicts **itself**: the M011C row says the automatic self-update smoke is "demonstrated three times" while the evidence table beneath it lists **seven** green automatic smokes. | `plans/registry.md:123`; `plans/registry.md:182-190` |

D2 is **already resolved** by the architecture documentation pass of 2026-10-19:
`architecture/02-cli.md` was rewritten against the current source and its status
block now states the explicit-selector rule. D2 is retained in the table so the
record shows that the defect was found, and was fixed by re-deriving the
document against the code rather than by editing the text in place. The
remaining D-series entries are open.

## Why previous verification missed it

This is the field that matters most; see `plans/003-planning-process.md` §6.

**D1 / D2 — the drift gate does not cover `long_about`.** The repository's
generated-artifact gate (`generate-docs -- --check`) renders the man page from
the command's `about`, and `man/cargo-cleanme.1` contains **no** instance of
the stale sentence (`grep -c "no ROOT" man/cargo-cleanme.1` → 0). The stale
sentence lives in `long_about`, which the man page does not render and the
gate therefore never sees. Its only surface is `cargo cleanme --help`, which
is ungated. The *behaviour* is correctly guarded — `scan` requires a selector
and that test is green — so the behavioural and textual contracts were allowed
to diverge with nothing to notice.

**D3 — no rule forbids advertising a hidden spelling.** The repository has a
strong, deliberate convention that hidden aliases exist for compatibility only
(`--dryrun`, `--yes`, `scan --full`), and one test even asserts the hidden
alias is absent from `scan --help`. But nothing asserts the converse
direction: that a *message* never names a hidden spelling. The message was
written when `scan --full` was still the published spelling (ADR 003 §5), and
the spelling was demoted to hidden by ADR 006 §5 without auditing the
messages that referenced it.

**D4 — the advice was written against file mtimes only.** The activity walk
stats **every** entry the traversal yields, directories included
(`src/traverse.rs:461-468`), and `dua_core::walk` yields directories as
entries. A directory's mtime changes whenever an entry is added or removed
inside it, so a build that writes into `src/` marks the workspace active
even if every file inside is old. The README sentence and the `scan`
doc comment both describe the rule in terms of *source* changing, and no test
exercises the "every file backdated, one directory mtime fresh" case.
Reproduced live: a scratch project with every file touched to two hours old
but with `src/` still at build time reported `active_skipped=1` in the scan
statistics and produced zero output groups; backdating the directories
immediately made the group appear.

**D5 — the skill is not gated against the ADRs it summarizes.**
`scripts/check-doc-citations.py` gates the mechanical half of `architecture/`
citations only (it globs `architecture/*.md`); the six skills in `.skills/`
are hand-maintained prose with no cross-check. ADR 007 was accepted
2026-10-16 and M017 implemented it, but the skill's "unresolved ownership
blocks the whole scope" section was not revisited. The live run proves the
code follows ADR 007 — 12 unresolvable `fuzz/Cargo.toml` manifests were
reported and did **not** block the worktrees cleanup — so the skill now
teaches the opposite of what the product does.

**D6 / D7 — the evidence gap was documented and left open.** The deep dive
records that no live `clean --full` run exists, and that the one-document
contract test "cannot run `clean --full` because that is a full filesystem
walk". The unit test that covers the gate
(`src/policy.rs:1062`, `full_cleanup_roots(1, Some(state()))`) asserts that a
**literal** `1` is refused; it never asserts that a real traversal on a real
machine produces a non-zero code. The gate was therefore green for a
premise — "code 1 is refused" — that held, while the operation it guards was
unreachable in practice. A live run on a non-root machine produced 127
permission-denied traversal errors under `/` (all in `/var`, `/snap`, `/run`,
`/etc`, `/root`, `/opt`, `/media`; **zero** under `/home`), the Full scan
exited 1, and `clean --full` refused with exit 2. The refusal is not an edge
case; it is the universal outcome for an ordinary user.

**D8 — the silent-no-op defect class was fixed for units, not for scopes.**
ADR 006 §4 required excluded *units* to be reported with a typed reason
("Silence would recreate the M012A silent-no-op defect"), and that was
implemented. The parallel case — the *scope* resolved to roots that do not
cover the working directory at all — was never given the same treatment. A
bare invocation in a directory outside the maintenance scope produces no units
to report, so the reporting mechanism has nothing to fire on, and the run
looks like a successful no-op.

**D9 / D10 — the configuration surface documents the *key*, not the *match*.**
`include` and `exclude` are glob sets, and a glob is only meaningful relative
to something. The code matches them against the **workspace root**; the key
name (`cleanup.include`) and the flag name (`--include`) both suggest the
output directory. Nothing states the subject of the match, so the most natural
pattern an operator would write — the one naming the output directory — is
also the one that never matches. The same gap produced D10: `scan.root` is
read by `policy::resolve` only on the paths that consult configuration, and
the "full" arm returns before that read. The behaviour is *correct* and
deliberate; the documentation does not carry the condition, which is the same
defect class as D1.

**D11 / D12 — the schema document is a snapshot, and nothing re-checks it.**
`plans/output-schema-v1.md` was corrected once (the "ten divergences" fixed
by the schema-document pass) and was then left to drift again. Two changes
landed since: M014 added the `skipped_outside_selection` reason code, and the
non-UTF-8 path rendering changed from a platform-escaped debug form to
`to_string_lossy()`. Nothing compares the document against the code, so each
change silently invalidated a line of it. The document's own wording is
also internally inconsistent: "are UTF-8 strings when representable" describes
a conditional that the type does not have.

**D13 — a control surface that contradicts itself is worse than a stale one.**
The M011C row and the table beneath it are written by different passes and
were never reconciled. A reader who trusts the row under-counts the evidence;
a reader who trusts the table cannot tell which is authoritative. The
registry's purpose is to be the compact, trustworthy answer to "what is open" —
a self-contradicting answer defeats that purpose.

## Invariants

- Nothing in this plan may change what is cleanable, how ownership is proven,
  how activity is detected, or any exit code.
- A hidden alias may be named in a *code comment*; it may not be named in a
  user-facing message or in operator documentation.
- Every user-facing sentence about scope selection must state the condition
  under which it is true.
- The verification ladder in `AGENTS.md` §3 must pass in order, and the
  results actually obtained must be recorded — a passing claim that was not
  run is worse than no claim.

## Production changes

1. `src/cli.rs` — rewrite the root `long_about` (D1) to describe the canonical
   surface as it is: bare invocation is Routine Execute anchored at the
   working directory; `scan --all` is the Full read-only reconciliation and
   requires an explicit selector; `clean ROOT` is the bounded explicit
   scope. Do **not** introduce the ADR 008 spelling here — this plan does not
   authorise it.
2. `src/discovery.rs:373-380` — change the diagnostic text from
   `run \`scan --full\`` to `run \`scan --all\`` (D3). One string.
3. `src/main.rs` — in the bare-invocation path, when the resolved Routine
   scope contains no root covering the current working directory, emit one
   bounded diagnostic naming the working directory and the resolved roots
   (D8). It must be a **diagnostic**, not a block: the run's exit code and
   cleanup behaviour are unchanged. The report must stay within the existing
   bounded-diagnostic contract, and `--format log` must remain one bounded
   line.

## Work packages

Ordered; each is independently verifiable.

- **WP-1 (D1, D2, D3).** Rewrite the root `long_about`; fix the
  `discovery.rs` diagnostic; update `architecture/02-cli.md` §status block and
  §2.2 to state the explicit-selector rule as the current behaviour. Re-derive
  every citation in `architecture/02-cli.md` from the new source — do **not**
  shift line numbers arithmetically (`AGENTS.md` §5). Run
  `scripts/check-doc-citations.py --self-test && scripts/check-doc-citations.py`.
- **WP-2 (D4).** Amend `README.md:136-140` and the `scan` doc comment to say
  that the activity guard considers **file and directory** mtimes, and that
  backdating files alone is not sufficient when a directory's mtime is fresh.
  Add a unit test in `src/traverse.rs` that constructs the "all files old,
  one directory mtime fresh" case and asserts `SourceActivity::Recent`.
- **WP-3 (D5).** Rewrite `.skills/cleanup-safety/SKILL.md` §"Unresolved
  ownership blocks the whole scope" to state ADR 007's actual rule: the proof
  boundary is the selected scope; out-of-boundary unprovables are reported in
  diagnostics and never block; in-boundary byte-capable unknowns block the
  boundary; refused-manifest passengers and scope notes never block; a scope
  that proves zero units against any unproven participant fails closed. Cite
  ADR 007 §3–§5 and M017 by name.
- **WP-4 (D6).** Amend `docs/USAGE.md` to state, next to the `clean --full`
  advertisement, that the operation requires a *complete* Full reconciliation
  and therefore refuses when the platform walk is incomplete — with the
  bounded alternative (`clean ROOT`) named. Cross-reference the existing
  "incomplete" passage at `docs/USAGE.md:643-645` rather than duplicating it.
- **WP-5 (D7).** Produce the missing evidence: a live `clean --full` run on a
  non-root machine, recorded with its exit code and the traversal diagnostics.
  Close the placeholder at `tests/cli_contract.rs:3080-3121` — either by
  making the one-document case runnable against a bounded root that exercises
  the same code path, or by replacing the placeholder with an explicit
  assertion that the path is unreachable and naming the manual rehearsal that
  covers it. Update `architecture/13-orchestration.md` §10 to reflect the
  outcome; do not leave a known-incomplete item described as merely
  "partial".
- **WP-6 (D8).** Implement the WP-3 diagnostic. Add a test that a bare
  invocation whose working directory is outside the resolved scope emits the
  diagnostic, and that it does not change the exit code.
- **WP-7 (D9, D10).** Amend the `--include` / `--exclude` flag help and the
  `cleanup.include` / `cleanup.exclude` comments in the root `config.toml` to
  state that the match is against the **workspace root**, with one worked
  example. Amend the `scan.root` comment to state that a rootless `scan`
  ignores it, and why (configuration must not narrow the canonical
  reconciliation command). The config template is embedded with `include_str!`
  and is part of the published crate, so the change is visible to every user;
  it changes no behaviour.
- **WP-8 (D11, D12).** Correct `plans/output-schema-v1.md`: add
  `skipped_outside_selection` to the `reason_code` enumeration; correct the
  non-UTF-8 path description to state that both renderings agree and render
  `U+FFFD`; and replace "are UTF-8 strings when representable" with the
  unconditional truth. Record in the document that `output::path` is lossy and
  therefore **not injective**, so a JSON path string must not be used as a join
  key. The code is authoritative; the document is corrected to match it.
- **WP-9 (D13).** Reconcile the `plans/registry.md` M011C row with its own
  evidence table, so the registry states one number and the table backs it.
  Do not add a third statement of the count anywhere else.

## Compatibility and configuration effects

- No configuration key changes.
- No JSON envelope change; the new diagnostic is a human/diagnostic-channel
  message, and `--format log` keeps its single bounded line.
- No exit-code change.
- The D1 long_about rewrite changes `cargo cleanme --help` output only. The
  man page is rendered from `about`, not `long_about`, so
  `man/cargo-cleanme.1` is unaffected — but `generate-docs -- --check` must
  still be green, because the CLI surface changed and the drift gate is
  authoritative about that.

## Required tests

- `src/traverse.rs`: a source-activity test for the fresh-directory-mtime case.
- `src/main.rs` or `src/cli.rs`: a bare-invocation test for the
  out-of-scope-working-directory diagnostic.
- `src/cli.rs`: a test asserting the root `long_about` contains no hidden
  alias spelling (`--dryrun`, `--yes`, `scan --full`, `clean --full`).
  This is the regression guard that D1 and D3 both lacked.
- `tests/cli_contract.rs`: a test asserting the `scan` no-selector case is
  still a usage error (exit 2) — existing coverage, kept as the behavioural
  anchor for the D1 text.
- A **mutation check** for the new long_about test: reintroduce the stale
  sentence and confirm the test fails.

## Verification commands

Run in order; stop at the first failure.

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

## Documentation updates

`README.md`, `docs/USAGE.md`, `architecture/02-cli.md`,
`architecture/13-orchestration.md`, `.skills/cleanup-safety/SKILL.md`,
`plans/output-schema-v1.md`, `plans/registry.md`, the root `config.toml`,
`CHANGELOG.md` (Unreleased). `architecture/overview.md` §3 counts are the
source of truth for module line counts and the inline-test total — if WP-1 or
WP-2 changes either, update the overview first and then align the deep dives.

## Acceptance criteria

1. `cargo cleanme --help` contains no statement that is false of the
   current behaviour, and no hidden alias spelling.
2. `cargo cleanme scan` (no selector) is still a usage error, and the help
   text does not imply otherwise.
3. No user-facing message or operator document names a hidden alias.
4. The activity-guard advice is accurate about directory mtimes, and a test
   pins the case.
5. The cleanup-safety skill states ADR 007's actual rule, with citations.
6. A live `clean --full` run is recorded with its exit code and diagnostics,
   and the deep dive no longer describes the evidence as merely "partial".
7. A bare invocation outside the resolved scope emits a bounded diagnostic
   and does not change its exit code.
8. The full verification ladder above was run, in order, and the actual
   results are recorded in the closure record.

## Stop conditions

Stop and escalate rather than expand scope if:

- any change appears to require altering a cleanup safety gate, an activity
  gate, an exit code, or the JSON envelope;
- the live `clean --full` rehearsal produces a result inconsistent with the
  evidence in ADR 008 — that would mean the diagnosis is wrong and ADR 008
  must be revisited before this plan continues;
- the `long_about` rewrite cannot be made accurate without introducing the
  ADR 008 spelling (it must be possible to describe the current surface
  truthfully);
- the generated-docs drift gate cannot be satisfied without a change to the
  CLI surface beyond D1's text.

## Closure evidence requirements

Per `plans/003-planning-process.md` §5, a closure record at
`plans/closure/artifact-discovery-cleanup/c036-status.md` must contain: the
implementation commit; a requirement-to-evidence matrix mapping D1–D8 to
evidence; the verification commands **actually run** with their results; the
live `clean --full` rehearsal record (WP-5); known limitations; unresolved
findings classified by severity; and a disposition. Compilation is not
closure.

Per `AGENTS.md` §6, this plan stays **open** until a published release
carries it.
