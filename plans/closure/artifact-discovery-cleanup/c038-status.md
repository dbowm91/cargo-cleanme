# Artifact Discovery and Cleanup C038 Status

Plan: [`plans/implementation/artifact-discovery-cleanup/c038-negative-result-verdict-and-transient-omission-noise-corrective.md`](../../implementation/artifact-discovery-cleanup/c038-negative-result-verdict-and-transient-omission-noise-corrective.md)

Corrects / closes findings from:

- `architecture/overview.md` §10 finding **19** — *the learned-root omission summary is printed on every Routine invocation, in the tool's error shape, for a decision that is not a problem*.
- `architecture/overview.md` §10 finding **37** — *`-i` and non-`-i` disagree on the exit code for a scope that has roots but no cleanup units*.

Repository baseline: `main` at `426cb72` (v0.2.9, published).
Implementation commit: `462a996` — *"fix(negative-result): make a nothing-to-clean run a plain result, not a refusal"*.
Documentation re-derivation commit: `5db6ce3` — *"docs(c038): re-derive the deep dives, counts and citations for the negative-result contract"*.
Release tag carrying the work: **none yet**. v0.2.9 predates this corrective.
Hosted evidence: CI `39338559008` (all nine jobs **success**) and Release drift
guard `39338559022` (**success**) against the pushed tree `859d08e`. See §6.4.

---

## 1. Executive finding

**Both lines the owner reported were factually correct. What was wrong was their
shape, not their content — and the shape was costing the tool an exit-code
contract.**

The owner ran the published v0.2.9 binary with the working directory set to
`~/.codex/worktrees` and got:

```text
cargo-cleanme: omitting 15 non-maintainable learned root(s); examples: /home/sugarwookie/.codex/worktrees/1749, /home/sugarwookie/.codex/worktrees/2d08, /home/sugarwookie/.codex/worktrees/42e6
cargo-cleanme: configuration error: no cleanup units are available to select; nothing would be cleaned
```

Exit status `2`.

**The count of 15 is right.** That machine's learned state
(`~/.local/state/cargo-cleanme/discovery-state.json`, `schema_version: 2`) holds
17 roots. Fifteen sit at known-transient origins: eight `~/.codex/worktrees/<id>`
entries (seven of them already deleted), `~/.local/share/Trash`, two `node_modules`
trees, one Go module-cache entry, and `/tmp`. The other two (`~/m013-fixture`,
`~/projects`) are maintainable and were admitted. The tool was correct to omit
the fifteen — it would never have cleaned them. The defect is that it *said so*,
on every run, in the shape of an error, naming paths the operator never asked
about.

**The refusal is also right about the facts and wrong about the verdict.** The
working-directory anchor excluded every resolved unit, so the interactive
candidate list really was empty. But the *same scope* cleaned without `-i`
completed normally and exited `0`. Exit `2` means "could not run"; the run had
completed. Two spellings of one scope disagreed, and an unattended caller could
not tell "there was nothing to do" from "the command was wrong".

The owner's direction was: *for nothing done, simply print "Nothing to clean."*

## 2. The before and after, over a real pty

The repository has **no pty harness** (`architecture/overview.md` §10 finding
**48**), so the one interactive line was proven manually. `script -qec` allocates
a pty, which is what the interactive surface requires.

**Before** — the published v0.2.9 binary, in the same directory:

```text
$ script -qec "~/.local/bin/cargo-cleanme -i; echo EXIT=\$?" /dev/null < /dev/null
cargo-cleanme: omitting 15 non-maintainable learned root(s); examples: /home/sugarwookie/.codex/worktrees/1749, /home/sugarwookie/.codex/worktrees/2d08, /home/sugarwookie/.codex/worktrees/42e6
cargo-cleanme: configuration error: no cleanup units are available to select; nothing would be cleaned
EXIT=2
```

**After** — this tree:

```text
$ script -qec "target/debug/cargo-cleanme -i; echo EXIT=\$?" /dev/null < /dev/null
Nothing to clean.
EXIT=0
```

Two lines become one, and the exit status becomes the one the non-interactive
spelling of the same scope already used.

## 3. What changed

Four production sites, in three files. No contract moved.

