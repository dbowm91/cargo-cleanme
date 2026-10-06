# C023 — v0.2.0 destructive-safety patch-release corrective: closure record

Status: **closed**, with the publication evidence recorded in §12 and three
findings recorded rather than fixed in §11.

Plan: [`c023-v0.2.0-destructive-safety-patch-release-corrective.md`](../../implementation/distribution-release-update/c023-v0.2.0-destructive-safety-patch-release-corrective.md)
Baseline: `eaa7bba1cd70513a7bf48a99d7d9ffe19334a539`

## 1. What was wrong

Two destructive defects shipped in cargo-cleanme 0.2.0. Both report success and
exit `0`. Both were found by post-release interrogation, not by a test.

**A pre-subcommand `--dry-run` executed a real cleanup.**
`cargo cleanme --dry-run clean ROOT` — the argv Cargo itself passes to an
external subcommand, and the spelling a user types — parsed cleanly, had the
flag discarded, and fell through to cleanup's default mode. It ran `cargo
clean` and printed `Cleaned …`.

**A covering output root could contain another project's sources.** The
source-disjointness check in `build_groups` consulted only the group's own
owner's members. A workspace whose declared output directory was not literally
named `target`, containing an independently resolved nested project, was
classified `PrivateBounded` and cleaned.

## 2. Premise evidence, against the immutable published binary

Both defects were reproduced against the published 0.2.0 asset, sha256
`46f976432facd54a822eed26273f8ca443a311c31de902c61dc47446ca8d567f`
(`cargo-cleanme-x86_64-unknown-linux-gnu`), not against a build of the tree.

The instrument is a logging `cargo` placed first on `PATH`. cargo-cleanme spawns
cargo with `Command::new("cargo")` (`src/cleanup.rs:442`, `src/cleanup.rs:462`,
`src/workspace.rs:82`), so this wrapper **is** the tool the subject resolves; it
records the argv and then `exec`s the real cargo, so every behaviour observed is
real behaviour rather than a stub's fiction.

### A1 — `--dry-run` destroyed build artifacts

```
$ cargo cleanme --dry-run clean <case>/demo
combined scope: 1 root(s), 1 manifest(s), 1 resolved workspace(s), 1 CleanupUnit(s), 0 unresolved ownership participant(s)
Cleaned  <case>/demo  [private]  output <case>/demo/target  before  68.00 KiB  after   0.00 B  observed decrease  68.00 KiB  — Removed 3 files, 64.4KiB total
exit=0

cargo invocations:
    locate-project --workspace --manifest-path <case>/demo/Cargo.toml
    metadata --offline --locked --no-deps --format-version 1 --manifest-path <case>/demo/Cargo.toml
    clean --offline --locked --manifest-path <case>/demo/Cargo.toml --target-dir <case>/demo/target
cargo-clean invocations: 1

target/debug exists: yes -> no
target bytes:        65580 -> (absent)
src bytes:           unchanged
VERDICT: DEFEAT
```

### A2 — a neighbouring project's sources were deleted

`outer` declares `build.target-dir = <case>/outer/out`. `inner` is an
independently resolved workspace whose **sources** live inside that directory,
and which resolves its *own* output elsewhere, so the pair is not merely a
shared group.

```
$ cargo cleanme clean <case>
combined scope: 1 root(s), 2 manifest(s), 2 resolved workspace(s), 1 CleanupUnit(s), 0 unresolved ownership participant(s)
Cleaned  <case>/outer  [private]  output <case>/outer/out  before  80.00 KiB  after   0.00 B  observed decrease  80.00 KiB  — success
exit=0

cargo invocations:
    locate-project --workspace --manifest-path <case>/outer/Cargo.toml
    metadata --offline --locked --no-deps --format-version 1 --manifest-path <case>/outer/Cargo.toml
    locate-project --workspace --manifest-path <case>/outer/out/inner/Cargo.toml
    metadata --offline --locked --no-deps --format-version 1 --manifest-path <case>/outer/out/inner/Cargo.toml
    clean --offline --locked --manifest-path <case>/outer/Cargo.toml --target-dir <case>/outer/out
cargo-clean invocations: 1

inner source bytes: DESTROYED or ALTERED
  -./.cargo/config.toml
  -./Cargo.lock
  -./Cargo.toml
  -./src/main.rs
