# Self-Update — provenance, version authority, and staged replacement

> Component deep dive · part of the [architecture overview](overview.md)

`src/update.rs` (1971 lines: 1021 production, 950 inline tests, 30 `#[test]`
functions) implements `cargo cleanme update`. It is architecturally distinct from
the other 16 modules in `src/`: the only module with a network dependency, and
the only one that writes to its own executable path.

Three questions are answered definitively below; each is load-bearing elsewhere
in this document.

| Question | Answer | Evidence |
|---|---|---|
| Can a Cargo-managed binary ever be replaced? | Not once classified as such — but the guarantee is only as strong as the detector, which fails **open** in four enumerated cases | `src/update.rs:835-840`, `:850-857` |
| Is the downloaded artifact's integrity verified before replace? | Yes — SHA-256 against the release `.sha256` sidecar, plus an executed identity check. The trust root is HTTPS to GitHub, **not** a signature | `src/update.rs:929`, `:954-958`, `:972-983` |
| Is version comparison semver-aware or lexicographic? | Numeric (tuple of `u64`). **Not** lexicographic | `src/update.rs:582-595` |

---

## 1. Responsibility

This module owns *policy* and delegates *mechanics*. The split is the module's
own header (`src/update.rs:5-16`):

- **Owned here:** whether this binary may update itself (provenance), which
  release is acceptable, which host this is, what the candidate must say about
  itself, and the refusal text a user acts on.
- **Delegated to the published Eggup crates:** acquisition, transport, digest
  binding, bounded candidate execution, the mutation lock, staging, rollback.
  "Nothing here re-implements a checksum, a retry, a staging directory, a lock,
  or a rollback" (`src/update.rs:14-16`) — §6 verifies this, and finds no
  divergence.

It shares no code with the scan/cleanup path. Verified: exactly two modules
import `crate::error` — `src/config.rs:1` and `src/update.rs:68`. `update` has no
dependency on discovery, policy, or cleanup, and none of them on it. The
updater's dependency set (`eggup-acquisition`, `eggup-core`, `eggup-eggfetch`,
`Cargo.toml:78-80`) is disjoint from the scan path's.

**Blast radius.** This is the highest-consequence code in the crate, in two
directions. A bug here replaces the binary that would otherwise be the thing
that fixes it: every other module fails at *reporting*; this one fails at *being*.
And a bug in the provenance check can destroy a user's Cargo-managed install
silently and lastingly — the bytes change, but `cargo install --list` keeps
reporting the old version. That is not hypothetical: it is C017, which reached
published `v0.1.1`..`v0.1.4` (`plans/registry.md:81`, `:144`).

---

## 2. Public surface

Complete inventory. Anything not listed is private.

### Host mapping

| Item | Signature | Contract |
|---|---|---|
| `host_target` | `-> Option<&'static str>` (`src/update.rs:120`) | The contracted triple for the host this binary was *compiled* for, via `#[cfg]` on `target_os`/`target_arch`. `None` outside the five published targets. |
| `asset_for_target` | `fn(&str) -> Option<&'static str>` (`:145`, private) | Triple → asset name, from `PUBLISHED_TARGETS`. |
| `PUBLISHED_TARGETS` | `const` (`:76`, private) | Compile-time copy of `release/eggpack/distribution.toml`; `scripts/check-release-contract.py` fails the build on divergence (`:72-75`). |

### Outcomes

`Provenance` (`src/update.rs:166-181`) — why this installation cannot be updated:

| Variant | `code()` | Meaning | User action |
|---|---|---|---|
| `CargoManaged { bin_root, version }` | `cargo_managed` | Positively recorded in a Cargo install root's `.crates.toml`, naming both this package and this binary. Cargo owns the file and its bookkeeping. | `cargo install cargo-cleanme --locked --force` (`:198`) |
| `VerifiableSelfManaged { digest }` | `self_managed` | Not Cargo-managed, and positively identified by exact SHA-256 of current bytes, revalidated under Eggup's mutation lock before replacement. | none (`:207`) — the only state in which an update proceeds |
| `CargoInstallOnlyHost { triple_hint }` | `cargo_install_only_host` | Host has no published artifact; reached only when `host_target()` is `None` (`:404-408`). | `cargo install cargo-cleanme --locked` (`:201`) |
| `UnprovableOwnership { detail }` | `unprovable_ownership` | Could not be positively identified, so replacement cannot be proven safe. | reinstall via the published installer (`:203-206`) |

`code(&self) -> &'static str` (`:185`) is the stable machine-readable JSON code;
`remediation(&self) -> Option<String>` (`:195`) is the exact command to run
instead, when one exists.

`UpdatePlan` (`src/update.rs:247-254`) is the resolved input for one run and the
whole JSON surface (`src/main.rs:568-594`, `schema_version: 1`). Fields:
`from_version`, `to_version`, `target`, `asset`, `tag`, `provenance`. It is
`Clone`; `run` returns a clone of the pre-commit plan (`:1000`).

`UpdateError` (`src/update.rs:258-291`) — all ten variants, `Display` text, and
the `From` mapping:

| Variant | `Display` | → `AppError` |
|---|---|---|
| `TransportUnavailable { detail }` | `the update transport is unavailable: {detail}` (`:297`) | `AppError::Update` |
| `NoPublishedStableVersion { detail }` | `the registry reported no usable stable version: {detail}` (`:300-303`) | `AppError::Update` |
| `AlreadyCurrent { current, published }` | `already at the latest stable version ({current}; published is {published})` (`:306-309`) | `AppError::Update` |
| `NewerThanPublished { current, published }` | `this build ({current}) is newer than the published release ({published}); refusing to downgrade it` (`:312-316`) | `AppError::Update` |
| `Acquisition { stage, detail }` | `could not {stage}: {detail}` (`:319`) | `AppError::Update` |
| `ReleaseEvidence { detail }` | `release integrity evidence is unusable: {detail}` (`:322`) | `AppError::Update` |
| `Transaction { detail }` | `the update transaction failed: {detail}` (`:325`) | `AppError::Update` |
| `Receipt { disposition, detail }` | `the update did not commit cleanly ({disposition}): {detail}` (`:331-334`) | `AppError::Update` |
| `Provenance { provenance }` | `this installation cannot be updated by cargo-cleanme update ({code})` (`:336-342`) | **`AppError::Provenance(remediation_text(..))`** (`:359-361`) |
| `HostUnsupported { detail }` | `this host has no published cargo-cleanme binary: {detail}` (`:344-347`) | `AppError::Update` |

The single special mapping is deliberate and commented: "A refused update is a
user-actionable condition, not a crash" (`:358`). A refusal becomes
`AppError::Provenance` carrying the *manager command*; everything else becomes
`AppError::Update(String)`. `remediation_text` (`:367-376`) assembles
`"{PRODUCT} is {code}; run this instead:\n  {command}"`.

### Environment, verification, version, entry points

