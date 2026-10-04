# C011 — Self-update transport qualification corrective

Status: closed

Repository baseline: `8b393135bbaff41c553207dcb198fb8bf43f5533` (tag `v0.1.0`)

Closure: `plans/closure/distribution-release-update/c011-status.md`

Original plan: `plans/implementation/distribution-release-update/010c-eggup-self-update-and-install-provenance.md`

Original closure record: `plans/closure/distribution-release-update/010c-status.md`

Source roadmap: Phase 10 — Distribution, release, and update

Subsystem roadmap: `plans/subsystems/distribution-release-update-roadmap.md`

Implementation commit: `95bb38e2bf2b335211bff4498089016f6d4239eb` (tag `v0.1.1`)

Related: `c010-eggup-curl-user-agent-seam.md` (the bounded upstream request this
corrective's evidence shows is genuinely needed),
`plans/closure/distribution-release-update/010d-status.md`.

## 1. Objective

Two defects make `cargo cleanme update` fail in production in ways the M010C
test suite could not detect. This corrective repairs both, and repairs the
*evidence standard* that let a non-functional feature be recorded as closed.

M010C is not reopened wholesale. Its provenance, identity, and ownership
invariants were real, held, and are unaffected. What was false is narrower and
is named exactly in section 3.

## 2. Why previous verification missed it

The M010C test suite was 19 fixture tests, all green, against a feature that
did not work. The suite drove a fixture transport that always answers, and it
asserted against fixture metadata. It therefore could only ever prove that the
*policy* was correct given a working transport and a cooperative registry. It
proved nothing about whether the production transport was one.

The 0.1.0 release smoke then made two real requests against the live registry
and both defects appeared immediately. This is the second time in Phase 10
that a fixture-only suite passed over a defect that only a real third party
could reveal; the first was the C009 Windows lane. The pattern is now treated
as a process finding, not an accident — see section 6.

## 3. The unclosed requirements and defects

### C011-F1 (critical) — the production transport could not read the version authority

M010C selected `eggup-curl 0.1.2` as the production transport and recorded
that selection as a *passed* requirement: "Prefer the lightweight curl path;
measure both". The measurement was real, and the conclusion drawn from it was
wrong.

`eggup-curl` 0.1.2 has no `User-Agent` seam. Verified directly in the published
source: `CurlConfig` exposes `strict`, `timeouts`, `max_redirects`,
`follow_redirects`, `allowed_protocols`, `proxy`, and `effective_timeouts`; the
curl argument vector is built from a fixed set of `--location`, `--max-redirs`,
`--connect-timeout`, `--max-time`, `--max-filesize`, `--proto`,
`--proto-redir`, `--noproxy`, `--output`, `--write-out`, and `--`, with **no**
argument seam and zero occurrences of any User-Agent handling. The adapter also
calls `env_clear()` and restores only proxy variables, which closes the
`CURL_*`-variable and `~/.curlrc` escape hatches as well.

So every request went out as `curl/<version>`. crates.io answers HTTP 403 to
any non-descriptive User-Agent. Measured against the live registry during the
0.1.0 smoke:

| Request `User-Agent` | Response |
|---|---|
| absent | 403 |
| `curl/8.5.0` | 403 |
| descriptive agent naming product and version | 200 |

Consequence: `update` could never read the version authority, on any host, in
any state. The feature was non-functional in production while its tests were
fully green.

Why the requirement was recorded as satisfied: the matrix row asserted a
*preference* ("prefer the lightweight curl path") and the evidence satisfied
that preference. The row did not assert the requirement it stood in for — that
the transport can actually complete the transaction — so nothing ever tested
it. This is a defect in the milestone's acceptance criteria as much as in its
implementation.

### C011-F2 (critical, latent) — `update` would have downgraded every 0.1.1 install

The version comparison accepted any pair of differing versions as "newer
available". Once 0.1.1 was published, a user installing 0.1.1 and running
`cargo cleanme update` would have compared running `0.1.1` against published
`0.1.0`, taken the update branch, and replaced their working binary with the
older one. The defect was masked in 0.1.0 only because the transport could
never get far enough to reach the comparison.

The registry is the version authority for *releases*. It is not entitled to
replace a build that was never published, such as one from `main` or
`--git`. Comparison is now numeric, an unorderable pair is refused, and a
build newer than the published stable version stops with
`UpdateError::NewerThanPublished`.

## 4. Resolution

The production transport is `eggup-eggfetch` 0.1.2, whose
`EggfetchConfig::user_agent` is the seam the requirement needs. The updater
identifies itself as
`cargo-cleanme/<version> (+https://github.com/dbowm91/cargo-cleanme)`.

The curl path is **not** retained as a fallback. A fallback that cannot reach
the version authority is a second way to fail, and keeping it would invite a
future contributor to silently reintroduce the 403.

This reverses a decision recorded as settled in the M010C closure record. The
original reasoning — that curl is smaller and therefore better — was correct
about size and wrong about qualification, and it should not have been allowed
to close the question before a single live registry request had been made.

## 5. The cost, stated rather than minimized

| Measurement | 0.1.0 (curl) | 0.1.1 (eggfetch) | Delta |
|---|---|---|---|
| Release binary | 5,420,664 B | 12,125,624 B | +6,704,960 B |
| Normal dependencies | 82 | 177 | +95 |

The updater's transport is now an embedded HTTP/TLS stack
(`eggfetch-core` + rustls + tokio) rather than the system `curl` executable.
This is a genuine regression in footprint and a genuine improvement in
function: 0.1.0 had the smaller binary and a feature that could not run.

`release/baseline-benchmark.json` records the new figures. The semantic-counter
gate is unaffected; the binary size and dependency count are reported, never
gated.

## 6. Regression evidence required to prevent recurrence

1. **A test on the requirement, not the preference.**
   `the_production_transport_identifies_itself_descriptively` asserts the
   wired transport's effective User-Agent names the product and is neither
   empty nor a bare tool name. It is written against the *property* the
   requirement is, so it survives a future transport change and fails if a
   transport that cannot satisfy it is wired in.

2. **A test for the ordering rule that was missing entirely.**
   `a_newer_local_build_is_never_downgraded` and
   `an_older_published_version_is_not_offered_to_a_matching_build` cover both
   directions, and `versions_order_numerically_not_lexically` pins numeric
   ordering so `0.1.10 > 0.1.9` cannot regress to a lexical comparison.

3. **A live smoke that runs before publication, not only after.** The 0.1.0
   defect was found by the post-release smoke. `docs/RELEASING.md` now requires
   the external smoke to be run against a *staged draft* before anything is
   published.

4. **A process finding, recorded.** Two of this milestone's defects were
   invisible to fixtures because both concerned a live third party's behavior.
   A feature that depends on a remote service's policy needs at least one
   rehearsal against that service before its milestone is closed. This is why
   M010C's "no update was performed against a real release, because no release
   exists" was accepted as sufficient, and it should not have been.

## 7. Out of scope

- Reopening or rewriting the M010C closure record to imply it succeeded. The
  record keeps its original content and carries an addendum.
- Changing `eggup-curl`. That is the bounded upstream request in C010.
- Keeping a curl fallback, or writing a second HTTP client.
- The M010D publication sequence, which continues independently.

## 8. Verification

~~~sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo +1.89 check --locked --all-targets
python3 scripts/release-benchmark.py --check
bash scripts/release-check.sh
~~~

Plus the live requirement, which no command above substitutes for:

~~~sh
cargo build --release
./target/release/cargo-cleanme update --dry-run
~~~

A 0.1.1 build against a published 0.1.0 must refuse with
`NewerThanPublished`; a 0.1.0 build against a published 0.1.0 must report
`AlreadyCurrent`. Both are registry-backed and neither is provable offline.