VERDICT: DEFEAT
```

The inner project's entire source tree — manifest, lockfile, config, and
`src/main.rs` — was deleted, and the tool reported `Cleaned … [private] …
success`.

**Why `inner` resolves its own output elsewhere.** The first attempt at this
fixture had both workspaces inheriting one `build.target-dir`, so the group was
refused as `shared` for an unrelated reason and the fixture reported "safe"
while testing nothing about source containment. That is C012's exact failure
shape and it is stated here so the next reader does not repeat it.

### The same fixtures against the fixed candidate

| Fixture | Published 0.2.0 | Fixed candidate |
|---|---|---|
| A1 `--dry-run clean ROOT` | 1 `cargo clean`, 65,580 bytes gone, exit 0 | 0 `cargo clean`, `Simulated … no cargo clean command was invoked`, bytes unchanged |
| A2 neighbour sources | 1 `cargo clean`, 4 source files gone, exit 0 | 0 `cargo clean`, 4 source files byte-identical |

The A2 refusal is observable, not silent:

```
combined scope: 1 root(s), 2 manifest(s), 2 resolved workspace(s), 0 CleanupUnit(s), 0 unresolved ownership participant(s)
cleanup: 0 cleaned, 0 skipped, 0 failed; … 1 filesystem diagnostics

$ cargo cleanme --stats clean <case>
cleanup stats: … groups_measured=0 … empty_skipped=1 … uncertain_skipped=1 …

$ cargo cleanme scan <case>
  0.00 B inventory estimate across 0 inactive Cargo output groups
1 filesystem diagnostics; rerun with a bounded root if needed
  warning: output root <case>/outer/out contains a workspace source tree, not a
  build artifact; it is not sized and not eligible for cleanup
