# C015 — Relative Scan Root Degrades Cargo Workspace Resolution

Status: ready

Repository baseline: `c9b0104c5c6480c4056be3cacd11b893b7d52642`

Discovered by: `plans/implementation/distribution-release-update/c013-cross-platform-fixture-premise-audit.md`

Closure evidence: `plans/closure/distribution-release-update/c013-status.md`

Related: C009, C011, C012 (the false-green fixture correctives that share the
same *evidence* defect class). C015 is different in kind: the evidence was
false, and what it was hiding is a real product defect.

Source roadmap: post-Phase-10 distribution/release/update corrective line

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Primary class: corrective / production defect

Hard dependencies: none. C013 must be closed (it is the finding source), but
C015 is independently implementable.

Downstream dependency: none blocks on C015. v0.1.2 under C014 may proceed; the
defect is not a release blocker because it fails toward *less* reported
reclaimable space, never toward deleting something unmeasured.

## 1. Objective

Make an explicitly relative scan root produce the same result as the absolute
spelling of the same directory.

Today `cargo-cleanme scan fixture` silently resolves **zero** Cargo
workspaces. The scan still exits 0 and still prints a valid, single, versioned
JSON document, so every consumer — a human, a script, or CI — reads a
well-formed report of nothing.

## 2. Evidence

Discovered while auditing the release-candidate smoke validator that C009
introduced. The validator passed on every hosted lane while the subject under
test reported a Cargo failure; C013 tightened the validator and exposed this.

Measured on `x86_64-unknown-linux-gnu`, cargo-cleanme 0.1.1, one inactive
fixture package with a 4096-byte `target/debug/artifact.bin`:

| Root spelling | Exit | Workspaces | Groups | Bytes | Diagnostics |
|---|---|---|---|---|---|
| `/tmp/…/fixture` (absolute) | 0 | 1 | 1 | 4096 | none |
| `fixture` (relative) | 0 | 0 | 0 | 0 | `cargo locate-project failed` |

Scan counters for the relative run: `manifests=1 workspaces=0 locate=1
metadata=0 failures=1 bytes=0 reportable=0`.

The scan is therefore *silent about the difference* in its exit status and its
JSON document, and only a diagnostic line on stderr distinguishes the two.

## 3. Mechanism

1. `discovery::discover_manifests_root` seeds the walker with
   `walker_root = root.to_path_buf()` (`src/discovery.rs:727`), preserving the
   caller's spelling. Every manifest discovered beneath it inherits that
   relative prefix, so the fixture yields `fixture/Cargo.toml`, not
   `/tmp/…/fixture/Cargo.toml`.
2. `workspace::resolve_workspaces_with_coverage` builds
   `["locate-project", "--workspace", "--manifest-path", <manifest>]` and runs
   it with `cwd = manifest.parent()` (`src/workspace.rs:317-325`).
3. The child therefore receives `cwd = <root>/fixture` and
   `--manifest-path fixture/Cargo.toml`, which resolves to
   `<root>/fixture/fixture/Cargo.toml`. That path does not exist, so Cargo
   exits non-zero with `manifest path 'fixture/Cargo.toml' does not exist`.

Independently confirmed outside the product:

~~~text
$ cd /tmp/…/fixture
$ cargo locate-project --workspace --manifest-path fixture/Cargo.toml
error: manifest path `fixture/Cargo.toml` does not exist
$ cargo locate-project --workspace --manifest-path /tmp/…/fixture/Cargo.toml
{"root":"/tmp/…/fixture/Cargo.toml"}
~~~

The two spellings are only equivalent when the manifest path is absolute, or
when the child's working directory is the *process* working directory rather
than the manifest's parent.

## 4. Severity: medium

- **No destructive behavior is introduced.** The failure direction is
  fail-safe: a degraded scan reports *fewer* groups and *fewer* bytes, so
  `clean` deletes less, never more. Nothing unmeasured is ever deleted.
- **A false all-clear is possible.** `cargo-cleanme clean ./some/dir --yes`
  reports nothing to clean. A user who reads that as "already clean" stops
  cleaning, which is the wrong conclusion in the direction that matters for a
  tool whose purpose is reclaiming space.
- **It is silent in the machine-readable channel.** Exit 0, valid JSON,
  `groups: []`. Only stderr distinguishes it.
