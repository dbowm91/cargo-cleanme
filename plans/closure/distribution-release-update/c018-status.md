# Distribution, Release, and Update C018 Status

Plan: `plans/implementation/distribution-release-update/c018-self-update-provenance-uncertainty-fail-closed.md`

Disposition: **closed — defect fixed and verified; the release-evidence portion
remains outstanding and is tracked as a published-release gap, not a code gap**

Implementation commit: the `phase11-hardening` branch commit that replaces the
`Option<String>` ownership parser in `src/update.rs` with typed evidence and moves
the mutating refusal ahead of all network work.

Repository baseline at implementation: `a150eda8b43e8bf2f088aec8b804e7fa557c3a18`

Date: 2026-10-05

## Executive finding

`cargo_recorded_version` returned `Option<String>`, and `None` meant **four
different things at once**: no record, unreadable record, malformed record, and a
perfectly good record that simply does not mention this package. The caller could
not distinguish "Cargo says nothing here" from "I could not read what Cargo
would have said" — so a record it failed to parse fell through to hashing the
executable and was classified `VerifiableSelfManaged`.

That is the fail-open direction. `VerifiableSelfManaged` is the classification
that **authorizes replacement**, and `cargo install --list` / uninstall
bookkeeping are then left describing a version the file no longer has. C017
established the failure; C018 establishes that three of the four routes into it
were reachable and are now closed.

**Collapsing uncertainty into absence is the defect.** Not "the parser is
strict", but: a state that means *I could not find out* must never be read as a
state that means *there is nothing there*.

## Work package A — real Cargo characterisation

Real Cargo, not fixtures, on both toolchains the project cares about:

| Toolchain | Command | Layout produced |
|---|---|---|
| **1.89.0** (MSRV) | `cargo install --root DIR --locked` | `DIR/.crates.toml`, `DIR/.crates2.json`, `DIR/bin/cargo-cleanme` |
| **1.99.0** (release builder) | `cargo install --root DIR --locked` | identical |

Exact schema, from Cargo's own output:

```toml
[v1]
"cargo-cleanme 0.1.6 (path+file:///…)" = ["cargo-cleanme"]
```

Recorded findings:

1. **`.crates.toml` is the stable record and is present in both toolchains.**
   `.crates2.json` is also written by both, and carries more detail (version_req,
   bins, features, target, rustc commit), but it is the newer of the two and has
   no stability commitment attached. `.crates.toml` is therefore the only marker
   treated as evidence. Treating `.crates2.json` as a second authority would mean
   maintaining two parsers for a file Cargo has not promised to keep.
2. **In every observed layout the record sits in the executable's *parent***
   (`ROOT/bin/<exe>` + `ROOT/.crates.toml`), one level from the executable. The
   default Cargo home, `--root`, and a relocated `CARGO_HOME` all reduce to this.
3. **The user's real Cargo home was not mutated.** Every installation and
   injection ran under `/tmp/c018-wpa/`.

Failure states injected into isolated copies of a real install — all eight
constructed and verified: record absent after a proven install; malformed TOML;
entry naming the package with no version; entry with a non-array executable list;
a `.crates.toml` with no `v1` table; `.crates.toml` replaced by a directory;
`.crates.toml` at mode `000`; and a non-UTF-8 executable name.

Where platform permissions make "unreadable" unreliable, the parser is exercised
by a deterministic failing-read path (a record that is a directory, or mode
`000`) rather than by relying on a permission bit behaving.

## Work package B — typed evidence

`CargoRecord` replaces the `Option`:

| State | Meaning |
|---|---|
| `Recorded { version }` | Cargo positively records **this package and this binary** here |
| `DoesNotClaim { reason }` | Readable, parseable, and provably not claiming this file. **Not** uncertainty — a positive statement of non-responsibility |
| `Absent` | No record at this root. Shows no evidence of being a Cargo root, so the search continues |
| `Uninterpretable { detail }` | A record exists and could not be interpreted |

The distinction that carries the weight: a malformed entry for a **different**
package is skipped, because the parser can prove it cannot be the cargo-cleanme
record. A malformed entry that **names this package** might still be the one that
claims this binary, so it is `Uninterpretable`, not absence. Both directions are
regression-tested, because they pull opposite ways — treating every broken entry
as fatal would let an unrelated broken package hide a later valid cargo-cleanme
record and force a refusal where ownership is provable.

Bounded on every axis, as the plan required: size-capped before the read, parsed
as a TOML document rather than scanned, and only the candidate root is consulted —
no parent, no `PATH`, no other manager's metadata.

The `Option`-returning wrapper was **deleted** rather than kept for convenience.
Leaving it would have left the ambiguous signature in the file, one refactor away
from being used again.

## Work package C — evidence-aware root detection