```

The cleanup envelope carries the diagnostic only as a count, because a
source-overlap group is dropped from the inventory by design. `scan` is the
documented qualification surface and states the reason in full, in both the
human and the JSON rendering. C023 did not change that schema: §17 puts it out
of scope, and the diagnostic text already exists and is reachable.

## 3. What the fixes already were

Both fixes were on `main` at the C023 baseline in `eaa7bba`. C023 did not write
them. It qualified them, repaired the tree they could not be qualified in, and
published them.

- `src/cli.rs` — `invocation()` reconciles the root-level `--dry-run` into the
  `Clean` and `Update` arms. The root flag is deliberately **not**
  `global = true`, because that would collide with `update --dry-run`'s own
  argument id.
- `src/workspace.rs` — `build_groups` consults the global workspace universe for
  source disjointness, not just the group's own owner.

## 4. Hosted macOS regression, repaired

`src/discovery.rs`'s prune test built a `dua_core::Entry` from a
`std::fs::FileType`. That field is `std::fs::FileType` on Linux and
`dua_core::FileType` on macOS and Windows, so the helper compiled on the lane
that wrote it and could not compile on the lane that runs it.

Hosted run 37492549416 failed at `src/discovery.rs:931` on macOS. **Ubuntu and
Windows were cancelled and were therefore not evidence either** — for that run,
"two lanes passed" was not a statement about anything.

The helper now collects entries by walking a real temporary tree through
`dua_core::walk_roots`, the same machinery `discover_manifests` drives, so the
entry under test is always the platform's own representation.

`scripts/check-fixture-portability.py` gains a fourth rule for this class: a
hand-built `dua_core::Entry` literal, with a `fixture-scope:` escape hatch like
the existing rules. It is self-tested in both directions — a literal is flagged,
an entry from `walk_roots` is accepted, a literal with a marker is accepted,
and a mention of the type in prose is not a literal.

## 5. Release-blocking evidence added

### Dry-run dispatch

`every_accepted_simulation_spelling_reaches_simulate_and_spawns_no_cargo_clean`
walks six accepted spellings — bare `--dry-run`, before the subcommand, after
it, both legacy `--dryrun` aliases, and the root-flag/preview combination — and
asserts the mode label, the machine summary, **zero `cargo clean`
invocations**, and untouched bytes on each. The assertion that matters is
`clean_calls() == 0`: the mode label can be right while execution is wrong.

`the_cargo_plugin_simulation_spelling_also_spawns_no_cargo_clean` covers the
Cargo external-subcommand entry point, which is the spelling a user actually
types. It asserts `assert_staged_is_resolved` before the run, because without
that premise it would report on whichever `cargo-cleanme` happened to be
installed — C012's exact failure.

`clean_rejects_simulate_and_cargo_preview_together_at_parse_time` pins the one
combination with no coherent public meaning as a parse-time refusal. The
asymmetry is deliberate: clap can make a subcommand's flags conflict but cannot
make a root flag conflict with a subcommand's, so the same two flags spelled
across the boundary are accepted and resolved to simulation.

### Source containment

`a_neighbouring_projects_sources_inside_a_covering_output_root_are_never_cleaned`
builds the A2 shape and drives the production cleanup entry point with a runner
that resolves workspaces from real `.cargo/config.toml` files and **deletes
output the way Cargo deletes**. A regression would therefore take the
neighbour's files with it rather than merely changing a label.

`cleanup::tests::another_workspaces_source_tree_inside_the_cleaned_region_is_never_cleaned`
pins the final-preflight layer on its own, across all three modes, with the
region named `out` rather than `target` — discovery refuses to descend into a
`target` directory, so a fixture using the conventional name would never
discover the neighbour and would prove nothing.

`an_unrelated_sibling_workspace_stays_eligible_alongside_a_dangerous_one` exists
so the previous case cannot pass on a fixture where everything is refused for an
unrelated reason. `PrivateBounded` must remain reachable.

## 6. Premise-negative controls, run live

Every one of these was performed by mutating the source, observing the failure,
and restoring. These are the strongest artefacts in this record, because each
one demonstrates that the new tests would have rejected the behaviour they were
written for.

| Mutation | Observed |
|---|---|
| `invocation()` `Clean` arm reads only its own three flags, as 0.2.0 did | `every_accepted_simulation_spelling…` and `the_cargo_plugin_simulation_spelling…` **fail**; all 34 pre-existing `cli_contract` cases stay green |
| `build_groups` disjointness consults only the group's own owner, as 0.2.0 did | the classification-layer assertion **fails**, and `workspace::tests::another_workspaces_source_tree_inside_a_covering_root_is_never_private` **fails**; the preflight case correctly still passes |
| `domain::elapsed_nanos` truncating cast | `left: 18446744072709551616` against `right: 18446744073709551615` — a 584-year duration reads as `0` |
| `discovery_state` cursor revert | the identical learned root published twice |
| `output::log::config` / `update` leaking bodies | fails at three independent layers: exact shape, the shared ASCII/bounded helper, and the no-path assertion |
| `workspace::cargo_failure_reason` truncation removed | the bounding test fails |
| `active_skipped` per-group → per-workspace | the exact-count assertion fails |
| production walk descend → `entry_is_within_any` | `left: []` — nothing found under the exception root, the original defect |
| `progress::finish_and_clear` no-op / per-bar clear removed | liveness assertion fails where frame-emptiness does not |
| `update` staging-cleanup propagation removed | the run reports success; the test fails |
| `config::create_initial` fallback replaced with the pre-fix arm | publication fails with `Function not implemented` |
| `a_canonical_spelling_cannot_inject_glob_characters` raw splice | the named tree stops matching |

**The dry-run control is the one that matters most.** All 34 pre-existing
`cli_contract` cases passed while the defect was live. M012A had proved the
intended semantics; it had never asked whether every accepted spelling reached
them.

**The ownership control produced a finding worth keeping.** With the classifier
mutated, the *final preflight* still caught the region and the neighbour
survived. The two layers are genuinely independent, which is exactly what
`workspace.rs`'s disjointness invariant requires. It also means a classifier
regression would otherwise be masked behind a still-safe green, which is why
layer 1 is now asserted separately from layer 2.

## 7. Requalification of the commit under review (WP-E)

Every change in `eaa7bba` shipped without a test that could have rejected it.
An audit found nine uncovered claims and three partial. Each was closed with a
discriminating test, or recorded as a finding in §11.

Closed: `elapsed_nanos` saturation; duplicate learned-root folding;
`--format log` for `config` and `update`; bounded/char-boundary Cargo stderr on
both failure paths; `active_skipped` per-group; the production-walk descent
under a non-empty system-prune list; `finish_and_clear` leaving no live bar;
updater staging-cleanup propagation; first-use config publication where
`link(2)` is `ENOSYS`.

Two of those deserve comment:

**The overflow fixture I first specified was a false friend.**
`Duration::new(u64::MAX, 999_999_999)` is exactly `2^64·10^9 − 1`, so a
truncating cast returns `u64::MAX` for it too and the assertion passes under the
bug. The fixtures that actually discriminate are `Duration::new(u64::MAX, 0)`
and `Duration::new(1 << 63, 0)`, the latter reading as `0` under truncation.

**The config fallback needed a real trap.** No filesystem available here
produces `ErrorKind::Unsupported` from `hard_link` — vfat gives `EPERM`, squashfs
`EROFS`, a cross-device link `EXEXDEV`. The test installs a seccomp filter on a
dedicated thread that returns `ENOSYS` from `linkat(2)` and asserts the premise
on that thread. Filters are per-thread and irreversible, so it runs on a thread
that exits afterwards.

## 8. Two hosted-only defects found while requalifying

Neither was visible to local Linux verification.

**A Windows fixture that could not parse on Windows.**
`a_canonical_spelling_cannot_inject_glob_characters` wrote a raw `Path::display()`
into a TOML basic string. On Windows that contains backslashes, which TOML reads
as escape introducers, so the fixture failed to parse before the subject was
consulted. It was added by `eaa7bba` and had never executed on a Windows lane,
because macOS was red and Windows was being cancelled.

The same test had a **second, worse** Windows defect behind the first: its
`#[cfg(not(unix))]` branch bound the link to the *real* name, so the canonical
rewrite never ran and the test could only ever fail. It is now `#[cfg(unix)]`
with the reason recorded. Repairing the premise makes the Unix case honest; it
gives Windows no coverage, because there is no Windows fixture to write until
§11.1 is fixed.

