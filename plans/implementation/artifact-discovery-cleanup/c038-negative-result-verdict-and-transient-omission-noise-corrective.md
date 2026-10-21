# C038 — Negative-Result Verdict and Transient-Omission Noise Corrective

Status: **closed** — implementation landed and verified, locally and on the
hosted lanes (CI `39338559008`, all nine jobs green, plus the Release drift
guard `39338559022`), and the publication that was the last remaining
condition has happened: **v0.2.10** carries this corrective (tag `v0.2.10` at
`270cceb2be31`; GitHub release published 2026-10-21T00:35:53Z, immutable with
a valid attestation; crates.io sha256 `0f2e471b…`; automatic smoke
`39522277814` green on five targets). Evidence in
`plans/closure/artifact-discovery-cleanup/c038-status.md`; release record in
`plans/closure/distribution-release-update/r030-status.md`.

Repository baseline: `main` at `426cb72` (v0.2.9, published). The defect was
reported against the published v0.2.9 binary.

Source roadmap: `plans/subsystems/artifact-discovery-cleanup-roadmap.md`.

Related: `architecture/overview.md` §10 findings **19** and **37**; the
C033 corrective (`c033-learned-root-reconciliation-and-unavailable-noise-corrective.md`),
whose "make a normal Routine invocation quiet" objective this extends rather
than reopens; ADR 003 (canonical invocation and unattended output) and ADR 006
(working-directory-anchored selection) remain binding and unmodified.

## Objective

A run that cleaned nothing must say so in one plain line and must not look like
a failure. Today two separate things break that:

1. The interactive surface refuses an empty candidate list as a **configuration
   error** (exit `2`), while the same scope cleaned without `-i` exits `0`. Two
   spellings of one scope disagree.
2. The human report for a scope with nothing in it is a scope census and a
   summary of zeroes — four lines to carry one fact.

And one thing makes every Routine invocation noisy:

3. The learned-root omission summary is printed to stderr on **every** run, in
   the tool's error shape, naming paths the operator never asked about. On a
   real machine it fires essentially always, because a developer machine keeps
   agent worktrees, package caches and temp directories among its learned
   roots.

The owner's direction (2026-10-19): *for nothing done, simply print "Nothing to
clean."*

## Non-goals

- **Not** a change to the JSON contract. The envelope, its fields, and
  `schema_version` are untouched; only the human surface shortens.
- **Not** a change to the log contract. `--format log` keeps its one bounded
  line.
- **Not** a change to the ownership proof, the scope boundary, or anything that
  decides whether bytes may be deleted.
- **Not** a change to the interactive session's own lifecycle. The hidden
  cursor (finding 36) is a separate defect and stays open.
- **Not** a suppression of *actionable* diagnostics. A durable learned root
  that has vanished, and a root that is no longer a directory, are still
  reported. Only the by-design bucket is silenced.

## Current implementation evidence

Reported by the repository owner, running the published v0.2.9 binary with the
working directory set to `~/.codex/worktrees`:

```text
cargo-cleanme: omitting 15 non-maintainable learned root(s); examples: /home/sugarwookie/.codex/worktrees/1749, /home/sugarwookie/.codex/worktrees/2d08, /home/sugarwookie/.codex/worktrees/42e6
cargo-cleanme: configuration error: no cleanup units are available to select; nothing would be cleaned
```

Both lines are **factually correct**, and that is worth stating plainly
before changing anything:

- The learned-root state on that machine holds 17 roots. Fifteen of them sit at
  known-transient origins — eight `~/.codex/worktrees/<id>` entries (seven of
  them already deleted), `~/.local/share/Trash`, two `node_modules` trees, one
  Go module cache entry, and `/tmp`. The other two (`~/m013-fixture`,
  `~/projects`) are maintainable and were admitted. The count is right.
- The interactive surface had nothing to offer: the working-directory anchor
  excluded every resolved unit, so the candidate list was empty, and refusing
  an empty list was the coded behaviour.

What is wrong is the *shape*. The first line wears the tool's error prefix and
appears on every run for a decision that is not a problem. The second turns a
completed run with nothing to do into a typed configuration error with exit
`2`.

Verified live on the published binary, over a real pty, and against the same
machine's learned state.

## Invariants

- **A negative result is not a failure.** A run that completed and cleaned
  nothing exits `0` and says so in one line.
- **Two spellings of one scope agree.** `cargo cleanme -i` and
  `cargo cleanme` over the same scope reach the same exit status.
- **A by-design filter is not a diagnostic.** A root omitted because it sits at
  a known-transient origin is omitted *by design* — the tool would never have
  cleaned it — so omitting it narrows nothing the operator was relying on. It
  is not news, and it does not reach the operator.
- **An actionable omission is still reported.** A durable learned root that has
  vanished is news: there was a project there.
- **Silence about uncertainty is still forbidden.** ADR 007's rule stands: an
  unproven participant, an out-of-scope unprovable, a scope block, a failure,
  and a skipped unit are all still reported. "Nothing to clean." is only ever
  said when there is genuinely nothing to report.
- **The machine contract does not move.** The JSON envelope and the log line
  are byte-identical before and after.

## Production changes

1. **`src/interactive.rs`** — `SessionResult` gains `NothingToClean`.
   `run_interactive` settles an empty candidate list **before** the terminal
   precondition: there is no session to drive, so there is nothing a terminal
   could be needed for. This is also what makes the case testable without a
   pty.

2. **`src/main.rs`** — `SelectionOutcome` gains `NothingToClean`; the
   orchestrator's mapping is extracted into `interactive_outcome` so the three
   interactive endings are separately assertable. The verdict is one line on
   stdout (`Nothing to clean.`) and exit `0`.

