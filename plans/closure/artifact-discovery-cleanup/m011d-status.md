# Artifact Discovery and Cleanup M011D Status

Plan: `plans/implementation/artifact-discovery-cleanup/011d-cargo-selector-qualification-lifecycle.md`

Disposition: **closed — implementation, static guarantees, and local real-Cargo
evidence complete; the hosted matrix's recurring evidence is a maintenance lane,
not a closure blocker**

Implementation commit: the `phase11-hardening` branch commit that adds
`release/selector-qualification.json`, `.github/workflows/qualify-cargo-selectors.yml`,
and `scripts/check-selector-qualification.py`, and rewrites
`scripts/qualify-cargo-selectors.sh`.

Repository baseline at implementation: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Date: 2026-10-05

## Executive finding

`workspace::clean_capabilities_from_version` decided whether `--profile` and
`--package` were offered, from a hard-coded allowlist in `workspace.rs`. The
product **fails closed** for an unknown Cargo version — all-false capabilities,
degrading to whole-target scope instead of guessing. That is the correct runtime
posture, and it is precisely why the drift was invisible: nothing breaks when
the claim stops matching reality. The product just quietly stops offering a
selector, and every test stays green.

Correctness and verifiability are different properties. Only the second was
missing.

The maintenance script was worse: it defaulted to `1.89 1.91 1.92 stable` and
was wired into **nothing** — no workflow, no `release-check.sh`, no `ci.yml`.

## What was built

One authority, four artifacts proved to agree:

| Artifact | Role |
|---|---|
| `release/selector-qualification.json` | The single machine-readable authority. `profile_selector`, `package_selector`, and `exploratory` held **separately**, because a package-qualified release has strictly stronger preconditions. |
| `clean_capabilities_from_version` | What the product does — therefore what gets **proved**, not what gets to define the truth. |
| `.github/workflows/qualify-cargo-selectors.yml` | The hosted matrix, naming its toolchains **literally**. |
| `scripts/qualify-cargo-selectors.sh` | The real-Cargo assertions, consuming the policy rather than carrying a second copy. |

### The two non-obvious design decisions

**The matrix names its toolchains literally rather than reading the policy at run
time.** Reading it would make a missing version *impossible to detect* — which is
the whole defect the gate exists to catch. Naming them makes a dropped version a
detectable, gate-failing drift.

**`exploratory` is structurally incapable of becoming a claim.** A leaked entry is
rejected by the static checker *and* by
`exploratory_toolchains_are_never_promoted_by_accident` in Rust, the hosted
exploratory job is `continue-on-error`, and the script's exploratory mode always
exits 0 while reporting. A newly released Cargo version is not supported because
the research lane happened to pass.

## The script is an assertion harness, not a characterizer

Every claimed capability now produces one named result with an explicit verdict,
and **a claimed version that produced no assertion is a failure, not a pass** —
the exact false green a weaker harness reports.

Three failure classes are kept apart, and this is the load-bearing part:

| Class | Meaning |
|---|---|
| `semantic_regression` | A claimed capability did not hold on real Cargo. A product finding. |
| `toolchain_unavailable` | The exact pinned toolchain cannot be run here. An operational blocker, reported as one; it never silently drops a version from the allowlist. |
| `premise_failed` | A fixture could not be built for a reason unrelated to selectors. **A fixture that did not run is not evidence about Cargo.** |

Assertions are written against the **product's own command shape**
(`clean_args` in `cleanup.rs`): `clean [--dry-run --verbose] --offline --locked
--manifest-path ROOT --target-dir TARGET [--profile NAME | --package SPEC]`. Two
consequences: a claim proved only by a `.cargo/config.toml` discovered from a
particular working directory would be proving something the product never does,
and the whole-target fallback is the same command with **no selector** — the
product never passes `--workspace`, so the unqualified-Cargo path is qualified
against what the product actually sends.

Per phase: profile claims get dry-run non-mutation, release and custom profile
isolation, a configured `build.target` honoured (with the *default* target
directory asserted absent, so "reported both" is not evidence), the unselected
clean shape, and a distinct build directory for Cargo ≥ 1.91. Package claims
additionally get dry-run non-mutation, version-qualified spec resolution,
ambiguity rejection, selected-removed / sibling-preserved / other-profile-preserved,
a recorded shared-dependency observation, and — the condition M008D actually used
to authorise package selection — **configured `build.target` and explicit
`--target` selecting the same bytes**.