**Dead code on the lane that could not use it.**
Hosted run 37498646836 failed Windows with `function toml_string is never used`
and `method failing_cleanup is never used`. Both helpers are reached only from
`#[cfg(unix)]` tests, so on Windows they are dead code and CI compiles with
`-D warnings`. Gated to match their callers.

This is C021's lesson arriving through a different door: a helper added for a
Unix-only test is not automatically Unix-only, and Linux cannot see it because
on Linux both helpers *are* used.

## 9. What was NOT done, and why

- **0.2.0's GitHub release was not rewritten.** Releases are immutable; the
  remedy is a new version. The assets and tag are untouched.
- **No new discovery root was introduced.** C023 did not need one and adding a
  scanning root would be a scope change on a safety patch.
- **The cleanup JSON/log schema was not changed.** A source-overlap group is
  counted, not rendered, in the cleanup envelope; `scan` states the reason in
  full. §17 puts schema changes out of scope.
- **The `--dry-run` flag was not made `global = true`.** It would collide with
  `update --dry-run`'s own argument id. Reconciliation in `invocation()` is the
  chosen mechanism and is now covered on both arms.

## 10. Repositories hygiene found on the way

Two `.pyc` artifacts were tracked and `__pycache__` was not ignored. Any
interpreter that differs from the one that produced them rewrites the file,
which dirties the tree — and `scripts/release-check.sh` refuses to run on a dirty
tree. A latent release blocker, now untracked and ignored.

## 11. Findings recorded rather than fixed

These are C023's, and they are why this record is not a list of wins.

### 11.1 Windows glob canonicalization is a no-op — **C024 opened**

