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

`cargo-cleanme` ships as a binary, four installer variants, a self-updater, five
shell completions, and man pages. The procedure is normative in
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
  them. `docs/TROUBLESHOOTING.md` states this limitation plainly; keep it.

## The gate, and its self-tests

`scripts/release-check.sh` runs: `cargo fmt --check`, clippy with
`-D warnings`, the full suite, doc tests, the Rust 1.89 check and test, the
semantic-counter benchmark, the completions/manpage drift gate, the derived
workflow-shape check, the Eggpack CI drift check, the release contract check,
the installer-contract check, the installer fixture suite, the fixture-portability
and architecture-citation guards, `cargo package --locked`, and
`cargo publish --locked --dry-run`.

Each static guard runs its own `--self-test` **first**. That ordering is the
point: a guard that has quietly stopped detecting its defect shape is worse than
no guard, so the gate proves each one can still fail before trusting it to pass.
Two of those guards exist because of a defect this repository shipped (see
[test-evidence](../test-evidence/SKILL.md)).

Two `cargo publish --locked --dry-run` warnings are **expected**: the staged
self-updater and the `release-check` script have dev-only dependencies. Do not
"fix" them.

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
- **The generated artifacts document `cargo cleanme`**, not `cleanme`, because
  that is the spelling that resolves after a registry install. Regenerate with
  `cargo run --quiet --features dev-tools --bin generate-docs`.

## Related

- [`json-and-exit-contract`](../json-and-exit-contract/SKILL.md) — the one
  untested output mode, and what to do about it
- [`planning-and-closure`](../planning-and-closure/SKILL.md) — why a fix is not
  a closure