3. **`src/main.rs`** — `emit_cleanup` grows a branch for a human report with
   nothing in it, decided by `report_has_nothing_to_report`: no rows, no
   unproven ownership, no out-of-scope participant, no scope block, no failure,
   no discovery diagnostic. The JSON and log branches are untouched.

4. **`src/policy.rs`** — `ClassifiedRoots::omission_diagnostics` stops
   reporting the by-design transient bucket. The omission *vector* is
   unchanged, so the late pre-spawn premise recheck still sees every omission.

## Work packages

- **A.** Add `SessionResult::NothingToClean` and settle the empty candidate
  list before the terminal precondition.
- **B.** Map it through the orchestrator to one stdout line and exit `0`.
- **C.** Extract `interactive_outcome` so the three endings are separately
  testable, and add the orchestrator's own test module.
- **D.** Add the empty-report verdict to the human surface of `emit_cleanup`.
- **E.** Silence the by-design transient omission bucket in the policy layer.
- **F.** Re-derive every shifted citation, and every count that moved.
- **G.** Update the user documentation, the deep dives, the skills, and the
  finding register.

## Required tests

- `src/interactive.rs`: an empty candidate set returns `NothingToClean` — and a
  *non-empty* one still refuses without a terminal, so the headless guard is
  not weakened.
- `src/main.rs`: the three interactive endings map to three distinct
  outcomes, and a surface that cannot be presented is still a usage error.
- `src/main.rs`: a report with nothing in it is the empty case, and a report
  carrying any of the six things the operator must read is not.
- `src/policy.rs`: the by-design transient bucket is not reported, and a
  vanished durable root still is.
- `tests/cli_contract.rs`: `clean` on an empty scope prints exactly
  `Nothing to clean.`, exits `0`, writes nothing to stderr, and leaves the JSON
  envelope unchanged.

## Verification commands

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check
```

The `-i` verdict additionally needs a real terminal. The repository has no pty
harness (finding 48), so the CLI-level proof of that one line was taken
manually over a pty and is recorded in the closure record.

## Documentation updates

- `architecture/overview.md` — findings 19 and 37, the §3 counts, the §3
  module table, and the concentration sentence.
- `architecture/10-reporting.md` — the refusal list.
- `architecture/13-orchestration.md` — the selection outcome, the report
  branches, the omission-diagnostics paragraph, and the function table.
- `architecture/14-testing-and-verification.md` — the census and the module
  inventory.
- `architecture/16-interactive.md` — the empty candidate set, the ordering
  against the terminal precondition, and the `SessionResult` vocabulary.
- `architecture/04-policy-and-scope.md` — the omission diagnostics.
- `docs/USAGE.md`, `docs/TROUBLESHOOTING.md`, `README.md` — the operator-facing
  behaviour.
- `.skills/terminal-and-interactive/SKILL.md`,
  `.skills/json-and-exit-contract/SKILL.md`, `.skills/test-evidence/SKILL.md`.

## Rejected alternatives

- **Widen the "nothing to clean" verdict to every zero-result run.** Rejected:
  a run that found units and deliberately left them alone must not print
  "Nothing to clean." — ADR 006's anchor rule exists so that "nothing was
  selected" never reads as "nothing was eligible". The verdict is therefore
  gated on the report having nothing at all in it.
- **Make the interactive empty case exit `1`.** Rejected: `1` means the
  operation failed or the scope was incomplete. Nothing failed; the scope was
  empty and proved so.
- **Keep the empty case as an error and only shorten the report.** Rejected:
  the two spellings of one scope must agree, and an unattended caller must be
  able to tell "there was nothing to do" from "the command was wrong".
- **Print the omission summary on stdout instead of stderr.** Rejected: it
  would put prose into a stream the machine formats own, and it would not
  remove the noise, only move it. The right move was to stop emitting a
  by-design fact.
- **Add a `--quiet` flag.** Rejected: the tool has no verbosity surface, and
  the noise here is a defect in *what* is reported, not in *how loudly*. A new
  flag would also be a CLI-surface change needing its own drift check.
- **Leave the `main.rs` progress-bar ordering unpinned.** Accepted as
  pre-existing: it needs a pty harness, and the four new tests in `main.rs` do
  not reach it.

## Acceptance criteria

- `cargo cleanme -i` over a scope with no cleanup unit prints exactly
  `Nothing to clean.` and exits `0`.
- A human cleanup report with nothing in it prints exactly `Nothing to clean.`
  and exits `0`.
- A Routine invocation on a machine with transient learned roots prints no
  omission summary; a machine with a vanished durable learned root still does.
- The JSON envelope and the log line are unchanged.
- Every shifted citation is re-derived and every moved count is re-derived
  from `src/`, not copied.

## Stop conditions

Stop and escalate if any of the following turns out to be true:

- the empty-report verdict would suppress a row, an unproven participant, an
  out-of-scope participant, a scope block, a failure, or a discovery
  diagnostic;
- silencing the transient bucket would change the *scope* rather than the
  *reporting* of it;
- the JSON envelope or the log line changes in any byte.

## Closure evidence requirements

- implementation commit;
- a requirement-to-evidence matrix over the five invariants above;
- the verification commands above, actually run, with their results;
- the pty evidence for the interactive verdict, and the statement that the
  repository still has no pty harness;
- the citation re-derivation, with the count of citations verified;
- the census re-derivation, with the method and its validation against
  `cargo test --lib -- --list`;
- known limitations, classified;
- disposition.

**Publication is not closure.** This corrective stays open until a published
release carries it.