- **It affects a documented invocation form.** A relative path is ordinary
  shell usage, and the no-argument form is a different scope (`routine`) that
  is unaffected; only an explicitly relative or dotted root is affected.

Not high: nothing is deleted that should not be, and no data is lost. Not low:
it is a real functional gap on a documented form, it produced a false release
qualification, and it was invisible to every existing check.

## 5. Objective constraints

- Preserve the cleanup safety boundary completely. A fix must not make any
  manifest *more* resolvable than Cargo can actually resolve.
- Do not resolve membership by path ancestry or hand-parsed TOML. Cargo stays
  authoritative.
- Do not change the meaning of `routine`/`global` scope discovery.
- Do not change the JSON output schema or add a field, unless the fix cannot be
  made without one. A schema change is a separate decision.

## 6. Candidate approaches

Implementation chooses, but the following are the shapes under consideration
and the constraints each must satisfy:

1. **Pass an absolute `--manifest-path`.** Reuse the existing
   `canonical_or_absolute` helper (`src/workspace.rs:151`) — it already
   canonicalizes where the path exists and refuses a relative path Cargo
   returned. Preserves the current per-manifest `cwd`, so Cargo's config
   discovery from the manifest's directory is unchanged.
2. **Run the child from the process working directory** and pass the manifest
   path as discovered. Smaller change, but it silently changes which
   `.cargo/config.toml` ancestry Cargo sees for relative manifests, so the
   configuration-compatibility consequence must be stated before choosing it.
3. **Absolutize the root once, at discovery.** Fixes every downstream consumer
   at the source, but changes the `display_path` strings the product reports,
   which is user-visible and may be load-bearing for report consumers.

Whatever is chosen, the relative and absolute spellings of one directory must
produce equal counters and equal `groups`/`inventory_bytes`.

## 7. Required tests

- A case asserting that a relative root and its absolute equivalent yield the
  same `discovered_manifests`, workspace count, group count, and
  `inventory_bytes`.
- The same case must use a real Cargo, not a stub, because the defect is in the
  argument composition the real Cargo receives. `SystemCargoRunner` is the
  subject; the assertion is on the observed counters.
- A case asserting the composed Cargo arguments for a relative manifest are
  resolvable — for example, that the runner receives a `--manifest-path` that
  exists when joined to the supplied working directory. This is the premise
  assertion C013 requires: it fails loudly if the argument composition regresses
  to a relative path under a parent directory.
- A regression test for the dotted root (`.` and `..`).
- Existing cleanup, selector, and JSON contract tests must remain unchanged in
  their expectations.

## 8. Verification commands

~~~text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
cargo +1.89 test --locked --all-targets
python3 packaging/tests/test_installers.py --self-test
python3 scripts/check-fixture-portability.py
python3 scripts/smoke-release-candidate.py target/debug/cargo-cleanme
bash scripts/release-check.sh
~~~

Hosted CI must be green on Linux, macOS, and Windows. The relative-root
regression case must be cross-platform: the composition defect is not
platform-specific.

## 9. Documentation updates

- `docs/TROUBLESHOOTING.md`: a scan reporting no groups while a directory holds
  build output, with the relative-root cause, if the fix does not make the case
  unreachable.
- `CHANGELOG.md` under the next patch release.

No README change: the documented behavior is unchanged once fixed.

## 10. Acceptance criteria

C015 closes only when:

- a relative root and its absolute equivalent produce equal counters, groups,
  and `inventory_bytes`;
- the regression cases fail against the pre-fix tree (premise-negative
  evidence recorded in the closure record);
- the cleanup safety boundary is unchanged, and no test that asserts a deletion
  boundary was weakened;
- hosted Linux/macOS/Windows CI is green;
- the release-candidate smoke validator's Cargo premise assertion still holds,
  so the class of defect that hid this one is still detectable.

## 11. Stop conditions

Stop and register a further corrective if:

- making the two spellings agree requires changing the JSON schema or the
  `display_path` contract;
- the fix cannot preserve Cargo as the sole authority for workspace membership;
- a fix would make a manifest resolvable that Cargo itself cannot resolve.

## 12. Closure evidence

Record:

- implementation commit;
- the before/after counter table from §2;
- premise-negative evidence that the new regression cases fail pre-fix;
- local verification commands and results;
- hosted run IDs;
- known limitations;
- unresolved findings by severity;
- disposition.

C015 is a product corrective. It does not reopen Phase 10 and does not
condition v0.1.2 publication.