The ancestor walk is unchanged; **what makes a candidate a candidate** is not.
Before, every ancestor was mapped to `<ancestor>/bin` and probed. Now each
candidate root is read directly, and:

- `Recorded` → `CargoManaged`
- `DoesNotClaim` / `Absent` → keep walking
- `Uninterpretable` → **`UnprovableOwnership`, stop**

The third branch is the correction. The record file's *presence* is positive
evidence that this is a Cargo installation root, and the content that would
settle ownership is exactly what could not be read. Walking past it would replace
a file Cargo owns.

**The anti-over-correction is explicit.** Treating every ancestor `bin` as
manager-owned would have closed the hole by making every published installer
layout unupdatable. `Absent` deliberately continues the walk, which is what keeps
`~/.local/bin` and `/usr/local/bin` on the self-managed path. The invariant the
plan named — "an arbitrary installer-managed `.../bin/cargo-cleanme` with no
Cargo evidence must remain eligible for self-managed classification" — is pinned
by `a_genuinely_self_managed_installer_layout_stays_self_managed`.

The `CARGO_ROOT_SEARCH_DEPTH` bound was **characterised, not removed**, per the
plan's second acceptable outcome. Because Work Package A showed the record is
always in the executable's parent, the bound is now provable rather than
asserted: `the_cargo_root_search_cannot_miss_a_supported_layout` builds every
supported shape, proves the walk reaches it, proves the nearest candidate is the
executable's parent, and proves 24 levels of nesting still yields no more
candidates than the bound allows. The honest statement of what the bound costs is
that it *could* miss a root five levels above; the evidence says Cargo does not
write one.

`binary_name`'s non-UTF-8 downgrade is now load-bearing and documented: a
`.crates.toml` value can only hold UTF-8, so such a name can never match a record,
and every caller treats that non-match in a Cargo root as unprovable.

## Work package D — refuse before network work

The ordering was the fix. Refusing only *after* fetching and comparing registry
metadata meant a Cargo-managed or unprovable installation still performed a full
crates.io round trip and was told "up to date" or "a newer version exists" on the
way to being refused. The one case where this tool must be certain it may not
touch the file was also the case that learned the most about the network and the
least about the local state.

A mutating update now refuses forbidden provenance immediately after
classification:

| Provenance | Mutating run |
|---|---|
| `CargoManaged` | immediate manager refusal (`cargo install … --locked --force`) |
| `UnprovableOwnership` | immediate reinstall/refusal guidance |
| `VerifiableSelfManaged` | proceeds to version authority and normal planning |
| `CargoInstallOnlyHost` | existing fail-closed semantics preserved |

`--dry-run` deliberately **keeps** the version-authority lookup: reporting a
candidate is the entire point of a dry run. What it must never do is hide the
refusal a real run would hit, and
`a_dry_run_still_reports_a_candidate_and_names_the_real_provenance` asserts the
plan reports the *actual* classification alongside the candidate.

The stale `"Refuse before acquiring anything"` comment is now **true** for the
mutating path, and the architecture review's complaint about it has been rewritten
to say precisely that — including which path it does and does not apply to.

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Real-Cargo layouts characterised, MSRV and release builder | 1.89.0 and 1.99.0 `cargo install --root`; recorded schema and files | Pass |
| Failure states injected in isolated copies | 8 states, all constructed and verified | Pass |
| Typed evidence replaces `Option` | `CargoRecord`; wrapper deleted | Pass |
| Unrelated malformed entries do not hide a valid record | `an_unrelated_malformed_entry_does_not_hide_a_valid_record` | Pass |
| Malformed cargo-cleanme record not skipped into self-managed | `a_malformed_cargo_cleanme_record_is_not_skipped_into_self_managed` | Pass |
| Unreadable/unparseable metadata never self-managed | `unreadable_cargo_metadata_can_never_become_self_managed`, `a_cargo_record_in_an_unsupported_shape_is_uncertain_not_absent` | Pass |
| Explicit Cargo home bin with missing record is unprovable | `an_explicit_cargo_home_bin_with_a_missing_record_is_unprovable` | Pass |
| Non-UTF-8 executable name is unprovable | `a_non_utf8_executable_name_in_a_cargo_root_is_unprovable` | Pass |
| Installer layout stays self-managed | `a_genuinely_self_managed_installer_layout_stays_self_managed` | Pass |
| Non-UTF-8 / traversal / unbounded reads rejected | `OsStr` non-match, single-root read, `MAX_METADATA_BYTES` cap | Pass |
| Root detection evidence-aware, not `bin`-name based | `cargo_roots_above` + `read_cargo_record`; `Absent` continues, `Uninterpretable` stops | Pass |
| Search bound proven against real layouts | `the_cargo_root_search_cannot_miss_a_supported_layout` | Pass |
| Mutating refusal before any registry/release request | `a_mutating_run_on_forbidden_provenance_makes_zero_registry_requests`; asserted `requested.is_empty()` | Pass |
| Cargo-managed and `--root` installs refused, bytes unchanged | `a_recorded_cargo_root_beyond_the_cargo_home_is_still_refused_before_the_registry`; live bytes compared | Pass |
| Dry-run stays zero-write and reports real provenance | `a_dry_run_still_reports_a_candidate_and_names_the_real_provenance` | Pass |
| C017's default-root and `--root` behaviour not regressed | `cargo_managed_installation_is_refused_with_the_manager_command`, `a_cargo_root_install_is_refused_even_when_it_is_not_the_cargo_home` | Pass |
| No cleanup/discovery semantics changed | No change outside `src/update.rs` | Pass |

