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

## Known defects by version

Two defects in the `0.1.x` line affected the updater. Both were found by the
project's own release rehearsals, not by a test, and both are fixed. None of
these versions is yanked, because their other behavior is correct and yanking
would misdescribe them.

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

There is no signature verification, no transparency log, and no
reproducible-build claim for any target. See
[INSTALLING.md](INSTALLING.md#signing-and-notarization).

## Binary size

0.1.0 used the external `curl` executable and **could not reach the registry at
all**: crates.io answers HTTP 403 to a non-descriptive User-Agent, and that
adapter had no way to set one. 0.1.1 replaced it with the embedded
HTTP/TLS transport, which works. The release binary is roughly twice the size it
was in 0.1.0 — that is the measured cost of a transport that functions.

## The `update --format json` exception

`update --format json` is the one output mode that does **not** use the
versioned envelope. It bypasses `EnvelopeV1`, hand-rolls its own
`schema_version`, and has no `scope`, no `mode`, and a differently shaped
`result` object.

It is also the only output surface in the CLI that is **untested**. That is a
known, recorded gap. If you are parsing it, do not assume the fields other
commands share are present — check for them.

Migrating it to `EnvelopeV1` is a contract change and needs a plan and a
version bump. Adding a test for it needs neither.