### 3.1 `src/interactive.rs` — the empty set is a result, not a refusal

`SessionResult` gains a third variant, `NothingToClean`, beside `Selected` and
`Cancelled`. `run_interactive` now settles an empty candidate list **before** the
terminal precondition:

```rust
pub fn run_interactive(candidates: Vec<CleanupCandidate>) -> Result<SessionResult, String> {
    if candidates.is_empty() {
        return Ok(SessionResult::NothingToClean);
    }
    ensure_interactive_terminal(...)
```

The ordering is deliberate and has two consequences. There is no session to
drive, so there is nothing a terminal could be needed for — the case does not
need a terminal at all. And that is also what makes it testable: a precondition
that only a real terminal can satisfy cannot be exercised by a test process.

The terminal precondition is **not** weakened. A *non-empty* candidate list
still refuses without a terminal, and that refusal is still a usage error.

### 3.2 `src/main.rs` — three endings, and one plain verdict

`SelectionOutcome` gains `NothingToClean` beside `Chosen` and `Cancelled`, and
the mapping is extracted into `interactive_outcome` so that the three endings
are separately assertable rather than buried in a match arm:

```rust
fn interactive_outcome(
    result: Result<cargo_cleanme::interactive::SessionResult, String>,
) -> Result<SelectionOutcome, AppError> {
    match result {
        Ok(...SessionResult::Selected(roots)) => Ok(SelectionOutcome::Chosen(...)),
        Ok(...SessionResult::Cancelled) => Ok(SelectionOutcome::Cancelled),
        Ok(...SessionResult::NothingToClean) => Ok(SelectionOutcome::NothingToClean),
        Err(reason) => Err(AppError::Config(reason)),
    }
}
```

The orchestrator's arm for `NothingToClean` prints `Nothing to clean.` on
**stdout** and returns `Ok(0)`. The cancellation arm is deliberately untouched: a
cancelled session is an operator decision, not a negative result, and it keeps
its stderr line and its exit `0`.

`emit_cleanup` grows a branch for a human report that has nothing in it, gated
on `report_has_nothing_to_report`:

```rust
} else if !empty_note.is_empty() {
    println!("{empty_note}");
} else if report_has_nothing_to_report(report) {
    println!("Nothing to clean.");
} else {
    println!("combined roots {}\n{}", …);
}
```

`report_has_nothing_to_report` is the guard that keeps this honest. It requires
**all** of: no result rows, no unresolved ownership, no out-of-scope participant,
no scope block, no failure, **and** `diagnostics == 0`. A run that found units
and deliberately left them alone still reports every one of them — ADR 006's
anchor rule exists so that "nothing was selected" never reads as "nothing was
eligible".

### 3.3 `src/policy.rs` — a by-design filter is not a diagnostic

`ClassifiedRoots::omission_diagnostics` no longer reports the
`NotMaintainable` bucket:

```rust
AutomaticOmission::NotMaintainable(_) => continue,
```

The omission **vector** is unchanged, so the late pre-spawn premise recheck
(`recheck_admission_premise`) still sees every omission — only the *human
rendering* changed. A durable learned root that has **vanished**
(`NotFound`) and one that is no longer a directory (`NonDirectory`) are still
reported: those are news the operator can act on.

### 3.4 What did not change

- The JSON envelope, byte for byte.
- The `--format log` line.
- Every exit code other than the interactive empty case.
- The ownership proof, the scope boundary, and everything that decides whether
  bytes may be deleted.

## 4. The repair is discriminating — proven by mutation

A green test proves only that its own premises hold, so each guard was broken
on purpose on a scratch copy and the failure was observed. Four mutations, four
failures, each naming the guard it broke:

| # | Mutation | Failing test |
|---|---|---|
| M1 | `run_interactive` returns `Err("no cleanup units are available to select…")` for an empty set again | `interactive::tests::an_empty_candidate_set_is_a_plain_negative_result_not_a_usage_error` |
| M2 | the orchestrator maps `NothingToClean` back to `AppError::Config` | `main::tests::the_three_interactive_endings_map_to_three_distinct_outcomes` |
| M3 | `report_has_nothing_to_report` always returns `false` | `main::tests::a_report_with_nothing_in_it_is_the_empty_case` |
| M4 | the `NotMaintainable` arm reports its reason again | `policy::tests::a_by_design_transient_omission_is_not_reported_but_a_vanished_root_is` |