- `trait UpdateEnvironment` (`:613-632`) — seven methods; §5.
- `HttpEnvironment` (`:649`, private) — the production implementation; §5.
- `LiveDigestVerifier` (`:219-241`, private struct) — holds
  `HashMap<MemberId, [u8; 32]>`; implements Eggup's `OwnershipVerifier` (`:224`).
  §6. It is the *only* ownership concept; there is no path- or name-based
  ownership anywhere in the module.
- `published_stable_version(&[u8]) -> Result<String, UpdateError>` (`:554`),
  `compare_release_versions(&str, &str) -> Option<Ordering>` (`:582`),
  `is_release_version(&str) -> bool` (`:598`). §4.
- `run(&dyn UpdateEnvironment, bool) -> Result<UpdatePlan, UpdateError>` (`:769`) —
  the whole transaction behind an injected environment, and the only reason the
  module is testable.
- `update(bool) -> Result<UpdatePlan, UpdateError>` (`:1015`) — CLI wrapper;
  constructs `HttpEnvironment` and delegates. Called from `src/main.rs:25`.
- `classify_provenance(&Path) -> Provenance` (`:386`) and
  `classify_provenance_in(&Path, Option<PathBuf>) -> Provenance` (`:403`). §3.

No `now`-style clock helper exists. The only time reads are the staging-directory
nonce (`:698-701`) and the test counter (`:1160`); neither is policy, so neither
is on the seam. Time influences no decision.

### `classify_provenance` vs `classify_provenance_in`

They differ by exactly one thing: where the Cargo home comes from.
`classify_provenance` (`:386-388`) is a one-line wrapper calling
`classify_provenance_in(current_exe, default_cargo_home())`, where
`default_cargo_home` reads `CARGO_HOME`, else `$HOME/.cargo` (`:391-395`).
`classify_provenance_in` (`:403`) is the decision function, taking the home as a
parameter.

The reason is stated at `:399-402`: `set_var` is `unsafe` in edition 2024, and a
test mutating process-global environment to check a pure decision would be "both
unsound and order-dependent". The parameter is the testability seam for the
refusal path specifically (repeated on the trait method, `:617-620`).

**Who calls each:** `classify_provenance_in` is called by `run` (`:775`),
supplied from the same seam via `environment.cargo_home()`. `classify_provenance`
is called only by tests (`:1629`, `:1644`) and is otherwise dead production
weight.

---

## 3. Provenance classification

`classify_provenance_in` (`src/update.rs:403-458`) is the module's most
consequential logic: a four-step decision with a strictly ordered outcome.

**Step 0 — host gate (`:404-409`).** If `host_target()` is `None`, return
`CargoInstallOnlyHost` before touching the filesystem. Note the dead binding at
`:409` (`let _ = target;`): the target is discarded here and recomputed at
`:777`, showing that the classification result and the transaction's target are
two independent reads of the same function.

**Step 1 — explicit Cargo home (`:416-431`).** If
`current_exe.parent() == cargo_home/bin`, this is the plain `cargo install`
layout, with two outcomes, the second being the important one:
`cargo_recorded_version` returns a version → `CargoManaged`; returns `None` →
**`UnprovableOwnership`**, not `VerifiableSelfManaged`. The comment at
`:411-415` is explicit that absence of a record inside a Cargo root is "itself a
reason to refuse, not a reason to adopt the file", because the premise of
`CargoManaged` is that Cargo's bookkeeping must not be made to lie. Inside
`$CARGO_HOME/bin`, a missing record is evidence *for* a Cargo-managed file, not
evidence of an adoptable one. This is the one place the classifier fails
**closed**.

**Step 2 — the ancestor walk, the `cargo install --root` shape (`:442-447`).**
`cargo_bin_roots_above` (`:484-494`) walks up to `CARGO_ROOT_SEARCH_DEPTH + 1 =
5` ancestors of the executable's parent and maps each to `<ancestor>/bin`,
because Cargo's only layout is `ROOT/bin/<exe>` with `ROOT/.crates.toml` — the
owning root is the executable's grandparent (`:463-465`).

This is the C017 fix. Before it, a `cargo install --root DIR` binary was
**misdetected as self-managed and silently replaced**, leaving
`cargo install --list` reporting a version the file no longer had
(`plans/registry.md:81`). **The fix is present**, and three properties make it
work:

1. **Location alone never grants ownership** (`:481-483`, `:438-441`). Every
   candidate root must additionally yield a matching `cargo_recorded_version`.
   This is what keeps the published installer's `~/.local/bin` and
   `/usr/local/bin` resolving to self-managed: a directory that merely looks
   like an install root is not evidence of anything (`:440-441`).
2. **The record must name this package *and* this binary** (`:529`, `:538`).
   `real_crates_toml` in the tests is parsed against Cargo's actual current
   schema, verified by `the_crates_toml_we_parse_is_the_one_cargo_writes`
   (`:1417`) — the pre-C017 code parsed a schema no current cargo writes.
3. **The bound is deliberately generous** (`:468-470`): under-detection is "the
   dangerous direction", since a Cargo-owned file misread as self-managed gets
   *replaced*, while a false positive "only costs a refusal that names the
   manager command".

**Step 3 — digest, or refuse (`:449-457`).**
`eggup_core::hash_file(current_exe)`. Success → `VerifiableSelfManaged { digest }`.
Any I/O error → `UnprovableOwnership` carrying the error text.

What "self-managed" means, precisely: **not** "not managed by anything" and
**not** "installed by our installer". Only that no Cargo root above this path
claims it, *and* its exact bytes are hashable. The name overstates the evidence;
the doc comment at `:212-218` is accurate — it "never trusts the file name, the
`PATH` order, or the directory it lives in."

### `cargo_recorded_version` (`:509-544`)

Reads `bin_root/../.crates.toml`, parses as `toml::Value`, takes the `v1` table,
and per entry: `spec.rsplit_once(')')` (`:522`) strips the parenthesised source
**from the right**, because a source id may itself contain spaces (`:515-517`);
a spec lacking `')'` is skipped, not fatal (`:519-521`); then requires
`name == package` (`:529`) and `binary` ∈ the value array (`:538`).

The skip is deliberate and regression-tested at `:1589`, whose comment records
that an earlier draft of the test placed the bad key *above* the `[v1]` header —
making it a root-level key the parser never reads, so the test passed against
code that aborted the scan. The comment names the shape directly: "the
blind-fixture shape this subsystem has now paid for five times" (`:1599-1600`).

### The consequence chain

```
Cargo-managed file
 → .crates.toml unreadable / pruned / depth > 4 / non-UTF-8 name   (four fail-open cases, §8)
 → no root claims it
 → hash succeeds                                                     (src/update.rs:449)
 → VerifiableSelfManaged
 → run() provenance gate passes                                      (src/update.rs:835)
 → binary replaced; bytes no longer match the version Cargo recorded
                                                     ← C017, silent and lasting
```

`remediation()` (`:195-209`) is the user-facing mitigation for the states a
correct classifier reaches: `cargo install cargo-cleanme --locked --force` for a
Cargo-managed file, reinstall-via-installer for unprovable ownership. `--force`
is required because the file being replaced is Cargo's.

