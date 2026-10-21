---
name: release-and-distribution
description: Releasing cargo-cleanme — the local gate, publication order, the updater's own limits, and the defect classes that only a live rehearsal can find
version: 1.0.0
tags:
  - release
  - distribution
  - updater
  - publication
---

# Release and Distribution

`cargo-cleanme` ships as a binary, four installer variants, a self-updater, **13
generated artifacts** (5 shell completions + 8 man pages), and a signed release
attestation. The procedure is normative in
[`docs/RELEASING.md`](../../docs/RELEASING.md); this is the reasoning behind the
parts that surprise people.

## When to Load

- Preparing or performing a release
- Changing the updater, an installer, or the Eggpack integration
- Anything touching `release/`, `packaging/`, or the release workflows
- A closure record that needs post-release evidence

## Publication order, and why it is that order

```
package --locked -> tag -> publish release -> crates.io publish -> identity self-test -> post-release smoke
```

**Tag before the GitHub release before crates.io.** A tag exists with no release
is recoverable; a crates.io version that exists with no matching tag is not. And
`cargo install --git --rev <tag>` must work for a version, so the tag has to
precede the published version, always.

**Every release is a qualification release.** No version is exempt from the gate
because it "only" carries a fix. M006C's fixes, C012's, and C015's all shipped
in releases that had to be rehearsed, and the rehearsal is what found the next
defect.

## The live rehearsal is the point

**The external smoke must be run against a staged draft *before* publishing**,
not only after. The registry's User-Agent policy is not something a fixture can
model, and v0.1.0 shipped a defect that no test could catch because every test
was a fixture.

Three defects were found this way and would not have been found otherwise:

| Defect | Found by |
|---|---|
| C016 — identity check invoked the candidate with no arguments, so its stdout could never match a version string, and it scanned the whole machine first | live self-update rehearsal |
| C017 — Cargo-managed provenance never consulted anything but `$CARGO_HOME/bin`, so a `cargo install --root` binary was replaced in place | live rehearsal of the previous release |
| v0.1.0 — external transport could not reach its version authority | external smoke |

Consequence for closure: **a corrective that is fixed but not yet carried by a
published release stays open.** C014 held open for exactly this reason. Closing
on publication would hide a real defect.

## The updater's own limits — know these before you promise anything

`update` is deliberately narrow, and every narrowness is a decision, not a gap:

- **It will not downgrade**, and it will not offer prereleases, because it
  cannot express "I accept that tradeoff" the way an explicit `cargo install` can.
- **It refuses a build from `main`** that is newer than the published release,
  rather than replacing it with an older one. The registry is the version
  authority for *releases*, not for something never published.
- **It refuses a file it cannot positively identify.** Never infers ownership
  from the file name, the directory, or `PATH` order. It now also parses the
  `.crates.toml` schema current Cargo actually writes, and walks up from the
  **executable's own path** to a bounded depth. Under-detection is the dangerous
  direction — a Cargo-owned file misread as self-managed gets *replaced*, while a
  false positive only costs a refusal that names the manager command — so the
  search is deliberately generous.
- **Integrity evidence is not authenticity.** The `.sha256` files prove the
  bytes were not corrupted in transit. They prove nothing about who published
  them. Scope the claim precisely, though: there is no *maintainer-key*
  signature and no transparency log, but the release **does** carry a GitHub
  release attestation, and `scripts/verify-release-attestation.py` verifies it
  (`scripts/release-check.sh:119`). What the **shipped updater** does not do is
  verify that attestation. "Everything cargo-cleanme publishes is unsigned" is
  over-broad; "there is no maintainer-key signature" is true.

## The gate, and its self-tests

`scripts/release-check.sh` is **24 steps** — from `cargo fmt --check` through
`cargo publish --locked --dry-run`, with no publication. Read the script for
the list; a restatement here would be a partial list, and a partial list of a
gate is the worst of both: it looks authoritative and is not.