`escape_glob_literal` returns `None` for any string containing `\`. Every
canonical Windows path contains one, so `canonical_pattern_prefix` returns the
user's pattern unmodified and a bracketed directory name reaches `globset`
unescaped. An `exclude` naming `real[abc]` becomes a character class, matches
something else, and does not protect the tree the user named.

It is not a regression — this has always been true on Windows — and it is not
destructive: the failure direction is *under*-matching, so the directory is
cleaned as though no `exclude` had been written, which is default behaviour.
The fix requires deciding how a canonical Windows path is spelled inside a
glob pattern, which changes what every existing Windows `include`/`exclude`
means. §17 puts matching-semantics changes out of scope for a patch release.

**[c024-windows-glob-canonicalization-corrective.md](../../implementation/distribution-release-update/c024-windows-glob-canonicalization-corrective.md)**
owns it.

### 11.2 The post-rewrite glob re-validation loop is unreachable

The second validation loop in `config.rs` shares its error message with the
first, and the only test asserting that message writes an invalid glob *in the
file*, so it fails early and never reaches the new loop. Deleting the loop
leaves it green.

It turns out the loop **cannot** fire: the only splice that feeds it either
produces a valid `globset` class or returns `None`, and 5,850,050 generated
(canonical, separator, tail) combinations produced zero uncompilable patterns.
It is defence in depth. No test can reach it, and a test that cannot fail is
decoration, so none was written. The proof is recorded in the plan that would
change this code.

### 11.3 `update::HttpEnvironment::staging_dir` exclusivity is untested

Production uses `create_dir`, which is exclusive. The test fixture mirrored that
but derives its name differently, and the two cannot be made to collide: the
name is `{product}-update-{pid}-{wall-clock nanos}`, the measured gap between
the test's clock read and production's is ~40 ns at p50, and one `mkdir` costs
~48 µs. Pre-creating a dense window is arithmetically impossible — placing N
consecutive nanosecond names costs N × 48 µs, far wider than the 40 ns window.

The property is pinned by the fixture's mirrored `create_dir` and a comment. It
is **not** tested against production. Closing it needs a seam, which is a
production change and therefore its own decision.

### 11.4 The `main.rs` progress-clear ordering is untested

`renderer.finish_and_clear()` now runs before the fallible cleanup call. The
ordering lives in `main.rs`, which has zero tests, and it is unreachable from
`src/progress.rs`: reverting it leaves every test green, because the bar only
draws on an attended terminal and the failing-cleanup contract test passes
`--no-progress`. Pinning it needs a pty harness and a new dev-dependency.

`src/progress.rs` does now test that `finish_and_clear` leaves **no live bar**
for a later error — a signal that separates cleared from live, where
frame-emptiness does not. The ordering is not tested.

### 11.5 The config fallback's partial-file removal is untested

The `create_new` write happens before `hard_link`, with the same calls, so a
seccomp trap on `write`/`fsync` kills the primary path first. The claim that a
failed staging write removes its own partial file is untested.

## 12. Publication evidence

Published **2026-10-06**. Release commit `5fc518d3685b`, tag `v0.2.1`.

### 12.1 Qualification before tagging

| Gate | Evidence |
|---|---|
| Local release gate | `scripts/release-check.sh` — `release-check: passed; no publication was performed`. Every static guard ran its own `--self-test` first. |
| Hosted CI, all three lanes observed | run **37504097504**, `checks (ubuntu/macos/windows-latest)` + msrv, benchmark, generated-docs, and all three installer lanes green. **No lane cancelled.** |
| Release drift guard | run **37504097326**, success. |
| Cargo selector qualification | run **37505094227** on the tag, success. |
| Release identity | `check-release-identity.py --tag v0.2.1` → `v0.2.1 names 0.2.1 at 5fc518d3685b`. |

### 12.2 Publication gate, in the required order

1. **Eggpack candidate build/stage green** — run **37505107200**, all 17 jobs
   green across the five contracted targets. Draft staged as release
   `405000990`; nothing published.
2. **Staged-release validation green for the same tag and source SHA** — run
   **37507507170**, triggered automatically by `workflow_run`. Its retained
   report (`staged-validation.log`) records: tag `v0.2.1`; source revision
   `5fc518d3685b`; upstream run `37505107200`; 15 assets = 5 binaries + 5
   sidecars + manifest + 4 installers; sidecar integrity ok for all 5 binaries;
   `release-manifest.json` agreeing with the bytes; contract agreement for all
   five triples; static ABI floor `GLIBC_2.17.0` on both Linux targets; and a
   **real installer qualification on the validator host** which installed the
   published asset, saw `cargo-cleanme 0.2.1`, and confirmed the installed bytes
   match the release digest exactly. `PASSED: the staged draft matches the
   release contract`.
3. **Human inspection** — performed on that log before publication. Inventory
   count, per-asset digests, the tag/source binding, the measured glibc floor
   and the installer result were each read, not assumed.
4. **Then the draft was published** — `2026-10-06T17:58:51Z`.

Minor observation, not release-blocking and not corrected here: the validator's
summary line says "2 installers" while listing four (`install.sh`,
`install-exact.sh`, `install.ps1`, `install-exact.ps1`). The inventory itself is
correct and complete; the arithmetic in the prose is wrong. Recorded so the next
reader does not treat the line as an inventory of two.

### 12.3 crates.io

Published from a **detached worktree of the exact tag**, per `docs/RELEASING.md`
— not from `main`. `cargo publish --locked` at `5fc518d3685b`, accepted
`2026-10-06T17:59:29Z`. crates.io now reports `max_stable_version: 0.2.1`.

### 12.4 Immutability and attestation

```
$ python3 scripts/verify-release-attestation.py --tag v0.2.1
verify-release-attestation: v0.2.1 is immutable and carries a valid release attestation
  attested release subject: pkg:github/dbowm91/cargo-cleanme@v0.2.1 (15 asset digest(s))
