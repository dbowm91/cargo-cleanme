# C017 — Cargo-Managed Provenance Was Misdetected, and One Path Silently Took Ownership of a File Cargo Owns

Status: ready

Discovered by: `plans/implementation/distribution-release-update/c014-v0.1.2-live-update-and-release-reproducibility-corrective.md`
(work package F, the `v0.1.3` -> `v0.1.4` rehearsal, cargo-managed scenario)

Partial fix applied: see §7. A further release is required to close.

Related: C010c (which introduced install provenance), C016 (the identity-invocation
defect, shipped in 0.1.3), C015 (the relative-root defect), C013 (the fixture-premise
audit that missed this).

Source roadmap: post-Phase-10 distribution/release/update corrective line

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: corrective / production defect / safety

## 1. Objective

Make Cargo-managed installations actually be *detected* as Cargo-managed, so the
provenance model in `src/update.rs` holds on real Cargo layouts instead of only on
the one layout the tests invented.

## 2. Evidence

The `0.1.3` -> `0.1.4` rehearsal drove the self-managed scenario to success and then
reached the cargo-managed scenario, which reported a **silent no-op**. Re-running that
scenario by hand, from a clean state, showed the no-op was not the defect. The defect
is the opposite: the tool **took ownership of a file it does not own and replaced it.**

### Case A — `cargo install --root` (the mutating defect)

~~~text
$ export CARGO_HOME=/tmp/c017-repro/cargo-home
$ cargo install cargo-cleanme --version 0.1.3 --locked --root /tmp/c017-repro/root
   Installed package `cargo-cleanme v0.1.3` (executable `cargo-cleanme`)

$ /tmp/c017-repro/root/bin/cargo-cleanme update --no-progress
cargo-cleanme: updated to 0.1.4
exit=0
~~~

The binary Cargo installed was overwritten with the release asset, by the tool
itself, with no refusal and no warning:

~~~text
binary now reports:        cargo-cleanme 0.1.4
cargo still records:       "cargo-cleanme 0.1.3 (registry+...)" = ["cargo-cleanme"]
cargo install --list:      cargo-cleanme v0.1.3
post-update sha256:        fb8608a30f2b259e0d7d2e9e277b8a4d5eba3f96703919c3a2e2b888c5470fdc
published v0.1.4 asset:    fb8608a30f2b259e0d7d2e9e277b8a4d5eba3f96703919c3a2e2b888c5470fdc
~~~

Cargo's bookkeeping is now false in both directions. `cargo install --list` reports
`v0.1.3` for a file that is `v0.1.4`, so `cargo upgrade` will not offer the right
target and `cargo uninstall` is operating on a package whose contents it does not
know. The whole point of `Provenance::CargoManaged` — "Cargo owns this file and its
bookkeeping; replacing it here would leave `cargo install --list` and uninstall
lying" — is exactly what happened.

### Case B — path matches, format does not (the parser defect)

With the binary in `$CARGO_HOME/bin`, the location test passes and the *parse* fails:

~~~text
$ export CARGO_HOME=/tmp/c017-repro/cargo-home2
$ cargo install cargo-cleanme --version 0.1.3 --locked --root "$CARGO_HOME"
$ "$CARGO_HOME/bin/cargo-cleanme" update --no-progress
cargo-cleanme: self-update refused: cargo-cleanme is unprovable_ownership; run this instead:
  Reinstall with the published installer, then retry: https://github.com/dbowm91/cargo-cleanme
exit=2
~~~

This one refuses, so the bytes are safe. But the remediation is wrong: the correct
answer for a Cargo-installed binary is `cargo install cargo-cleanme --locked --force`,
not "reinstall with the published installer". The user is sent to replace a working
Cargo installation with a self-managed one, which is the provenance confusion this
subsystem exists to prevent.

## 3. Why the tests never caught it

Two independent assumptions, both invented rather than observed.

**The location test assumed one layout.** `cargo_bin_root_containing` compares the
running executable's parent against `$CARGO_HOME/bin` and nothing else
(`src/update.rs:450-456`). `cargo install --root DIR` is a documented, ordinary
invocation that places the binary at `DIR/bin` and writes `DIR/.crates.toml`. No test
ever used `--root`, because every test constructed `$CARGO_HOME/bin` by hand and
passed it in as the second argument of `classify_provenance_in` — the seam the test
needed is the seam that removed the only case that fails.

**The parser assumed a format Cargo stopped writing years ago.**
`cargo_recorded_version` reads `value.get("packages")` and then `.get("vers")`
(`src/update.rs:459-467`). No cargo in current use writes that. What it writes,
confirmed on this machine with cargo 1.99.0, is:

~~~toml
[v1]
"cargo-cleanme 0.1.3 (registry+https://github.com/rust-lang/crates.io-index)" = ["cargo-cleanme"]
~~~

and the test at `src/update.rs:1294` hand-writes exactly the obsolete shape:

~~~rust
std::fs::write(
    cargo_home.join(".crates.toml"),
    "[packages.cargo-cleanme]\nvers = \"0.1.0\"\n",
)
~~~

That test therefore asserts that the parser handles the parser's own assumption. It
passes, and would pass with the real format in place. This is the C009/C011/C012/C013/
C016 blind-fixture pattern, **fourth occurrence, same subsystem** — and the C013
cross-platform fixture audit reviewed this file and recorded the assertion as sound
without asking whether the fixture could fail for the right reason.

## 4. Severity: high, and the only mutating defect in this line so far

- Case A **replaces a binary owned by another package manager**, exits `0`, prints a
  success message, and leaves that manager's database inconsistent. Every other defect
  in this corrective line (C011, C016) failed loudly and mutated nothing. This one
  succeeds at destroying the provenance invariant.
- The damage is bounded to the single binary and is recoverable by `cargo install`
  again, so it is not critical.
- It is **silent**: exit `0` and a normal-looking success line. A user has no signal
  that ownership changed hands.
- Case B is safe but wrong, and pushes users toward the confusion it should prevent.

The severity is set by Case A. The published `0.1.1` through `0.1.4` binaries all
contain it.

## 5. Root cause

Two assumptions in `src/update.rs`, both untested against reality:

1. `cargo_bin_root_containing` enumerates exactly one Cargo bin root, the process
   `CARGO_HOME`. Cargo supports arbitrary roots via `--root`, and those are the roots
   used by every hermetic install script, CI job, and container image.
2. `cargo_recorded_version` parses a `.crates.toml` schema that current cargo does not
   emit. Because it always returns `None`, the location branch could not have
   distinguished a real Cargo root from a foreign directory even in Case B — the
   refusal it produced was an accident of the two bugs, not a decision.

The direction of the first bug is the dangerous one. Detection is a *negative* test:
a false negative hands a Cargo-owned file to the self-managed path, and the
self-managed path is the one that writes. The fix must therefore be biased toward
over-detection, never under-detection.

## 6. Required work

1. Parse the real `[v1]` schema: keys are `"<name> <version> (<source>)"`, values are
   arrays of installed binary names. Match on package name and confirm the binary is
   actually recorded in that entry's value list.
2. Detect Cargo roots by bounded ancestor walk from the running executable, checking
   each ancestor for a `.crates.toml` that records `cargo-cleanme`. Keep the explicit
   `CARGO_HOME` check first, since it is authoritative when set.
3. Keep the existing conservative fallback: no record found anywhere means
   `VerifiableSelfManaged` only after the digest is positively revalidated under the
   mutation lock, which is unchanged.
4. Replace the hand-written `.crates.toml` fixture with one generated in the format
   cargo actually writes, and add a case for a `--root` install whose bin root is not
   `CARGO_HOME`.
5. Premise-negative evidence for both guards.

## 7. Applied in this working tree

`src/update.rs`:

1. `cargo_recorded_version` now parses the real `[v1]` schema and requires the
   record's value list to name the running binary, so a record is a statement
   about *this file* rather than about a package that shares a root. A malformed
   entry is skipped rather than fatal, because `.crates.toml` is Cargo's file and
   one unparseable key must not hide the entries after it.
2. `cargo_bin_roots_above` walks a bounded number of ancestor directories, which
   is what finds `cargo install --root DIR`. The explicit `CARGO_HOME` check
   still runs first and still refuses on an unrecorded file, so the pre-existing
   safety property is unchanged.
3. The two tests that hand-wrote `[packages.<name>] vers = ...` now build a
   record in the real schema, via a `real_crates_toml` helper pinned against
   captured cargo 1.99.0 output.

Gated locally:

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features      # 235 + 8 + 1
cargo +1.89 check --locked --all-targets
cargo +1.89 test  --locked --all-targets     # 235 + 8 + 1
python3 scripts/check-fixture-portability.py # 30 files
python3 scripts/check-release-contract.py (+ --self-test)
python3 scripts/check-release-identity.py --self-test
python3 scripts/check-post-release-smoke-contract.py (+ --self-test)
python3 packaging/tests/test_installers.py (+ --self-test)   # 19 passed
~~~