---

## 4. The version authority

### Source

`crates_io_metadata_url()` (`:154-156`) returns
`https://crates.io/api/v1/crates/cargo-cleanme`. `fetch_metadata` (`:732-753`)
pulls it into memory bounded by `MAX_METADATA_BYTES = 1 MiB` (`:100`) —
"generous while still making the fetch finitely bounded, which is the point."

The whole document is the authority; `published_stable_version` (`:554-576`) then
reads exactly one field, `crate.max_stable_version`, with three failure layers
and distinct `detail` text: not UTF-8 (`:556-558`), not valid JSON (`:559-562`),
no such string (`:567-569`). Narrowness is pinned by
`version_authority_reads_exactly_one_field` (`:1309`); an outage is a distinct
tested outcome, not a guess — `a_registry_outage_is_reported_not_guessed`
(`:1358`).

### `is_release_version` (`:598-607`)

Requires **exactly three** dot-separated components, each non-empty and all ASCII
digits, with no fourth. One predicate rejects:

| Rejected | Why it matters |
|---|---|
| `1.2.3-rc.1` | A pre-release. An auto-updater that jumps to a `-beta` is a serious defect: there is no moment at which the user consented to that tradeoff. "a self-update cannot express 'I accept that tradeoff' the way an explicit `cargo install` can" (`:551-553`). |
| `1.2.3+build.5` | Build metadata — the fourth-component check at `:606` catches it |
| `1.2`, `1.2.3.4`, `x`, `""` | Not a version at all |

Yanked versions are handled by *field choice*, not this predicate: crates.io
computes `max_stable_version` and excludes yanked releases. There is no
independent yanked check and none is needed. `published_stable_version` applies
the predicate at `:570-574`, so it is the prerelease gate on the authority's own
output, not decoration.

### `compare_release_versions` — numeric, definitively

```rust
let parse = |value: &str| -> Option<[u64; 3]> { /* split '.', parse u64, reject 4th */ };
Some(parse(left)?.cmp(&parse(right)?))          // src/update.rs:583-594
```

Each side parses into a `[u64; 3]`, compared as a tuple. This is
**semver-aware for the plain `X.Y.Z` subset, not lexicographic** — the comparison
never sees a string, so the classic `0.1.10 < 0.1.9` bug is impossible.
`versions_order_numerically_not_lexically` (`:1862-1883`) asserts `0.1.10 >
0.1.9`, `0.10.0 > 0.9.9`, equality, ordering, and `None` for `("1.2","1.2.3")`,
`("1.2.3-rc.1","1.2.3")`, `("x","1.2.3")`.

The cost of not using the `semver` crate: this is not a general semver
comparator. It cannot order a prerelease against a release — it returns `None`.
That is the safer failure, and `run` treats `None` as a refusal "rather than
guess" (`:809-815`).

### The no-downgrade guard

**Present**, at `src/update.rs:797-805`:

```rust
Some(std::cmp::Ordering::Less) => {
    // Never downgrade. A build that is newer than the published stable
    // release is a local or unreleased build, and the registry is not
    // entitled to replace it.
    return Err(UpdateError::NewerThanPublished { .. });
}
```