It invokes **twelve** `--self-test` modes
(`scripts/release-check.sh:72, 79, 86, 90, 97, 103, 110, 112, 119, 126, 129,
133`). That ordering is the point: a guard that has quietly stopped detecting
its defect shape is worse than no guard, so the gate proves each one can still
fail before trusting it to pass. Two of those guards exist because of a defect
this repository shipped (see
[test-evidence](../test-evidence/SKILL.md)).

**But the inventory is not uniform, and the uniform claim is a trap.** Of the
**17** scripts in `scripts/`, **11** implement `--self-test`. Six do not:

- `release-check.sh` itself — it *invokes* twelve self-tests, and provides none
- `gen-release-workflow-shape.py`
- `release-benchmark.py`
- `smoke-release-candidate.py`
- `validate-staged-release.py`
- `post-release-smoke.sh`

`packaging/tests/test_installers.py` **does** have one, and it is the twelfth
invocation. An earlier version of this skill said "each static guard runs its
own `--self-test` first", which was false for six of them. If you close one of
the six gaps, close it with a self-test — a guard that stopped guarding is
worse than no guard.

Two `cargo publish --locked --dry-run` warnings are **expected**: the staged
self-updater and the `release-check` script have dev-only dependencies. Do not
"fix" them.

The gate also requires a **clean working tree** and the `eggpack` binary, and
toolchain **1.89** installed. It is therefore not ordinary verification — do not
run it as a routine check.

## Before you cut a release

- The Rust version is **pinned**, and the tag is bound to the exact source — not
  to whatever `main` is at tag time.
- A dry-run against a real registry API has happened; a fixture against a local
  server has not been mistaken for one.
- Post-release smoke proves the binary's bytes did **not** change in the
  download. A changed binary means the updater installed something else.
- You can state the exit-code and updater assertions from 0.1.5 onward, and know
  that through 0.1.4 they behave differently (see `docs/TROUBLESHOOTING.md`).
- The post-release record goes to `plans/closure/<subsystem>/<NNN>-status.md` and
  is referenced from `plans/registry.md`.

### The published-release count

**Eighteen** releases are published, `v0.1.0` through `v0.2.10`. **`v0.2.0` is
yanked**; the other seventeen are installable. The current source version is
`0.2.10`, **published**.

Two ways to get this wrong, both of which have shipped as bugs in this
repository's own docs: saying "seventeen releases are published" (that is the
*installable* count — 0.2.0 is published, just yanked) and saying "eighteen
are installable" (it is seventeen). The published count is always **one
higher** than the installable count, and the two errors above are what you get
when you swap them. State both numbers, or neither.

## Non-obvious traps

- **A Cargo-managed binary must never be self-updated.** C017 was exactly this,
  and the symptom was silent: `cargo install --list` kept reporting the old
  version for a file that no longer had it, so `cargo upgrade` and `cargo
  uninstall` stopped working.
- **Do not use a C017 victim to rehearse v0.1.5+.** It must rehearse a version
  that *can* update, from a *staged candidate that can be* `install.sh`-updated.
- **`install.ps1`'s `irm | iex` is correct.** It is idiomatic PowerShell. Do not
  "harden" it by downloading to a temp file unless you are prepared to argue the
  change with someone who ships Windows tooling.
- **`install.ps1`'s parameter is `-Directory`, not `-Dir`.** `-Dir` happens to
  bind by PowerShell prefix-matching, so it works by accident. Document the real
  name.
- **The generated artifacts document `cargo cleanme`**, not `cleanme`, because
  that is the spelling that resolves after a registry install. Regenerate with
  `cargo run --quiet --features dev-tools --bin generate-docs`.
- **`release/eggpack/distribution.toml` is the authority, but its tables are
  hand-copied** into `README.md` and `docs/INSTALLING.md`. When the matrix
  changes, all three move together.

## Related

- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md) — the
  `update_json` surface, which the updater's machine contract depends on
- [`planning-and-closure`](../planning-and-closure/SKILL.md) — why a fix is not
  a closure
- [`test-evidence`](../test-evidence/SKILL.md) — the `--self-test` discipline
  the release gate depends on