### 7.1 Premise-negative evidence

Each guard was broken on purpose, one at a time, and the suite re-run. A guard
that cannot fail is not a guard:

~~~text
A. schema fix reverted (parse obsolete [packages]/vers)   26 passed; 4 failed
B. ancestor walk reverted (only $CARGO_HOME/bin)          27 passed; 3 failed
C. binary-identity requirement dropped                    29 passed; 1 failed
D. package-name filter dropped                            29 passed; 1 failed
E. malformed-entry resilience removed (? aborts scan)     29 passed; 1 failed
~~~

Each break turned red a test that names the specific guarantee. Two of these
were caught only by running the harness rather than trusting the assertions:

- **The over-detection test had no teeth.** A first draft planted a record for an
  unrelated package (`ripgrep`) and asserted the binary stayed self-managed. Both
  the correct code and the broken code return self-managed there, so the test could
  never fail. It now plants the one near miss that does discriminate — a record
  for a *different package* that installs a binary named `cargo-cleanme` — which
  is what makes break D go red.
- **The malformed-entry test had no teeth, in the very shape this subsystem has
  already paid for five times.** The bad key was written *above* the `[v1]`
  header, making it a root-level key the parser never reads; the test passed
  against code that aborted the scan. It is now spliced explicitly inside the
  table, with an assertion that the fixture really has that shape.

### 7.2 Live verification against real Cargo output

The fixed binary, placed into a real `cargo install --root` layout with a real
record, and given an unrelated `CARGO_HOME`:

~~~text
before fix:  exit=0  "cargo-cleanme: updated to 0.1.4"   bytes: CHANGED
             (Cargo's record still said 0.1.3; the file was the v0.1.4 asset)
after fix:   exit=2  bytes unchanged
~~~

That is the defect itself: the mutating wrong-ownership overwrite is gone.

### 7.3 Known limitation: provenance is not surfaced when there is nothing to do

The `Provenance` refusal is raised at `src/update.rs:834-839`, *after* the
already-current check at `789-795`. A Cargo-managed binary that is already at the
published version therefore reports "already at the latest stable version" and is
never told it is Cargo-owned. This is safe — nothing is acquired and nothing is
written — and it is left as it is, because reordering it would change the meaning
of a correct no-op message for reasons unrelated to this defect.

It does constrain the live proof: the rehearsal's cargo-managed scenario only
observes the *refusal and its remediation* when the installed version is behind
the published one, so the proving rehearsal must run `--from <fixed> --to <newer>`.

## 8. What is still required to close

The live rehearsal cannot pass until a *published* release carries the fix, and
per §7.3 the installing binary must also be behind the published target. The
cargo-managed scenario installs the **from** version via Cargo, so:

1. publish `0.1.5` carrying this fix;
2. publish `0.1.6` as the newer target, exactly as `0.1.4` was the target that
   gave the fixed `0.1.3` updater something real to move to;
3. re-run `scripts/post-release-smoke.sh --from 0.1.5 --to 0.1.6` — this is the
   rehearsal that proves the cargo-managed refusal end to end;
4. dispatch the five-target post-release smoke workflow.


## 9. What must not be done

- Do not replace `v0.1.4` bytes. The fix ships as a new version.
- Do not yank `v0.1.1`..`v0.1.4`. Their scan, clean, config, and reporting behavior
  is correct and their self-managed path is now correct; annotate the defect.
- Do not close C014 on publication alone. C014's own failure semantics forbid it.
- Do not "fix" this under C016 or C014. It is a distinct product defect and gets its
  own plan, which is what this file is.

## 10. Verification commands

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 scripts/check-fixture-portability.py
bash scripts/release-check.sh v0.1.5
bash scripts/post-release-smoke.sh --target <host triple> --from 0.1.5 --to 0.1.6
~~~

## 11. Acceptance criteria

C017 closes only when:

- a published release's updater **refuses** a `cargo install --root` installation with
  unchanged bytes, and names `cargo install cargo-cleanme --locked --force`;
- the same release refuses a `$CARGO_HOME/bin` installation, not `unprovable_ownership`;
- both premise-negatives in the closure record are reproduced from the closing tree;
- hosted Linux/macOS/Windows CI is green on the release commit;
- `v0.1.1`..`v0.1.4` are recorded as immutable releases carrying this defect.

## 12. Closure evidence

Record the source revision, tag, both authorities' identifiers, the before/after
refusal output for both cases, both premise-negatives, the CI run ids, known
limitations, and the disposition of `v0.1.1`..`v0.1.4`.