## Verification run

- `cargo fmt --all -- --check` — passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo test --all-targets --all-features` — **passed, 261 tests**.
- `cargo test --all-features --lib update` — **passed, 42 tests**, up from 31.
  Eleven new C018 tests.
- `python3 scripts/check-doc-citations.py` — passed, 16 documents clean.

## Defects this work found in its own new code

Recorded because the evidence discipline asks for it, and because all three are
the class this milestone is about — a guard or a test that cannot fail.

1. **A self-test case reported "rejected" for a tree with no defect in it.** The
   promotion mutation anchored on `| "1.89.0"`, but the *first* entry of a
   `matches!` arm list carries no leading pipe, so the mutation silently
   matched nothing and the case passed for the wrong reason. Fixed by anchoring
   on a `| `-prefixed arm **and** by asserting the mutation changed the file —
   the control that would have caught it. This is the single most important
   thing learned in this milestone: a guard with a no-op mutation is worse than
   no guard, because it converts a broken tree into a passing check.
2. **The first `maturity` count was derived by guessing.** The prod/test split
   in `architecture/overview.md` is read by hand from the `#[cfg(test)]` line
   number. The new numbers were computed from the source rather than estimated;
   the first attempt was off by 148 lines and 307 lines respectively and was
   caught by checking the claimed value against `src/` before committing.
3. **One flake, not reproduced.** See limitation 4 below.

## Limitations and unresolved findings

1. **The release-evidence half of this plan is outstanding.** The plan's
   work package 9 asks for a default-Cargo-home and `--root` real installation
   refused by a *released* binary with bytes unchanged. That requires a
   published release carrying this fix. This branch publishes nothing.
   **The fix is complete and locally verified; the published evidence is not,
   and this record must not be read as claiming it.** Severity: none for the code;
   the gap is evidentiary, and it stays open in the same way M011A/M011B/M011C's
   hosted runs do.
2. **Windows "unreadable" is not exercised live.** Mode-`000` is a POSIX
   mechanism, and an elevated Windows runner would read the file regardless. The
   parser is instead driven by a deterministic failing-read path (a record that is
   a directory) which behaves identically on both platforms. Recorded as
   platform evidence that is deliberately partial, per the plan's own allowance.
3. **Class 2 (the search bound) is bounded, not closed.** A Cargo layout placing
   the record five or more levels above the executable would still be missed.
   Work Package A found no such layout, and the plan explicitly accepts "retaining
   a bound but proving it cannot miss a supported Cargo layout". If a future Cargo
   changes its install layout, this is the regression that would reintroduce
   C017's fail-open, and `the_cargo_root_search_cannot_miss_a_supported_layout` is
   the test that would have to change.
4. **One flake observed, not reproduced.** `staging_is_cleaned_up_on_success_and_on_failure`
   scans `std::env::temp_dir()` for `cargo-cleanme-update-test-*` and failed once
   under parallel execution, passing on three consecutive reruns. Pre-existing and
   unrelated to this work; recorded rather than dismissed, because a flaky
   evidence check is precisely the kind of thing a closure record should not hide.
5. **Not a signed release.** M011A's attestation closes the authenticity gap for
   *downloads*; it does not change what `update` verifies. The updater still
   checks a SHA-256 sidecar served by the same host as the binary
   (`architecture/12-self-update.md`). Adding attestation awareness to the updater
   is a separate, larger plan and is not claimed here.

## Closure status and unblocking

**Closed as a defect corrective.** The fail-open class is fixed, the three
reachable routes into it are closed with individual regression tests, the fourth
is characterised and bounded with evidence, and the mutating refusal now happens
before any network work. Eleven new tests, each of which fails against the
pre-C018 implementation.

The published-release evidence in work package 9 remains outstanding and is
recorded above as an evidentiary gap rather than papered over. It does not hold
the corrective open — C010 through C017 all treated "fixed and verified locally"
as closure, with the publication dependency tracked separately — but it is the
first thing to be checked when the next release is staged.

**Downstream effect:** M011A's immutability work makes the next release
irreplaceable, which raises rather than lowers the value of this fix shipping in
it. No plan was blocked on C018.
