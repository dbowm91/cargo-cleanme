# Self-update

```sh
cargo cleanme update --dry-run   # resolve and report; change nothing
cargo cleanme update             # update to the latest stable release
```

`update` is deliberately narrow. Every narrowness below is a decision, not a
gap, and the reasoning is given so you can tell a refusal from a bug.

## Contents

- [What it will do](#what-it-will-do)
- [What it refuses, and why](#what-it-refuses-and-why)
- [Known defects by version](#known-defects-by-version)
- [Integrity is not authenticity](#integrity-is-not-authenticity)
- [Binary size](#binary-size)
- [The `update --format json` exception](#the-update---format-json-exception)

## What it will do

1. Read the published **stable** version from the crates.io registry API.
2. Construct the GitHub release tag as `vX.Y.Z` from that value. The tag is
   never scraped from a page, a redirect, or a "latest" link.
3. Download the asset for this host and its `.sha256` sidecar.
4. Verify the sidecar against the asset, and confirm the sidecar is evidence
   for *that* asset.
5. Run the candidate and require it to print exactly `cargo-cleanme X.Y.Z`.
6. Observe the live file's exact SHA-256, take a mutation lock, re-verify under
   the lock, and only then replace the bytes.
7. Report the result, including when a commit was not clean and recovery is
   required.

Replacement, rollback, and staging are provided by the `eggup-eggfetch` adapter
rather than reimplemented here. The transport is that published adapter; exactly
one production transport is wired, and a run never silently switches to
another.

## What it refuses, and why

| Refusal | Reason |
|---|---|
| **A Cargo-managed install** | If the running executable lives in a Cargo bin root that Cargo's own `.crates.toml` records, `update` prints `cargo install cargo-cleanme --locked --force` and changes nothing. Cargo owns that file *and its bookkeeping*; replacing it here would make `cargo install --list` and uninstall lie. |
| **A downgrade** | You asked to be current. |
| **A prerelease** | `update` cannot express "I accept that tradeoff" the way an explicit `cargo install` can. |
| **A build from `main` or `--git`** | The registry is the version authority for *releases*, not for something never published. It refuses to replace it with an older build. |
| **A file it cannot positively identify** | The file name, the directory, and `PATH` order prove nothing. A file that cannot be positively identified fails closed. |
| **A host with no published binary** | Reported as such, with the Cargo command to use instead. |

Provenance classification is deliberately **generous toward detection**. Under-
detection is the dangerous direction — a Cargo-owned file misread as
self-managed gets *replaced* — while a false positive only costs a refusal that
names the manager command. The search walks up from the executable's own path to
a bounded depth, because container images and hermetic harnesses nest it deeper.

### The machine contract for a refusal

A refusal that stops the update exits **`2`**, and none of them replaces a
byte. In `--format log` it is one bounded line on stderr with a typed
`reason=` code and no prose:

```text
cargo-cleanme op=update status=error reason=provenance
```

`reason=provenance` is a **refusal** — the update was declined on purpose: a
Cargo-managed install, or a file that cannot be positively identified.
`reason=update` is a **failure** — the transaction could not be completed: a
transport error, a registry outage, a checksum that did not verify, a
prerelease-only registry, or a host with no published binary. The two are
different codes so a scheduler can tell "this will never work" from "this
might work later".

Two conditions are *not* refusals in that sense and exit **`0`** instead,
because nothing is wrong and nothing was changed: "already at the latest stable
version", and a build that is newer than the published release — a local or
`--git` build, which `update` declines to downgrade.

The other codes the CLI can put in the same position are `config`, `io`, and
`scan`; `update` itself produces only `provenance` and `update`. The line
carries a code and nothing else — no path, no URL, no prose — because it goes
into a retained history pane where a path is both noise and a small
disclosure.

## Known defects by version

Two defects in the `0.1.x` line affected the updater. Both were found by the
project's own release rehearsals, not by a test, and both are fixed. None of
these versions is yanked, because their other behavior is correct and yanking
would misdescribe them.

**0.2.0 is a different case and it *is* yanked.** It is not listed here because
these are updater defects; it is described in
[TROUBLESHOOTING.md](TROUBLESHOOTING.md#check-this-first-known-defects-by-version),
which also explains why yanking it is the honest description and retracting it
would not be. An updater running from a yanked version still works: the
registry entry remains installed and resolvable by exact version.

### If you are on 0.1.1 through 0.1.4: check whether `update` replaced a Cargo-managed binary

**This is the only defect in the `0.1.x` line that could change a file it did
not own.** A binary installed with `cargo install --root DIR` — the form every
hermetic install script, CI job, and container image uses — was not recognised
as Cargo-managed, because detection only ever looked at `$CARGO_HOME/bin`. The
file was treated as a self-managed installation and **overwritten in place**,
with no warning and exit status `0`. Afterwards `cargo install --list` reported
the old version for a file that no longer had it, so `cargo upgrade` and
`cargo uninstall` could no longer do their jobs.

Nothing else about those releases is affected: scanning, sizing, reporting, and
cleanup are correct. To check whether you were hit:

```sh
cargo cleanme --version                      # the version the file reports now
cargo install --list | grep cargo-cleanme     # the version Cargo believes it owns
```

If those disagree, repair it with:

```sh
cargo install cargo-cleanme --locked --force
```

Fixed in **0.1.5**, which also parses the `.crates.toml` schema that current
Cargo actually writes.

> Do not use a C017 victim to rehearse a later release. It must rehearse a
> version that *can* update, from a *staged candidate that can be*
> `install.sh`-updated.

### If you are on 0.1.1 or 0.1.2: `update` could not complete

The identity check ran the downloaded candidate with **no arguments**, so its
stdout could never equal a version string — and before failing, it scanned the
whole machine. The transaction then aborted safely every time, so nothing was
corrupted and no file was replaced. The advertised feature simply never once
worked in any published version.

Fixed in **0.1.3**. If you are on 0.1.1 or 0.1.2, upgrade with `cargo install
cargo-cleanme --locked --force` rather than `cargo cleanme update`.

## Integrity is not authenticity

The `.sha256` files are **integrity evidence, not authenticity**. They prove
that the bytes you downloaded are the bytes the release built. They prove
nothing about *who* published them, and `update` makes no authenticity claim.

There is no **maintainer-key signature** verification, no transparency log, and
no reproducible-build claim for any target. What exists instead is a GitHub
**release attestation**, verified by `scripts/verify-release-attestation.py`;
its trust root is GitHub's release infrastructure rather than an independent
maintainer key, and the updater does not check it — `update` verifies the
`.sha256` sidecar and nothing more. See
[INSTALLING.md](INSTALLING.md#signing-and-notarization).

## Binary size

0.1.0 used the external `curl` executable and **could not reach the registry at
all**: crates.io answers HTTP 403 to a non-descriptive User-Agent, and that
adapter had no way to set one. 0.1.1 replaced it with the embedded
HTTP/TLS transport, which works.

The 0.2.x build carries both of those in-process: the HTTP/TLS client that
reaches crates.io and the release host, and the SHA-256 verifier that checks
the downloaded asset against its `.sha256` sidecar before the live file is
replaced. `curl` was originally chosen because it is much smaller, and that
reasoning was correct about size and wrong about qualification — a transport
that cannot perform the required transaction is not a cheaper transport, it is a
broken one. The measured 0.1.0 → 0.1.1 comparison is recorded in the 0.1.1
entry of `CHANGELOG.md`; this repository publishes no per-release size table
for 0.2.x, so no size is quoted here.

## The `update --format json` exception

`update --format json` is the one output mode that does **not** use the
versioned envelope. It differs from the envelope in four concrete ways: it
carries its own `schema_version` literal rather than the envelope's contract,
it has **no** `scope` and **no** `mode` (an update resolves no cleanup scope
and runs no cleanup mode), its `result` object is a different projection
entirely, and it is assembled from a hand-rolled `serde_json::Map` rather than
a typed struct — so a change to an unrelated struct cannot move a published
field.

The document is exactly this: `schema_version` (the literal `1`),
`cargo_cleanme_version`, `operation`, `dry_run`, and a `result` object
carrying `from_version`, `to_version`, `target`, `asset`, `tag`, `provenance`,
and `changed`. The `provenance` value is the domain's stable code
(`self_managed`, `cargo_managed`, `cargo_install_only_host`,
`unprovable_ownership`), never the human remediation prose, and `changed` is
derived from the requested mode rather than from what the transaction happened
to do — a dry run that reported `changed` would be claiming bytes it never
touched.

It is **not** an untested surface. Four unit tests in `mod update_tests`
(`src/output.rs`) pin its schema version, its identity fields, the dry-run and
mutating pair, every public provenance token, and the
one-document-plus-exactly-one-newline stream contract, and a CLI contract case
pins that the binary's JSON branch prints exactly that stream and nothing
else.

So if you are parsing it, do not assume the fields other commands share are
present — check for them. Migrating it to `EnvelopeV1` is a contract change
and needs a plan and a version bump. The testing gap it used to carry is
closed, so that is no longer a reason to wait.