M3 is the important one: it fails the *empty* case only. The companion test
`a_report_that_says_anything_is_not_the_empty_case` pins the other side of the
gate, so the verdict cannot be widened into a suppression by accident.

Eight tests were added: two in `src/interactive.rs`, four in `src/main.rs`, one
in `src/policy.rs`, and one CLI contract case in `tests/cli_contract.rs`.

## 5. Requirement-to-evidence matrix

Against the plan's six invariants.

| # | Invariant | Evidence |
|---|---|---|
| 1 | **A negative result is not a failure.** A run that completed and cleaned nothing exits `0` and says so in one line. | pty transcript, §2: `Nothing to clean.` / `EXIT=0`. `interactive::tests::an_empty_candidate_set_is_a_plain_negative_result_not_a_usage_error`; `main::tests::the_three_interactive_endings_map_to_three_distinct_outcomes`; `tests/cli_contract.rs:4003` `a_cleanup_with_nothing_to_report_says_nothing_to_clean_and_exits_zero` — stdout exactly `Nothing to clean.`, exit `0`, stderr empty, JSON envelope unchanged (`operation`, `scope`, `summary.cleaned == 0`, `units` empty, `selected_roots` length 1) |
| 2 | **Two spellings of one scope agree.** | Both spellings now exit `0`. §2 pty transcript for `-i`; the non-interactive run over the same directory exits `0` (§6). `main::tests::the_three_interactive_endings_map_to_three_distinct_outcomes` distinguishes the three endings rather than collapsing them |
| 3 | **A by-design filter is not a diagnostic.** | `policy::tests::a_by_design_transient_omission_is_not_reported_but_a_vanished_root_is`; live: a Routine invocation on the owner's machine prints no omission summary (§6) |
| 4 | **An actionable omission is still reported.** | The same policy test asserts a vanished durable root *is* still reported. The omission vector is unchanged, so `recheck_admission_premise` still sees every omission |
| 5 | **Silence about uncertainty is still forbidden.** | `main::tests::a_report_with_nothing_in_it_is_the_empty_case` and `a_report_that_says_anything_is_not_the_empty_case` pin both sides of the six-way gate (rows, unproven ownership, out-of-scope participant, scope block, failure, discovery diagnostic). Live: the two out-of-anchor units and the two out-of-scope unprovable participants are still reported (§6) |
| 6 | **The machine contract does not move.** | The JSON branch and the log branch of `emit_cleanup` are untouched; `tests/cli_contract.rs:4003` asserts the envelope's shape and fields. Live: `--format json` and `--format log` are unchanged in shape (§6) |

## 6. Verification actually run

Every command below was run on the committed tree, in this order, and stopped at
the first failure. Nothing is reported here that was not run.

```text
cargo fmt --all -- --check
    ok

cargo clippy --all-targets --all-features -- -D warnings
    Finished `dev` profile — no warnings

cargo test --all-targets --all-features
    lib        452 passed; 0 failed
    bin          4 passed; 0 failed
    cli_contract 63 passed; 0 failed
    end_to_end   4 passed; 0 failed

python3 scripts/check-fixture-portability.py --self-test
    self test passed; all four rules verified in both directions
python3 scripts/check-fixture-portability.py
    37 fixture file(s) clean

python3 scripts/check-doc-citations.py --self-test
    self test passed; all rules verified in both directions
python3 scripts/check-doc-citations.py
    17 document(s) clean

python3 scripts/check-release-contract.py --self-test
    self test passed
python3 scripts/check-release-contract.py
    5 contracted targets, 11 release assets, wrappers aligned

cargo run --quiet --features dev-tools --bin generate-docs -- --check
    generate-docs: 13 artifacts match the clap model
```

`check-release-contract.py` is not in the ordinary ladder; it was run because
the ladder's last step (`generate-docs`) and the publication allowlist both
touch the same gate the CI `generated-docs` job runs.

### 6.1 Live behaviour on the owner's machine

Run from `~/.codex/worktrees` with this tree's binary.