`--evidence` writes a bounded JSON artifact; a partially written or empty run
reports `fail`, never `pass`.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| One machine-checkable qualification matrix | `release/selector-qualification.json`, consumed by the checker, the workflow, and a Rust test | Pass |
| Script emits assertion-level evidence | 79 named assertions across 9 toolchains in the local run | Pass |
| Every runtime-enabled selector is in a guarded matrix | Checker compares the runtime allowlist to the policy to the matrix | Pass |
| Matrix covers the declared Cargo selectors | 9 profile-qualified + 2 package-qualified, all 9 locally exercised | Pass |
| Static matrix/drift checker with self-test | `check-selector-qualification.py --self-test`, 20 cases | Pass |
| Script has premise guards | `qualify-cargo-selectors.sh --self-test`, 15 cases, plus a real unprobeable toolchain | Pass |
| Hosted workflow, manual + scheduled, pinned actions, least privilege | `qualify-cargo-selectors.yml`; checker enforces every premise | Pass |
| Policy is not a second authority the script can ignore | Checker rejects any hardcoded `versions=(...)` in the script | Pass |
| Documentation updated | `docs/USAGE.md`, `architecture/07-workspace.md`, `architecture/14-testing-and-verification.md` | Pass |
| Distinct exploratory lane that cannot gate | `exploratory-stable` job, `continue-on-error: true`, checker-enforced | Pass |

## Verification run

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — **passed, 261 tests**, up from 250.
  Three new `workspace.rs` tests, each of which would fail if the runtime
  allowlist and the policy disagreed in either direction — including the
  `exploratory` leak and the `1.99` / `1.9` / `1.99.0-nightly` conflation cases.
- `python3 scripts/check-selector-qualification.py` — **passed**.
- `python3 scripts/check-selector-qualification.py --self-test` — **passed**, 20
  cases, each with a control asserting the mutation actually applied.
- `bash scripts/qualify-cargo-selectors.sh --self-test` — **passed**, 15 cases.
- **`bash scripts/qualify-cargo-selectors.sh --evidence …` — real-Cargo run
  against all nine claimed releases, 79 assertions, exit 0.** Every toolchain was
  installed for the run; the 1.91.1 / 1.92.0 / 1.93.1 / 1.94.1 / 1.99.0
  toolchains were fetched to make this a complete run rather than a partial one.

## Defects this work found in its own new code

Recorded because the repository's own evidence discipline asks for it, and
because all three are the class this milestone is about.

1. **The script's first real run reported 8 failures** — every one a bug in the
   harness, not in Cargo. The commands ran with `--manifest-path` from the wrong
   working directory, so `.cargo/config.toml` was never discovered; a `distinct
   build directory` assertion ran for Cargo 1.89/1.90, which have no separate
   build directory; and `--workspace` was asserted although the product never
   passes it. The harness reported these honestly as failures, which is how they
   were found at all.
2. **Phase ordering produced a wrong pass.** The package phase ran second and
   asserted "the release profile survived a `--package` clean" — against output
   the profile phase had already deleted. The assertion would have passed for the
   wrong reason. Fixed by rebuilding the fixture at the top of every phase.
3. **`comm` silently reported a wrong count.** It re-checks sort order under the
   *current* locale while the snapshots were produced with `LC_ALL=C sort`, so it
   warned, returned a wrong `removed_paths`, and still exited 0. A silently wrong
   observation in a retained evidence artifact is worse than none.

## Limitations and unresolved findings

1. **The hosted matrix has not run.** This branch's workflows run on merge to
   `main`; the local run above is the equivalent evidence and covers the same nine
   toolchains. The workflow's *first* hosted execution — and therefore the
   path-filtered, scheduled, and artifact-retention behaviour — is unobserved. It
   is a maintenance lane rather than a release gate, so unlike M011A/M011B/M011C
   it does not hold closure: the claim is enforced statically, and the recurring
   real-Cargo re-qualification is what maintains it.
2. **The exploratory lane has not run on a newer stable than 1.99.0.** The local
   machine's `stable` is 1.99.0, which the policy already claims, so the
   exploratory observations this run produced describe an already-qualified
   release. The lane's value appears on the first stable that exceeds the
   allowlist.
3. **Windows and macOS layouts are not exercised by this harness.** It is a Bash
   script running on `ubuntu-latest`; the Windows and macOS selector behaviour
   rests on the contract plus the existing hosted release matrix, exactly as the
   pre-existing script did. Not a regression, but not fixed either.

## Closure status and unblocking

**Closed.** All five work packages are implemented, the static gate is proved in
the failing direction with controls, and real-Cargo evidence for all nine claimed
releases is recorded above. The promotion rule is written into the policy file
itself and into `docs/USAGE.md`: adding an entry requires a plan or closure
record carrying exact hosted real-Cargo evidence for that version, not merely an
edit to the file or to the Rust allowlist.

**Downstream effect:** no plan was blocked on M011D. The support claim can now be
audited and its evidence regenerated on demand, which is what the C013/C015
lineage required and could not previously supply.