This is the C011 addition (`plans/registry.md:75`: "Reversed the transport to
`eggup-eggfetch` and added the missing no-downgrade guard"). It prevents a
locally bumped or development build — exactly what a developer has after
`cargo install --path .` — being replaced by the older published release.
`a_newer_local_build_is_never_downgraded` (`:1886-1899`) is the guard, and its
comment records the defect was found by the live release smoke, "not by a
fixture." `an_older_published_version_is_not_offered_to_a_matching_build` (`:1902`)
pins `Equal → AlreadyCurrent`.

---

## 5. The environment seam

### `trait UpdateEnvironment` (`src/update.rs:613-632`)

| Method | Abstracts | Notes |
|---|---|---|
| `current_exe() -> Result<PathBuf, UpdateError>` | The running executable's absolute path | `std::env::current_exe()` (`:680`) |
| `cargo_home() -> Option<PathBuf>` | The Cargo home to consult | Exists purely so the refusal path is reachable from a test without mutating process globals (`:617-620`) |
| `current_version() -> String` | This build's version | `env!("CARGO_PKG_VERSION")` (`:690`) — compile-time and infallible, hence untestable-by-design in the fixture |
| `staging_dir() -> Result<PathBuf, UpdateError>` | Creating a private staging directory | `std::env::temp_dir()` + `cargo-cleanme-update-<pid>-<nanos>` (`:694-708`) |
| `fetch(url, destination) -> Result<u64, UpdateError>` | Streaming a URL to disk | The return value is load-bearing — the empty-asset check (`:880-884`) |
| `fetch_metadata(url, limit) -> Result<Vec<u8>, UpdateError>` | Fetching a bounded document into memory | `limit` threaded into `FetchLimits.max_metadata_bytes` (`:737-738`) |
| `cleanup(&Path)` | Removing **only this run's** directory | `remove_dir_all`, errors ignored (`:755-758`) |

### `HttpEnvironment` (`:649-759`)

Constructed at `:653-667`:

```rust
eggup_eggfetch::EggfetchConfig::strict()
    .user_agent(USER_AGENT)
    .timeouts(CONNECT_TIMEOUT, TOTAL_TIMEOUT)      // src/update.rs:658-660
```

- **It does set a User-Agent, and a descriptive one**:
  `cargo-cleanme/<CARGO_PKG_VERSION> (+https://github.com/dbowm91/cargo-cleanme)`,
  a `const` built with `concat!` (`:641-645`) so it is a compile-time string, not
  a runtime format.
- Timeouts `CONNECT_TIMEOUT = 10s`, `TOTAL_TIMEOUT = 120s` (`:105-106`), mirrored
  into `FetchLimits` with the two byte ceilings (`:669-676`). `strict()` is
  documented as "bounded deadlines, bounded redirects, explicit proxy decision"
  (`:655-657`).
- Error mapping differentiates two kinds of absence: `FetchOutcome::NotFound`
  becomes `ReleaseEvidence` for assets (`:726-728`) and
  `NoPublishedStableVersion` for metadata (`:749-751`), because an absent release
  asset and an absent crate are different facts. `AcquisitionError` is flattened
  with `format!("{error:?}")` (`describe_acquisition`, `:761-763`).

### The testability point, honestly

`FixtureEnvironment` (`:1031-1151`) lets all 30 tests run with no network, no
`curl` discovery, no process globals. That is necessary — and it is not evidence
the feature works. A fixture that always answers successfully cannot prove the
real transport reaches the authority, which is exactly the C011 lesson:
`plans/registry.md:95-103` records that C011 was a feature whose transport failed
on every host, and that "failure reports coverage that does not exist" — the
fixture registry always answered. The module's own header says it: "This was
found by the first live release smoke, not by the fixture suite, because the
fixture suite never talks to the registry" (`:41-42`).

The suite's best defence is partial.
`the_production_transport_identifies_itself_descriptively` (`:1912-1944`) does
construct a real `HttpEnvironment` (`:1927`) and reads the agent off the real
transport's config, asserting it equals `USER_AGENT`, names the product, is
non-empty, does not start with `curl/`, and contains both `/` and a space. It
proves the seam is *configured*; it cannot prove the registry *accepts* it, and
the test comment concedes exactly that (`:1925-1926`): "It does not model the
registry's policy, which would be a fixture that lies about the thing most likely
to change."

---

## 6. The update transaction

### `run` (`:769-843`) — planning

| # | Step | Failure mode / state left |
|---|---|---|
| 1 | Resolve `current_exe` (`:773`) | `Transaction`; nothing written |
| 2 | Read `current_version` (`:774`) | infallible in production |
| 3 | Classify provenance (`:775`) | none — yields `UnprovableOwnership` rather than an error |
| 4 | Resolve target (`:777-785`) | `None` → `Provenance { CargoInstallOnlyHost }`; no asset → `HostUnsupported` |
| 5 | Fetch metadata, 1 MiB bound (`:787`) | `Acquisition` / `NoPublishedStableVersion`; no writes |
| 6 | Extract `to_version` (`:788`) | `NoPublishedStableVersion` |
| 7 | Compare (`:790-816`) | `Equal` → `AlreadyCurrent`; `Less` → `NewerThanPublished` (**no-downgrade**); `None` → `NoPublishedStableVersion`; `Greater` → continue |
| 8 | Build plan; tag **constructed** as `v{to_version}` (`:818-826`) | no scrape, no redirect follow; asserted at `:1333` |
| 9 | `check_only` → return (`:828-830`) | **No filesystem write on this path** — see below |
| 10 | Provenance gate: only `VerifiableSelfManaged` proceeds (`:835-840`) | `Provenance` refusal before any download |

**Comment/code divergence:** the comment at `:832-834` says "Refuse before
acquiring anything." The metadata fetch at `:787` has already happened, so a run
makes a network request before refusing. "Acquiring" is accurate only for the
*artifact*; the consequence is bounded (a few KiB, no writes), but the wording
overstates.

### `execute` (`:845-1012`) — the transaction

10. Re-extract `live_digest` from the plan, refusing anything not
    `VerifiableSelfManaged` (`:850-857`) — a second, independent gate.
11. Create the private staging directory (`:859`).
12. All of the following runs inside a closure (`:860-994`) so `cleanup` is
    unconditional.
13. Fetch `<asset>.sha256` into staging (`:861-865`); parse with
    `eggup_core::parse_sha256_sidecar` (`:871-876`). *Failure:* `ReleaseEvidence`.
14. Fetch the asset (`:878-879`). Zero bytes → `ReleaseEvidence` (`:880-884`).
15. **Sidecar cross-check** (`:888-897`): if the sidecar names a file other than
    `plan.asset`, refuse — "so a sidecar copied from another release cannot
    silently authorize these bytes." Covered at `:1727`.
16. Build the `InstallPlan` (`:899-946`): install root is the executable's parent
    (`:899-904`); member declared `.with_permissions(PermissionsIntent::Executable)`
    (`:928`) and `.with_integrity(IntegrityRequirement::Sha256(*manifest.digest()))`
    (`:929`).
17. `prepare()` (`:948-953`) — Eggup stages and copies. `verify_integrity()`
    (`:954-958`) — **integrity checked here, before identity and before commit.**
18. `ExactIdentityValidator::new(.., expected_identity(&to_version)).args(["--version"])`
    (`:972-974`) — runs the candidate.
19. Commit with `CommitOwnership::new(&LiveDigestVerifier{..}, AbsentPolicy::DenyCreate)`
    (`:985-993`).
20. `environment.cleanup(&staging)` (`:996`) — **unconditional**, both paths.
21. Receipt (`:998-1011`): `Committed` → success; `RolledBack` → `Receipt
    {"rolled_back"}`; `RecoveryRequired` → `Receipt {"recovery_required"}`.

### `LiveDigestVerifier` / `OwnershipVerifier`, precisely

The expected value is the SHA-256 of the live executable **as observed before the
transaction began** — `classify_provenance`'s `digest` (`:850-851`), captured at
step 3 of `run` and threaded into the map at `:985-986`. `verify` (`:225-240`)
maps four outcomes:

| Condition | `Ownership` | Consequence |
|---|---|---|
| Member not in the expected map | `Unknown` | refuse |
| `symlink_metadata` → `NotFound` | `Absent` | refused, via `AbsentPolicy::DenyCreate` (`:990`) |
| Other I/O error | `Unknown` | refuse |
| **Is a symlink** | `Foreign` | refuse |
| Not a regular file | `Foreign` | refuse |
| `hash_file` == expected | `Owned` | commit proceeds |
| `hash_file` differs | `Foreign` | refuse |
| `hash_file` errors | `Unknown` | refuse |

The proof is: *the file is a regular file (not a symlink), still exists, and its
bytes still hash to what they hashed to before the download.* Eggup re-runs this
under its mutation lock, closing the observation-to-replacement window.
`AbsentPolicy::DenyCreate` closes the other direction: this updater may replace a
file it can prove it owns and may never create one.

### The identity check (C016) — fix present

`src/update.rs:972-974`:

```rust
let validator =
    ExactIdentityValidator::new(member_id.clone(), expected_identity(&plan.to_version))
        .args(["--version"]);
```

**Present**, and the comment at `:960-971` documents the defect in full: with no
arguments the binary runs its default routine scan, printing a scan report to
stdout and filesystem diagnostics to stderr — so an argv-less check "compares
stdout against a version string that can never match, and it scans the whole
machine on the way to failing." The same comment names why the suite missed it:
the live commit path had never succeeded, and the fixture's candidate stub
"ignored argv and printed the identity for any invocation"
(`plans/registry.md:80`, `:107`).

The expected string is `expected_identity(v) = format!("cargo-cleanme {v}\n")`
(`:109-111`) — trailing newline included, matching `--version` output exactly.
This is the only check that the thing about to replace this process is really us
(`:961-962`).

### Atomicity of the replace

Verified against the dependency source `eggup-core-0.1.2` (version confirmed at
`Cargo.lock:435-437`):

1. The **old file is moved aside, not deleted**:
   `fs::rename(&destination, &backup_root/…)` at
   `eggup-core-0.1.2/src/transaction.rs:583`, into `.eggup-backup-<nonce>-<nanos>`
   (`:724`) created in the installation root's parent.
2. The **new file is renamed over the destination**:
   `fs::rename(&staged, &destination)` at `transaction.rs:432`.
3. Eggup stages into a *sibling* of the install root —
   `.eggup-stage-<root>-<pid>-<seq>-<nanos>` at `stage.rs:195-213`, mode `0700` —
   moving bytes with `fs::copy` (`stage.rs:87`), not `rename`. That is why a
   source in `/tmp` on a different filesystem works.

**It is two renames, not one.** Each is atomic and same-filesystem (stage and
backup are both in the destination's parent), but the replacement is a sequence
with a window: between `transaction.rs:583` and `transaction.rs:432` the
destination path **does not exist**. A process killed in that window — SIGKILL,
power loss, OOM — leaves the installation with no `cargo-cleanme` at that path
and the previous binary intact inside `.eggup-backup-*`. Recoverable by hand, but
not atomic in the strict sense, and nothing in `src/update.rs` narrows or
documents the window.

Residue: the module's `cleanup` (`:996`) removes only the temp staging directory
it created. `.eggup-stage-*` and `.eggup-backup-*` are Eggup's, removed on the
success path (`transaction.rs:521`, `stage.rs:192`) and after a *verified*
rollback (`transaction.rs:944`). A crash leaves them in the parent of the install
root — e.g. `/usr/local/.eggup-backup-*`. `scripts/post-release-smoke.sh:209`
checks only for `cargo-cleanme-update-*` under `TMPDIR`, so it would not catch
this.

### Rollback: delegated, not reimplemented

The `Cargo.toml:61-63` claim — "Generic acquisition, transport, integrity,
staging, and rollback are owned by the published Eggup crates, not reimplemented
here" — is **verified accurate**. This module contains no checksum computation,
no retry loop, no lock, and no backup/restore code. Rollback is `restore_entries`
in the dependency, renaming the backup back over the destination
(`transaction.rs:583` on the restore path) and reporting `RolledBack` or, if that
also fails, `RecoveryRequired` (`transaction.rs:925`/`:962`). `src/update.rs:999-1003`
only *translates* those dispositions into `UpdateError::Receipt`. What this module
does own is rollback *policy*: refuse rather than force.

`RecoveryRequired` also proves the guarantee is bounded. Eggup can report that it
could not put things back; this module surfaces that as a non-zero-exit
`AppError::Update` but does not attempt recovery, nor preserve the backup for
inspection.

### `--dry-run` guarantees

`check_only` returns at `:828-830`, before the provenance gate and before
`execute`. Guaranteed: no staging directory, nothing downloaded beyond the
metadata document, no file written, no candidate executed, no lock taken. The
plan is fully resolved, so a dry run still proves version comparison and target
resolution. `a_dry_run_downloads_nothing` (`:1814`) asserts no release-asset URL
was requested; `scripts/post-release-smoke.sh:213-221` rehearses it live.

Consequence worth stating: a dry run on a **Cargo-managed** installation
*succeeds*, because the gate at `:835` is after the `check_only` return. The
JSON's `provenance` field carries `"cargo_managed"` (`src/main.rs:588`), so the
information is available, but the exit code is 0. Only a real run refuses.

---

## 7. The Eggup transport decision

### Resolved dependency set

From `Cargo.lock`: `eggup-acquisition 0.1.2` (`:429-431`), `eggup-core 0.1.2`
(`:435-437`), `eggup-eggfetch 0.1.2` (`:444-446`), `eggfetch-core 0.2.2`
(`:386-388`). Declared at `Cargo.toml:78-80`.

| Crate | Used here for | Reference |
|---|---|---|
| `eggup-acquisition` | The transport *seam*: `AcquisitionRequest`, `AcquisitionTransport`, `CancelFlag`, `FetchLimits`, `FetchOutcome`, `AcquisitionError` — all in `HttpEnvironment` | `src/update.rs:57-60`, `:669-676`, `:710-753` |
| `eggup-core` | All mechanics: `hash_file`, `parse_sha256_sidecar`, `Sha256Manifest`, `IntegrityRequirement`, `PermissionsIntent`, `InstallPlan::prepare`, `verify_integrity`, `ExactIdentityValidator`, `OwnershipVerifier`, `CommitOwnership`, `AbsentPolicy`, `TransactionReceipt` | `src/update.rs:61-66`, used at `:234`, `:449`, `:871`, `:917-993` |
| `eggup-eggfetch` | The production transport and its `User-Agent` seam | `src/update.rs:650-665` |

### Why `eggup-eggfetch` and not the smaller `eggup-curl`

Documented at length in `Cargo.toml:64-77` and restated in the module header
(`src/update.rs:32-51`). The chain:

1. `eggup-curl` is smaller, because `eggup-acquisition` has zero dependencies
   (`Cargo.toml:65-66`) — avoiding an embedded HTTP/TLS stack in a CLI that
   self-updates rarely is a real advantage, and it was the first choice.
2. `CurlConfig` in `eggup-curl` 0.1.2 exposes **no User-Agent seam**, and the
   adapter clears the child environment (`Cargo.toml:67-68`), so every request
   goes out as `curl/x.y`.
3. crates.io answers **403** to any non-descriptive User-Agent
   (`Cargo.toml:69-70`, `src/update.rs:38-40`), so the curl transport could not
   read the version authority *at all*, on any host.
4. "A transport that cannot perform the required transaction is not qualified,
   regardless of its size" (`Cargo.toml:70-71`).
5. `EggfetchConfig::user_agent` is the seam the requirement needs
   (`Cargo.toml:71-72`, `src/update.rs:45-47`).

**The cost, stated rather than assumed:** an embedded HTTP/TLS stack —
`eggfetch-core` + rustls + tokio (`Cargo.toml:74-76`), with `eggfetch-core 0.2.2`
and `eggfetch-http-connect 0.2.2` confirmed at `Cargo.lock:386-388`, `:418-420`.
`Cargo.toml:75` says "The measured delta is recorded in the M010C closure
record" — **I did not open that record, so I cannot state the measured figure.**

The defect was found by the first live release smoke, not by the suite
(`src/update.rs:41-42`), corroborated by `plans/registry.md:90`: the external
smoke of `v0.1.0` found `update` returning HTTP 403 on every host.

### Exactly one transport — verified

`Cargo.toml:78-80` declares `eggup-acquisition`, `eggup-core`, `eggup-eggfetch`.
`eggup-curl` is **not** a dependency and appears nowhere in `src/`. The claim at
`Cargo.toml:76-77` — "Exactly one production transport is wired; the curl path is
not kept as a fallback, because it cannot perform the required transaction" — is
**accurate**. There is no transport `match` anywhere in the module.

### C010: the bounded upstream request

`CurlConfig::user_agent` seam on `eggup-curl`, status `proposed`
(`plans/registry.md:82`, `:138`). It cannot be closed from this repository, and
nothing waits on it (`plans/registry.md:38`).

If upstream accepted it, this crate could drop `eggup-eggfetch` and the
`eggfetch-core`/rustls/tokio subtree, constructing `eggup_curl` with a
descriptive agent instead. Two costs would remain: loss of `EggfetchConfig::strict()`'s
bounded-redirect and explicit-proxy behaviour (`src/update.rs:655-657`), and
reintroduction of a subprocess dependency (`curl` on `PATH`) for a code path
whose whole point is recovering a broken installation. The migration would be a
transport swap behind the `UpdateEnvironment` seam, not a rewrite — the practical
payoff of that seam existing.

**Registry inconsistency, flagged:** `plans/registry.md` contradicts itself. The
summary rows still read C014 `open` (`:78`), C016 `ready` (`:80`), C017 `ready`
(`:81`), while the status list (`:131-138`) and closing narrative (`:144`) state
all are **closed** with closure records and commit identifiers; `:38` agrees with
those. The narrative and status list are mutually consistent and name specific
commits (`826fbf2`, `0aa7664`), so the three stale rows are the outlier — but I
did not read the closure records to adjudicate. A documentation defect, not a
code one.

---

## 8. Invariants and edge cases

Each item is a verified finding, not an assumption.

### Can a Cargo-managed binary ever be replaced?

**Once classified as such, no — provably not**, via three gates: `run` refuses
anything but `VerifiableSelfManaged` (`:835-840`); `execute` re-checks (`:850-857`);
and the commit needs `Ownership::Owned`, which a Cargo-managed file still
satisfies. So the protection lives entirely in classification, which is
correctly stated rather than redundantly re-derived.

**But the guarantee is exactly as strong as the classifier, and the classifier
fails open** to `VerifiableSelfManaged` in four cases (see the chain in §3):

1. `.crates.toml` absent, unreadable, or unparseable at the owning root —
   `.ok()?` twice at `:511-512`.
2. The owning root is more than `CARGO_ROOT_SEARCH_DEPTH = 4` ancestors above the
   executable (`:471`, `:490`).
3. The executable's file name is not valid UTF-8 — `binary_name` returns `""`
   (`:474-476`), matching no record.
4. The record's value is not a TOML array, or the spec string lacks `')'`
   (`:522`, `:535`).

Any of these turns a Cargo-owned file into `VerifiableSelfManaged` and replaces
it silently. This is the C017 failure mode, narrowed. The asymmetry is
documented and accepted (`:468-470`): over-detection costs a refusal,
under-detection costs a user's Cargo install. **I could not determine whether any
of these four is reachable on current Cargo** — that would require running cargo,
out of scope here.

### Is integrity verified before replace?

**Yes, three times over.** (1) SHA-256 of the acquired artifact against the
digest in the release's own `<asset>.sha256` — `IntegrityRequirement::Sha256`
(`:929`), enforced by `verify_integrity()` (`:954-958`). (2) The sidecar must
name `plan.asset` (`:888-897`). (3) The candidate must identify itself as
`cargo-cleanme <to_version>\n` (`ExactIdentityValidator`, `:972-983`).

**The trust root is HTTPS to GitHub, not a signature.** The expected digest comes
from a file served by the same host as the artifact it describes
(`release_asset_url`, `:158-160`). No signature, no pinned key, no independent
digest channel. Integrity against corruption, truncation, and a mismatched
sidecar is solid; integrity against a *compromised release host* is delegated
entirely to TLS — a bound to state rather than imply. Pinned by
`a_checksum_mismatch_leaves_the_live_binary_untouched` (`:1684`) and
`a_verified_candidate_replaces_the_live_binary` (`:1672`).

### A read-only or unwritable install location

**Fails cleanly, before any rename.** `prepare()` creates a stage directory in
the *parent* of the install root (`eggup-core/src/stage.rs:195-213`, mode `0700`)
and copies bytes into it (`stage.rs:87`), so for `/usr/bin` an unprivileged
process fails at `src/update.rs:948-953` with `Transaction`. Even if staging
succeeded, the mutation lock is `.eggup-mutation.lock` **in the install root**
(`eggup-core/src/lock.rs:54`) and `require_ready_parent` re-checks the
destination's parent (`transaction.rs:788-806`). A read-only filesystem, full
disk, or permission-denied target therefore all surface as `Transaction` at the
staging step, with `cleanup` running unconditionally at `:996` and the live
binary untouched.

### Symlinks

`LiveDigestVerifier` returns `Ownership::Foreign` for a symlinked destination
(`:232`) — the commit is refused. Independently, `revalidate_destination`
requires the canonicalised ancestor under the canonicalised install root and the
destination to be a regular file
(`eggup-core/src/transaction.rs:743-765`), and `require_ready_parent` rejects a
symlinked *parent* (`transaction.rs:802-806`). One residual: `current_exe` uses
`std::env::current_exe()` (`:680`), which on Linux already returns a resolved
path, so the hazard is handled before it arrives.
`ownership_is_never_inferred_from_the_file_name` (`:1636`) covers the
classifier's half. **No path here follows a symlink to replace a file the user
did not intend.**

### Concurrency

**Serialized within one installation root.** Eggup takes
`<install_root>/.eggup-mutation.lock` (`lock.rs:54`) and re-runs the ownership
verifier under it immediately before backup/rename
(`revalidate_ownership_locked`, `transaction.rs:624+`). Staging names carry pid +
nanoseconds (`:698-703`), so concurrent runs do not collide there. No test here
exercises concurrency; the property is inherited from `eggup-core`, and I read
only the lock path construction, not its implementation.

### Temp-file residue on failure

The module's own temp directory is removed unconditionally at `:996` on both
paths, because the transaction body is a closure (`:860-994`) whose result is
bound before `cleanup`. Covered by
`staging_is_cleaned_up_on_success_and_on_failure` (`:1789`) and, live,
`scripts/post-release-smoke.sh:209`. Eggup's `.eggup-stage-*` /
`.eggup-backup-*` directories are removed on success and after a verified
rollback, but a hard crash mid-commit leaves them in the install root's parent.
Nothing here, and nothing in the smoke script, asserts their absence.

### Rollback if the new binary is non-functional

**None.** The candidate is executed *before* the commit (`:972-983`) to prove
identity, but that proves only that it prints the right string — not that a scan,
config load, or filesystem walk still works. Once `fs::rename(staged,
destination)` succeeds (`transaction.rs:432`), the only outcomes are `Committed`
(`:1000`) or a receipt-derived error. There is no post-commit validation, no
smoke run of the new binary in place, and no `commit_with_post_commit` use — the
dependency offers that API (`eggup-core/src/candidate.rs:465`) with a
`PostCommitFailurePolicy::RollBack` arm, and **this module does not use it.**

This is the real bound on the guarantee: the transaction proves *identity and
integrity before replacing*, not *function after replacing*. Recovery is manual —
`cargo install --locked --force` (`:198`) or the installer link (`:203-206`).

### Yanked or unpublished versions

Handled by field choice, not an explicit check: `crate.max_stable_version`
excludes yanked releases, and `is_release_version` (`:598`) rejects anything not
plain `X.Y.Z`, which is also what rejects a prerelease. No separate yanked check
exists; `is_release_version`'s comment mentions "a prerelease or a yanked
version" (`:551-552`) without a distinct yanked path — the comment is slightly
looser than the code.

### Downgrade or wrong-target installation

- **Downgrade:** impossible via the registry (§4, `:797-805`). The comparison is
  against `env!("CARGO_PKG_VERSION")` (`:690`) — the version compiled into this
  binary — so a binary whose manifest disagrees with its compiled version is
  compared against the compiled one, correct for a shipped artifact.
- **Wrong target:** the asset name comes from a compile-time table keyed by the
  host triple (`:120`, `:145`) and the URL is constructed, not followed
  (`:158-160`, `:824`), so no release *response* can select the artifact. What is
  not verified here is that the *hosted* asset for a triple is really the binary
  for that triple — that is `scripts/check-release-contract.py` and the Eggpack
  contract's job. `the_published_target_table_matches_the_eggpack_contract`
  (`:1831`) pins the table, but only if that script runs in CI, which I did not
  verify.

### `HOME` / `CARGO_HOME` unset

`default_cargo_home` (`:391-395`) returns `CARGO_HOME`, else `$HOME/.cargo`, else
`None`. With `None`, step 1 is skipped (`:416`) but **step 2 still runs**: the
ancestor walk derives `/home/u/.cargo/bin` from the executable's own path, so a
plain `cargo install` is still detected. Losing both variables degrades detection
to the ancestor walk alone — which is why that walk exists, since it is the
layout "every hermetic install script, CI job, and container image uses", and
where the process `CARGO_HOME` "is frequently *not* where the running binary
came from" (`:433-436`). It does not fail open in the common case.

### Verified comment/code drift

- Dead binding `let _ = target;` at `src/update.rs:409`.
- "Refuse before acquiring anything" (`:832-834`) overstates: metadata was
  already fetched at `:787`.
- `staging_dir` "never overwrites an existing path, it fails instead"
  (`:695-697`) is **wrong as written** — `create_dir_all` (`:704`) succeeds on an
  existing directory. Impact negligible (pid + nanosecond collision), but the
  comment overstates the guarantee.

---

## 9. Testing

### Real numbers

| Metric | Value | Method |
|---|---|---|
| Test module starts | line 1021 (`#[cfg(test)]` at `:1020`) | direct read |
| `#[test]` functions | **30** | `grep -nE '^\s*#\[test\]' src/update.rs` |
| Inline test lines | 950 (1021–1971) | 1971 − 1021 |
| `tests/` integration tests referencing `update` | **0** | `grep -rniE 'update\|provenance\|self.?update' tests/` → no matches |

`tests/cli_contract.rs` has 8 `#[test]` functions, `tests/end_to_end.rs` has 1, and
**not one mentions `update`**. The update path has no integration coverage.

### The 30 tests, grouped by what they protect

| Group | Tests (line) | Protects |
|---|---|---|
| **Provenance** (9) | `the_crates_toml_we_parse_is_the_one_cargo_writes` (1417), `a_cargo_root_install_is_refused_even_when_it_is_not_the_cargo_home` (1441), `cargo_managed_installation_is_refused_with_the_manager_command` (1465), `an_unrecorded_file_in_a_cargo_root_is_not_ours_to_replace` (1501), `the_published_installer_layout_is_not_mistaken_for_a_cargo_root` (1528), `a_cargo_record_that_omits_this_binary_does_not_claim_this_file` (1564), `a_malformed_crates_toml_entry_does_not_hide_the_entry_after_it` (1589), `a_self_managed_installation_is_proven_by_its_digest` (1626), `ownership_is_never_inferred_from_the_file_name` (1636) | The C017 fix from both directions. Most carefully written: `:1528`, whose comment explains a lenient fixture would prove nothing — with an irrelevant record both the correct and the broken answer are "self-managed" — so it plants the one near miss that could cause a wrongful claim (1533-1541). |
| **Version selection & comparison** (8) | `version_authority_accepts_only_plain_releases` (1289), `version_authority_reads_exactly_one_field` (1309), `version_authority_rejects_prerelease_and_junk` (1316), `already_current_is_not_an_error_to_hide` (1345), `a_registry_outage_is_reported_not_guessed` (1358), `the_release_tag_is_constructed_not_scraped` (1333), `versions_order_numerically_not_lexically` (1862), `an_older_published_version_is_not_offered_to_a_matching_build` (1902) | Narrow authority, numeric comparison, constructed tags, outage as error not guess. |
| **No-downgrade** (1) | `a_newer_local_build_is_never_downgraded` (1886) | The C011 guard; comment records the defect was found by live smoke, not a fixture. |
| **Integrity** (5) | `a_verified_candidate_replaces_the_live_binary` (1672), `a_checksum_mismatch_leaves_the_live_binary_untouched` (1684), `a_candidate_reporting_the_wrong_version_is_rejected_before_commit` (1712), `a_sidecar_naming_a_different_asset_is_rejected` (1727), `an_absent_release_asset_is_not_replaced_with_anything` (1749) | `:1684` is the most significant test in the module: it is the one proving a bad candidate cannot become the installed binary. |
| **Transaction hygiene** (2) | `staging_is_cleaned_up_on_success_and_on_failure` (1789), `a_dry_run_downloads_nothing` (1814) | No residue; check-only writes nothing. |
| **Transport & release contract** (5) | `the_production_transport_identifies_itself_descriptively` (1913), `the_published_target_table_matches_the_eggpack_contract` (1831), `the_version_authority_url_is_https_and_product_scoped` (1947), `release_urls_are_constructed_from_the_tag_and_contracted_asset` (1954), `the_host_target_has_a_contracted_asset` (1964) | `:1913` is the C011 guard and the only test touching real transport construction. |

9 + 8 + 1 + 5 + 2 + 5 = 30.

### `FixtureEnvironment` (`:1031-1151`)

**What it fakes:** executable path, Cargo home, current version, the staging
directory (real on disk, via `unique()` at `:1157`), `fetch` (writes canned bytes
to the destination, returns the byte count), and `fetch_metadata` (canned bytes).
Three response modes — `respond` (`:1058`), `fail_transport` (`:1068`), `absent`
(`:1074`) — plus `requested` (`:1041`), asserted at `:1814`.

**What it cannot fake:**

1. **The network.** No User-Agent, TLS, redirects, proxies, status codes, or
   crates.io 403 policy. This is the C011 gap exactly
   (`plans/registry.md:95-103`).
2. **`env!("CARGO_PKG_VERSION")`.** `current_version` returns whatever the test
   declares (`:1095-1097`), so a test can claim to be any version without the
   binary being that version — the no-downgrade tests depend on this (`:1890`).
3. **argv.** The candidate stub at `:1183` printed the identity for *any*
   invocation, which is precisely why C016 survived. I verified that from the
   comments at `:966-971` and `:1916-1921`; I did not read `:1183-1215` line by
   line.
4. **A real filesystem for provenance.** `run` calls `classify_provenance_in`
   with the fixture's own `exe` and `cargo_home` (`:775`), so classification *is*
   exercised, but against invented paths. Real layouts — symlinked `~/.cargo`,
   relocated `CARGO_HOME`, root-writable `/usr/local/bin` — are not modelled.
5. **Concurrency, permissions, read-only filesystems, full disks.** Nothing in
   the fixture can make a write fail.

### Honest gaps

The registry's own evidence: this module shipped two defects no test caught
(C016, C017) and a whole corrective (C011) was green for the wrong reason
(`plans/registry.md:90`, `:107`). Concretely, these would all pass:

1. **Any change to real transport wire behaviour.** Dropping `.user_agent(...)`
   (`:659`) fails only if the constant changes too. Setting the agent to
   something the registry *rejects* — a bare token with no `/` or space, or a UA
   blocked by policy — passes everything, because no test models registry policy
   (`:1925-1926`). This is C011 re-entering through a different door.
2. **A provenance hole in any of the four fail-open cases in §8.** No
   `.crates.toml`; a changed depth bound; a non-UTF-8 name. Each yields
   `VerifiableSelfManaged` and a *replacement*. All nine provenance tests
   construct a well-formed `.crates.toml`; none tests its absence, and a
   well-formed-record test cannot distinguish "detected" from "would have been
   replaced anyway".
3. **The commit window.** Any regression in the two-rename sequence — a
   `RecoveryRequired` path, an unrestored backup, a residue directory — is
   invisible: the fixture always lets the happy path finish.
   `staging_is_cleaned_up_on_success_and_on_failure` checks the *module's* temp
   dir only, never `.eggup-backup-*`.
4. **Post-commit non-functionality.** Nothing asserts the installed binary works
   after replacement; the fixture's "installed" file is a byte-written stub, not
   an executable.
5. **A wrong-but-consistent host mapping** — only caught if
   `scripts/check-release-contract.py` runs in CI, which I did not verify.

### Is the real transport ever exercised automatically?

**No — not by `tests/`, and not by `scripts/smoke-release-candidate.py`.**

- `tests/cli_contract.rs` and `tests/end_to_end.rs`: zero references to `update`.
- `scripts/smoke-release-candidate.py`: a digest-computing validator over release
  bytes; no `update` invocation.
- The only automated exercise of the real transport is
  `scripts/post-release-smoke.sh` — "Real, networked self-update rehearsal
  against two *public* releases" (`:2`), "Work Package F of C014" (`:4`), against
  the *public* updater rather than a freshly built copy (`:6-8`). It rehearses a
  self-managed update (`:184-198`), a dry run (`:213-221`), and a Cargo-managed
  refusal asserting the exact remediation string (`:257-275`).

So the honest answer, and the crux of C011/C014/C016/C017: **the real transport
and the real commit path are exercised only by a networked rehearsal against
already-published releases, run as part of the release process.** That rehearsal
found C016 and C017; nothing else in the repository would have found either. It
is also inherently lagging — a fix cannot be validated by it until a release
carries the fix, which is why C016 and C017 sat at `ready` for releases and why
`v0.1.1`/`v0.1.2` are recorded as shipping an updater that cannot complete a
commit (`plans/registry.md:107`).

---

## 10. Review checklist

Concrete checks, each anchored to a line observed. Priority reflects blast radius.

1. **Provenance misdetection.** Confirm every non-`VerifiableSelfManaged` result
   of `classify_provenance_in` (`src/update.rs:403-458`) is actually refused
   before any rename — trace both gates at `:835-840` and `:850-857`. Then check
   the four paths that reach `VerifiableSelfManaged` on a Cargo-owned file:
   `.ok()?` twice (`:511-512`), the `CARGO_ROOT_SEARCH_DEPTH = 4` bound (`:471`),
   `binary_name` returning `""` for non-UTF-8 (`:474-476`), and the non-array /
   missing-`)` skips (`:522`, `:535`).
2. **Integrity is checked before replace, against what.** Confirm
   `IntegrityRequirement::Sha256(*manifest.digest())` is attached to the member
   (`:929`) and that `verify_integrity()` (`:954-958`) runs *before* `validate`
   (`:977`) and *before* `commit` (`:989`). Then confirm the trust root: the
   expected digest comes from a sidecar served by the same host as the artifact
   (`:158-160`, `:861-865`) — no signature, no pinned key. Any change letting a
   digest arrive from anywhere else is a critical regression.
3. **The sidecar cross-check.** `:888-897` must refuse when
   `manifest.filename() != plan.asset`. Removing it is how a sidecar from another
   release would authorize arbitrary bytes.
4. **No-downgrade.** `:797-805` must remain; verify the `Less` arm returns
   `NewerThanPublished` and the comparison source is `env!("CARGO_PKG_VERSION")`
   (`:690`), not anything scraped at runtime.
5. **Comparison must stay numeric.** `:583-594` parses to `[u64; 3]` and compares
   tuples. Any refactor toward string comparison — or toward a general semver
   crate that would accept and then order prereleases — changes a proven
   property. `is_release_version` (`:598-607`) is the prerelease gate; it must
   reject `1.2.3-rc.1` and `1.2.3+build`.
6. **Atomicity and the crash window.** Verify the old file is *renamed aside*
   into a backup, not deleted, and the new file renamed over it
   (`eggup-core-0.1.2/src/transaction.rs:583` and `:432`). Then record honestly
   that this is two renames with an absent-destination window which this module
   neither narrows nor documents. Check `cleanup` (`:996`) is still on the
   unconditional path via the closure (`:860-994`).
7. **Identity invocation.** `:974` must keep `.args(["--version"])`. This is the
   C016 fix; dropping it makes the identity check unmatchable *and* triggers a
   full filesystem scan inside the commit path (`:964-971`).
8. **The `User-Agent` seam.** `:658-660` must keep `.user_agent(USER_AGENT)`, and
   `USER_AGENT` (`:641-645`) must stay a `concat!` naming the product. This is the
   C011 defect, and the fixture suite cannot catch its recurrence (`:41-42`).
9. **Exactly one transport.** `Cargo.toml:78-80` should still list exactly
   `eggup-acquisition`, `eggup-core`, `eggup-eggfetch`, with no `eggup-curl`
   fallback reintroduced (`Cargo.toml:76-77`). C010 (`plans/registry.md:82`) is
   upstream-only; a local curl path would be a regression, not a partial fix.
10. **Fixture-vs-reality.** Classify every new test before writing it: does it
    exercise `HttpEnvironment`, or only the fixture? `:1912` is the model — real
    transport construction (`:1927`) plus an explicit statement of what it cannot
    prove (`:1925-1926`). Confirm `scripts/post-release-smoke.sh` still rehearses
    both directions: a real update (`:184-198`) and a Cargo-managed refusal with
    the exact remediation string (`:257-275`).
11. **Integration coverage gap.** There is no `tests/` coverage of `update`. A
    cheap addition driving `cargo-cleanme update --dry-run` against a stubbed
    origin would be worth more than another fixture test — but note the updater
    has **no environment override** for the version authority
    (`crates_io_metadata_url`, `:154-156`, is a hard-coded const format), unlike
    `packaging/install.sh:39-40`. That asymmetry is itself a finding.
12. **Documentation drift.** Re-check the "Refuse before acquiring anything"
    claim (`:832-834` vs the fetch at `:787`), the "it fails instead" claim
    (`:695-697` vs `create_dir_all` at `:704`), the dead binding at `:409`, and
    the three stale `plans/registry.md` rows at `:78`, `:80`, `:81` that
    contradict `:131-138` and `:144`.