```

### 12.5 The published binary, re-run against the safety fixtures

Both §2 fixtures were re-run against the **published release asset**, not a
local build. `cargo-cleanme-x86_64-unknown-linux-gnu` sha256
`09660ecbccb3dacd2cc67af5c3f45f7a1dc529cbad8f40821e8492ee23da825f`, verified
against its own `.sha256` sidecar before execution.

| Fixture | Published 0.2.0 | Published 0.2.1 |
|---|---|---|
| A1 `--dry-run clean ROOT` | 1 `cargo clean`, 65,580 bytes destroyed, exit 0 | **0 `cargo clean`**, `Simulated … no cargo clean command was invoked`, target bytes 65580 → 65580 |
| A2 neighbour sources | 1 `cargo clean`, 4 source files deleted, exit 0 | **0 `cargo clean`**, all four files byte-identical |

### 12.6 Post-release smoke — and what this evidence is not

This is the one place where the record has to be precise rather than
flattering, because two runs exist and they do not say the same thing.

**Run 37507738147 — the automatic `release: published` trigger. It fired, and it
FAILED on all five lanes**, with:

```
error: cannot install package `cargo-cleanme`, it has been yanked from registry `crates-io`
```

The automatic trigger is itself now demonstrated: it fired without anyone
remembering to run it, which is the property M011C was written for. But it
produced no green evidence, and the reason is C023's own doing.

**The cause was a direct collision between two of this plan's own
requirements.** WP-L requires yanking 0.2.0; WP-J requires a green five-lane
smoke; and the smoke's `select_predecessor` took the greatest stable release
below the target **from GitHub**, never asking crates.io whether that version
could be installed. Yanking 0.2.0 — the correct safety decision — made the
rehearsal source uninstallable, on every lane, before the updater it exists to
test ran a line. The updater was fine. The harness was lying about what it could
do.

**The fix was to the harness, not to the yank.** `scripts/resolve-smoke-transition.py`
now reads crates.io's yanked set and selects the greatest *installable* release
below the target, which is what WP-L's own resolution asks for: *"the updater
rehearsal must be defined against the new release."* Deliberately **not** done:
un-yanking 0.2.0 to make a harness reach it. That would have made a
known-destructive release installable again for anyone pinning it explicitly,
purely so a test could touch it.

**Run 37508262225 — the green evidence, from the manual rehearsal surface.**
Dispatched with `from_version=0.1.6`, `to_version=0.2.1`:

| Lane | Transition | Result |
|---|---|---|
| `x86_64-unknown-linux-gnu` | `v0.1.6` → `v0.2.1` | success |
| `aarch64-unknown-linux-gnu` | `v0.1.6` → `v0.2.1` | success |
| `x86_64-apple-darwin` | `v0.1.6` → `v0.2.1` | success |
| `aarch64-apple-darwin` | `v0.1.6` → `v0.2.1` | success |
| `x86_64-pc-windows-msvc` | `v0.1.6` → `v0.2.1` | success |

All five lanes performed the **real self-update transaction** against published
bytes on all five contracted targets.

**So, stated plainly: the automatic trigger fired and failed; the fixed
resolver has green five-lane evidence from the manual rehearsal surface; there
was no observed-green *automatic* run for v0.2.1, and there cannot be one now,
because `release: published` fires once per release and 0.2.1 has been
published.** The next release gets a genuinely automatic green run, from the
fixed resolver, with no action required.

This is recorded as the same kind of conditional closure M011C carried, and the
registry says so in the same words. It is not recorded as a clean pass.

### 12.7 A guard that caught me, in the one place I nearly did not look

Run 37508164968 — the first dispatch of the fixed rehearsal — failed in its
*first* step, because the resolver's own self-test did not pass. Two
pre-existing cases assert the fragment `no stable cargo-cleanme release below`,
and correcting the message to `no installable stable cargo-cleanme release
below` broke both.

I had run that self-test locally and read it with `tail -10`. Both failures were
above the fold; the new cases I had just added were below it, so the output I
looked at was entirely green. CI failed four minutes later.

That is the third time in this corrective that a partial read produced a wrong
conclusion, and the reason every verification claim in this record is an exit
code or a named run id rather than "it printed ok". The guard worked. The
reading did not.

### 12.8 crates.io 0.2.0 yank

Yanked at **2026-10-06T16:21:52Z**, before 0.2.1 existed, which is WP-L's
preferred ordering: stop new default installs from selecting a release that can
delete a neighbour's sources. Verified immediately against the registry API
(`yanked=True`) and again after publication, with `max_stable_version: 0.2.1`
and `0.2.1 yanked=False`.

Disclosed in `CHANGELOG.md` (Security section), `README.md` (install block),
`docs/INSTALLING.md`, and `docs/TROUBLESHOOTING.md` — including why this yank
differs from the 0.1.x ones, and what a user who ran 0.2.0 should check.

**Not retracted, not re-published.** The GitHub release for v0.2.0 is untouched
and remains immutable; its assets and tag were never replaced.

## 13. Reconciliation

- `plans/registry.md` — C023 closed; the two artifact-roadmap rows move off
  "corrective required"; M013 marked corrected-by-C023 rather than reopened;
  M011C upgraded to a confirmed-green automatic trigger.
- `plans/closure/distribution-release-update/013-status.md` — an **appended**
  corrective note. Not reopened, nothing retracted.
- `plans/subsystems/artifact-discovery-cleanup-roadmap.md` and
  `plans/subsystems/distribution-release-update-roadmap.md` — C023 closed, C024
  opened.
- `plans/002-long-term-roadmap.md` — Phase 13 corrected by C023.

## 14. Acceptance criteria

| # | Criterion | Result | Evidence |
|---|---|---|---|
| 1 | Both defects reproduced against the published binary in isolated fixtures | met | §2, A1 and A2 |
| 2 | The fixed candidate makes A1 non-mutating and A2 survive | met | §2, comparison table |
| 3 | macOS hosted CI compiles and passes | met | §4, run 37498861638 |
| 4 | Windows and Linux lanes observed green, not cancelled | met | §12 |
| 5 | Cargo registry and GitHub release drift and selector gates green | met | §12 |
| 6 | Release metadata mutually consistent at the published commit | met | §12 |
| 7 | Complete local release gate green on a clean tree | met | §12 |
| 8 | Dry-run evidence across every accepted spelling, asserting zero spawned cleans | met | §5 |
| 9 | Cross-workspace source-containment end-to-end fixture, with negative controls | met | §5, §6 |
| 10 | Every other shipped change qualified by a discriminating test | met | §7, §8 |
| 11 | A green automatic `release: published` five-lane smoke | **not met** | §12.6 |
| 12 | Published 0.2.1 binary passes both safety fixtures | met | §12 |
| 13 | crates.io 0.2.0 yanked and disclosed | met | §12 |
| 14 | No unresolved medium or high finding invalidates the release | met | §11.1 is medium and non-destructive; it is recorded, assigned to C024, and is not a precondition of this patch |
| 15 | Reporting distinguishes a skipped smoke from a passing one | met | §12.6 states plainly that the automatic run fired and failed, and names the manual run that is green |

## 15. Disposition

**Closed, with acceptance criterion 11 explicitly not met.**

Both destructive defects shipped in 0.2.0 are fixed, qualified by tests that
were shown to fail against the old behaviour, and published as 0.2.1 with
crates.io 0.2.0 yanked and disclosed in `CHANGELOG.md`, `README.md`,
`docs/INSTALLING.md`, and `docs/TROUBLESHOOTING.md`. The **published** 0.2.1
binary passes both safety fixtures.

Criterion 11 is not met, and the reason is this plan's own doing rather than an
oversight: WP-L required the yank and WP-J required the smoke, the smoke chose
its rehearsal source from GitHub without asking crates.io whether it could be
installed, and the yank made that source uninstallable. The automatic trigger
fired and failed on all five lanes. The resolver is fixed and self-tested; the
green five-lane evidence is from the manual rehearsal surface; and no green
automatic run for v0.2.1 can exist, because the event fires once per release.

This record therefore claims **conditional closure**, exactly as M011C's did,
and does not claim a clean pass. The next release produces a genuinely automatic
green run from the fixed resolver with no action required.

Two findings are handed on rather than absorbed:

- **11.1 → C024.** Windows glob canonicalization is a no-op. Not a regression,
  not destructive, and a matching-semantics decision rather than a safety patch.
- **The smoke's dependence on an installable predecessor is fixed**, but the
  deeper question — whether a release-evidence path should be able to depend on
  a version the same corrective decided to yank — belongs with whoever changes
  the rehearsal policy next.