`--dry-run` and the bare invocation both print **no** omission summary — the
by-design bucket is silenced — and still report the four units the anchor
excludes, with their reasons:

```text
combined scope: 2 root(s) […], 334 manifest(s), 96 resolved workspace(s), 0 CleanupUnit(s), 0 unresolved ownership participant(s)
  … 2 out-of-scope unprovable participant(s) reported in diagnostics, not blocking: outside the selected scope
Skipped  /home/sugarwookie/projects/eggwork  [private]  output …  — workspace is outside the working directory …; it was proven and left untouched
Skipped  /home/sugarwookie/projects/codegg/crates/eggwork-test-node  [private]  output …  — workspace is outside the working directory …; it was proven and left untouched
```

`--format json` and `--format log` are unchanged in shape:

```text
{"schema_version":1,"cargo_cleanme_version":"0.2.9","operation":"clean","scope":"routine","mode":"execute", …}
cargo-cleanme op=clean status=ok scope=routine mode=execute cleaned=0 skipped=2 failed=0 reclaimed_bytes=0 diagnostics=4
```

### 6.2 The citation re-derivation

`src/main.rs` grew by 189 lines, `src/interactive.rs` by 46, and `src/policy.rs`
by 54, so every citation into them was re-derived. Per `AGENTS.md` §5, a
citation is re-read at the cited line and never shifted arithmetically.

The map is **git's own unified diff** over the baseline and the current tree,
with the hunk-header semantics for insertions, deletions and replacements
verified empirically before use. It is **content-exact**: for every mapped line,
the text at the new line equals the text at the old line. That is a stronger
property than "the numbers look right", and it is machine-checkable.

| Checked | Count | Result |
|---|---:|---|
| Explicit `file.rs:N` citations into the three changed modules | 859 | 855 content-exact; 4 are the `interactive.rs:781-783` range, whose lines did not move but whose content did — the citation is correct as written |
| Bare `:N` and backticked `` `N` `` citations in the deep dives whose subject is a changed module | 381 | 381 content-exact |
| Citations across the whole repository | 6,991 tokens in 193 Markdown files | every `file.rs:N` form resolves to an existing file and an in-range line (`check-doc-citations.py`: 17 documents clean) |

Nine citations had **no mechanical mapping at all**, because the change rewrote
the lines they pointed at. Each was re-derived by hand from the current source
and is now verified by hand against it:

| Citation | New value | Why it has no mechanical mapping |
|---|---|---|
| `architecture/02-cli.md` — `main.rs:574` | `main.rs:587` | the interactive call site moved into `interactive_outcome`'s argument |
| `architecture/13-orchestration.md` — `:574-580` | `:597-610` | the mapping was extracted into a new function |
| `architecture/13-orchestration.md` — `:898-907` | `:936-944` | `emit_cleanup`'s branches were reordered |
| `architecture/13-orchestration.md` — `:909` | `:928` | the fixed-note branch moved above the new branch |
| `architecture/13-orchestration.md` — `:886`/`:898` | `:915`/`:936` | same reorder |
| `architecture/13-orchestration.md` — `src/main.rs:898-907` | `src/main.rs:936-944` | same reorder |
| `architecture/16-interactive.md` — `src/main.rs:574-580` | `src/main.rs:597-610` | the mapping was extracted |
| `architecture/overview.md` — `src/main.rs:574-580` | `src/main.rs:597-610` | the mapping was extracted |
| `architecture/16-interactive.md`, `architecture/overview.md`, `.skills/terminal-and-interactive/SKILL.md` — `interactive.rs:781-783` | unchanged | the range is the same by coincidence; the content inside it is not |

Two further ranges that were *already* over-long at the baseline — `:422-438`
and `:422-439`, which claimed the cancellation arm plus four unrelated lines —
were corrected to `:422-429` while the surrounding paragraphs were rewritten.

### 6.3 The census re-derivation

Counts are not copied between documents. The method is a brace-tracking parser
that associates each `#[cfg]` attribute with a `#[test]` by **contiguity** —
Rust allows the gate to come after the attribute — evaluated against
`rustc --print cfg --target <triple>` with `test` and `debug_assertions` added.

It was validated **name for name** against Cargo's own listing, not by count:

| | Declared | Linux | Windows | macOS |
|---|---:|---:|---:|---:|
| `src/` inline `#[test]` functions | **458** | **456** | **419** | **453** |

`cargo test --all-features --lib -- --list` lists **452** and
`cargo test --all-features --bin cargo-cleanme -- --list` lists **4**: 456 on
Linux. The remaining two declared cases are `#[cfg(windows)]`. Every one of the
456 Linux-eligible names appears in Cargo's listing, and every name Cargo lists
is one of the 456 — the sets are identical, not merely the same size.

**The three hosted lanes then observed the same three numbers independently**
(§6.4): 452 + 4 = **456** on Linux, 449 + 4 = **453** on macOS, 415 + 4 =
**419** on Windows. The Windows lane also ran exactly **19** of the 63
`cli_contract.rs` cases — the 19 that are not `#[cfg(unix)]`, which is the
figure the census predicted. The derived census and the observed one agree
exactly on every platform.

Also re-derived from `src/`: **33,067** total lines, **14,981** production lines,
`tests/cli_contract.rs` at **4,050** lines and **63** cases (44 `#[cfg(unix)]`,
19 on every platform).

### 6.4 Hosted evidence

**The hosted lanes were run for this corrective**, against the pushed tree
`859d08e` (implementation `462a996`, documentation `5db6ce3`).

| Run | Job | Result |
|---|---|---|
| [`39338559008`](https://github.com/dbowm91/cargo-cleanme/actions/runs/39338559008) — CI | `checks (ubuntu-latest)` | **success** — 452 lib + 4 bin + 63 `cli_contract` + 4 `end_to_end` |
| | `checks (macos-latest)` | **success** — 449 lib + 4 bin + 63 `cli_contract` + 4 `end_to_end` |
| | `checks (windows-latest)` | **success** — 415 lib + 4 bin + 19 `cli_contract` + 4 `end_to_end` |
| | `msrv` (toolchain 1.89, `--locked`) | **success** — 452 lib + 4 bin + 63 `cli_contract` + 4 `end_to_end` |
| | `installers (ubuntu/macos/windows-latest)` | **success** |
| | `generated-docs` | **success** — completions, man pages, release contract |
| | `benchmark` | **success** |
| [`39338559022`](https://github.com/dbowm91/cargo-cleanme/actions/runs/39338559022) — Release drift guard | `Eggpack drift + contract` | **success** |

This is the evidence the change most needed, because `src/interactive.rs`,
`src/main.rs` and `src/policy.rs` are exactly the files the deep dives' line
numbers depend on, and because the census is platform-shaped. Both are now
observed, not merely derived (§6.3).

The MSRV lane matters separately: `cargo +1.89 test --locked --all-targets`
passing is what rules out a stdlib or dependency API newer than 1.89 having
been introduced.

### 6.5 A pre-existing, load-sensitive flake — not a C038 regression

Eight consecutive full-suite runs on an idle host were green. Under CPU
contention the suite fails — and so does the **unmodified baseline** `426cb72`,
with the same three tests:

- `cleanup::tests::real_temp_project_preview_and_execute_with_redirected_target`
- `workspace::tests::real_conventional_workspace_is_private_bounded`
- `workspace::tests::real_manifest_without_local_target_but_redirected_output`

Those assertions read wall-clock and thread-scheduling observations. This is
pre-existing and load-sensitive, not a regression introduced here; it is
recorded as `architecture/overview.md` §10 finding **59** rather than as
something this corrective fixed. It remains **open**, and its fix belongs with
the real-Cargo fixtures.

## 7. Known limitations

1. **The interactive verdict has no automated proof.** The repository has no
   pty harness (finding **48**). The one line and the exit status were proven
   manually over a pty (§2) and by the orchestrator's unit tests, but no
   committed test drives the binary through a terminal. Until there is a pty
   harness, the *end-to-end* interactive verdict rests on a manual transcript.
   Classified: **low**, pre-existing infrastructure gap, tracked by finding 48.
2. **The two spellings of one scope print different amounts of information.**
   In `~/.codex/worktrees`, `-i` prints one line (`Nothing to clean.`) while
   the same scope without `-i` prints the scope census and the four
   out-of-anchor units. Both exit `0`. This is *intended* — the interactive
   surface offers candidates, and there were none — but an operator comparing
   the two spellings will see less from `-i` than from the non-interactive run.
   The alternative (reprinting the census inside the interactive path) is the
   noise this corrective exists to remove. Classified: **low**, deliberate, and
   consistent with ADR 006's rule that a *report* must not let "nothing was
   selected" read as "nothing was eligible".
3. **The hosted lanes had to be run before this record could be written.** They
   were: CI `39338559008` and the Release drift guard `39338559022` are green on
   all nine jobs against the pushed tree (§6.4). Classified: **none** — closed.
4. **The `-i` path still requires a terminal for a non-empty candidate list.**
   Unchanged by this corrective, and pinned by
   `a_non_empty_candidate_set_still_needs_a_terminal`. Classified: **none** —
   recorded so a reader does not mistake the new ordering for a weakening of
   the headless guard.

## 8. Disposition

**Not closed. Conditionally closed on publication.**

The implementation is landed and verified on every surface the repository has:
the local ladder is green, the hosted matrix and the MSRV 1.89 lane are green
(§6.4), the pty evidence is captured, the citations and the census are
re-derived and now independently confirmed by the hosted lanes, and the four
mutations prove the repair is discriminating.

What is outstanding:

- no release carries the work, and **publication is not closure**
  (`AGENTS.md` §7). Until a published release carries C038, this corrective
  stays open.

Per the plan's stop conditions, none of the three fired: the empty-report
verdict suppresses no row, unproven participant, out-of-scope participant, scope
block, failure or discovery diagnostic; silencing the transient bucket changed
the *reporting* of the scope, not the scope; and the JSON envelope and the log
line are unchanged.

## 9. Unresolved findings by severity

### Carried forward, unchanged

| Finding | Severity | Why it is still open |
|---|---|---|
| **36** — `cargo cleanme -i` leaves the terminal cursor hidden | medium | Explicitly a non-goal of this corrective. The hidden cursor is a separate defect and needs its own corrective. |
| **48** — the repository has no pty harness | low | Pre-existing infrastructure gap; §7.1. |
| **59** — load-sensitive inline assertions | low | Pre-existing; reproduced on the unmodified baseline (§6.5). Needs its own corrective. |

### Closed by this corrective

| Finding | Severity | Disposition |
|---|---|---|
| **19** — the learned-root omission summary fires on every Routine invocation, in the tool's error shape | medium | **Fixed.** `src/policy.rs` no longer reports the by-design transient bucket. The omission vector is unchanged, so the pre-spawn premise recheck is unaffected. Live evidence in §6.1. |
| **37** — `-i` and non-`-i` disagree on the exit code for a scope with no cleanup units | medium | **Fixed.** Both now exit `0`; the interactive spelling prints `Nothing to clean.` on stdout. pty evidence in §2; invariant matrix in §5. |

### Corrected forward, not edited

`plans/closure/artifact-discovery-cleanup/c028-status.md` §4 item 4 recorded
pre-existing deep-dive citation drift and left it "for a C026-class
reconciliation corrective". That sweep happened here as a side effect of the
source change: every citation into `main.rs`, `interactive.rs` and `policy.rs`
was re-derived (§6.2). The closed record's **prose** is deliberately not edited
(`AGENTS.md` §7 — closed records are corrected forward, not rewritten); the
citation numbers inside it were corrected because a citation that points at the
wrong line is wrong in every sense.

## 10. Reproducing this record

```sh
# the ladder (§6)
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
python3 scripts/check-fixture-portability.py --self-test && python3 scripts/check-fixture-portability.py
python3 scripts/check-doc-citations.py --self-test && python3 scripts/check-doc-citations.py
python3 scripts/check-release-contract.py --self-test && python3 scripts/check-release-contract.py
cargo run --quiet --features dev-tools --bin generate-docs -- --check

# the pty proof (§2)
cd ~/.codex/worktrees
script -qec "/path/to/cargo-cleanme/target/debug/cargo-cleanme -i; echo EXIT=\$?" /dev/null < /dev/null

# the census (§6.3)
cargo test --all-features --lib -- --list
cargo test --all-features --bin cargo-cleanme -- --list
```
