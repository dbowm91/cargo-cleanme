# Self-Update — provenance, version authority, and staged replacement

> Component deep dive · part of the [architecture overview](overview.md)

`src/update.rs` (3342 lines: 1499 production, 1843 lines of test code, 54
`#[test]` functions) implements `cargo cleanme update`. It is architecturally
distinct from the other 16 modules in `src/`: the only module with a network
dependency, and the only one that writes to its own executable path.

> **Citation health, read this before trusting a line number in this file.**
>
> Every source citation here is fully qualified — `src/update.rs:403`,
> `src/main.rs:84`, `src/output.rs:903` — so `scripts/check-doc-citations.py`
> gates **all** of them: the file exists, the line is in range, and the range is
> ordered. The previous revision of this file carried 257 bare `` `:N` ``
> references that the guard could not see, and C021 measured them to be 191–503
> lines behind the source. A citation that no tool can check is a citation that
> rots silently.
>
> This revision has **343** gated `.rs` citations and **no** bare `` `:N` ``
> references. Every line number below was re-derived by opening the line it
> names and reading what is there, not by shifting an old number arithmetically.
> Where a citation is to a *dependency's* source
> (`eggup-core-0.1.2/src/transaction.rs:583`) it is out of the guard's scope,
> because the guard only indexes `src/*.rs` and `tests/*.rs`; those are verified
> against the pinned crate in the cargo registry.

Three questions are answered definitively below; each is load-bearing elsewhere
in this document.

| Question | Answer | Evidence |
|---|---|---|
| Can a Cargo-managed binary ever be replaced? | **No.** A mutating run refuses any provenance but `VerifiableSelfManaged` **before** the registry is contacted, and C018 made local ambiguity fail *closed*: an unreadable, malformed, or oddly-shaped Cargo record is `UnprovableOwnership`, never self-managed. Uncertainty is never read as absence | `src/update.rs:1014-1016`, `src/update.rs:492-508`, `src/update.rs:515-519` |
| Is the downloaded artifact's integrity verified before replace? | Yes — SHA-256 against the release `.sha256` sidecar, enforced *before* the identity check and before the commit, plus an executed identity check. The trust root is HTTPS to GitHub, **not** a signature | `src/update.rs:1175`, `src/update.rs:1200-1204`, `src/update.rs:1218-1229` |
| Is version comparison semver-aware or lexicographic? | Numeric (tuple of `u64`). **Not** lexicographic | `src/update.rs:783-796` |

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
import `crate::error` — `src/config.rs:1` and `src/update.rs:68`; those are the
only two `crate::` references in the whole module. `update` has no dependency on
discovery, policy, or cleanup, and none of them on it. The updater's dependency
set (`eggup-acquisition`, `eggup-core`, `eggup-eggfetch`, `Cargo.toml:92-94`) is
disjoint from the scan path's.

**The dependency arrow points one way.** `src/output.rs` depends on `update`
(`src/output.rs:903`, `src/output.rs:941`), never the reverse: the updater's JSON
document is produced by `output::update_json`, not by `update.rs`. `update.rs`
knows nothing about JSON. The consequence is that the *machine contract* for
`update --format json` lives in the reporting module, not here — see §2.

**Blast radius.** This is the highest-consequence code in the crate, in two
directions. A bug here replaces the binary that would otherwise be the thing
that fixes it: every other module fails at *reporting*; this one fails at *being*.
And a bug in the provenance check can destroy a user's Cargo-managed install
silently and lastingly — the bytes change, but `cargo install --list` keeps
reporting the old version. That is not hypothetical: it is C017, which reached
published `v0.1.1`..`v0.1.4` (`plans/registry.md:221-222`).

---

## 2. Public surface

Complete inventory. Anything not listed is private.

### Host mapping

| Item | Signature | Contract |
|---|---|---|
| `host_target` | `pub fn() -> Option<&'static str>` (`src/update.rs:120`) | The contracted triple for the host this binary was *compiled* for, via `#[cfg]` on `target_os`/`target_arch`. `None` outside the five published targets. |
| `asset_for_target` | `pub fn(&str) -> Option<&'static str>` (`src/update.rs:145`) | Triple → asset name, from `PUBLISHED_TARGETS`. **Public** — `src/main.rs` uses it to name the asset in the JSON "already current" and "newer than published" documents (`src/main.rs:113`, `src/main.rs:154`). |
| `PUBLISHED_TARGETS` | `const` (`src/update.rs:76`, private) | Compile-time copy of `release/eggpack/distribution.toml`; `scripts/check-release-contract.py` fails the build on divergence (`src/update.rs:72-75`). |

### Outcomes

`Provenance` (`src/update.rs:166-181`) — why this installation cannot be updated:

| Variant | `code()` | Meaning | User action |
|---|---|---|---|
| `CargoManaged { bin_root, version }` | `cargo_managed` (`src/update.rs:187`) | Positively recorded in a Cargo install root's `.crates.toml`, naming both this package and this binary. Cargo owns the file and its bookkeeping. | `cargo install cargo-cleanme --locked --force` (`src/update.rs:198`) |
| `VerifiableSelfManaged { digest }` | `self_managed` (`src/update.rs:188`) | Not Cargo-managed, and positively identified by exact SHA-256 of current bytes, revalidated under Eggup's mutation lock before replacement. | none (`src/update.rs:207`) — the only state in which an update proceeds |
| `CargoInstallOnlyHost { triple_hint }` | `cargo_install_only_host` (`src/update.rs:189`) | Host has no published artifact; reached only when `host_target()` is `None` (`src/update.rs:404-408`). | `cargo install cargo-cleanme --locked` (`src/update.rs:201`) |
| `UnprovableOwnership { detail }` | `unprovable_ownership` (`src/update.rs:190`) | Could not be positively identified, so replacement cannot be proven safe. | reinstall via the published installer (`src/update.rs:203-206`) |

`code(&self) -> &'static str` (`src/update.rs:185`) is the stable machine-readable
JSON code; `remediation(&self) -> Option<String>` (`src/update.rs:195`) is the
exact command to run instead, when one exists.

### Cargo evidence

`CargoRecord` (`src/update.rs:587-605`) is **public**, and it must be: it is
the typed evidence C018 introduced to replace an `Option<String>`, and its four
states are the whole fail-closed rule. `CargoRecord::is_uncertain(&self) -> bool`
(`src/update.rs:610-612`) answers the only question a caller really needs —
"does this state leave ownership genuinely unknown, as opposed to positively
established as 'not Cargo's'?" — and returns `true` for `Uninterpretable` alone.

`UpdatePlan` (`src/update.rs:247-254`) is the resolved input for one run. Fields:
`from_version`, `to_version`, `target`, `asset`, `tag`, `provenance`. It is
`Clone`; `execute` returns a clone of the pre-commit plan
(`src/update.rs:1270`).

**The JSON surface is not in this module.** The document `update --format json`
prints is built by `output::update_json` (`src/output.rs:903-931`) and written
by `output::update_json_stream` (`src/output.rs:941-943`), which
`src/main.rs:194-201` prints with a single `print!`. Two *other* documents are
composed inline in `src/main.rs` for the two no-op outcomes —
`AlreadyCurrent` at `src/main.rs:118-133` and `NewerThanPublished` at
`src/main.rs:159-174` — each written with `serde_json::json!` rather than
through the library seam, each at `schema_version: 1`, and each carrying the
same `result` object shape. So the library seam covers the *success* path only;
the two no-op documents are a second, parallel producer of the same contract.
`update.rs` never sees any of it.

`UpdateError` (`src/update.rs:258-291`) — all ten variants, `Display` text, and
the `From` mapping:

| Variant | `Display` | → `AppError` |
|---|---|---|
| `TransportUnavailable { detail }` | `the update transport is unavailable: {detail}` (`src/update.rs:297`) | `AppError::Update` |
| `NoPublishedStableVersion { detail }` | `the registry reported no usable stable version: {detail}` (`src/update.rs:300-303`) | `AppError::Update` |
| `AlreadyCurrent { current, published }` | `already at the latest stable version ({current}; published is {published})` (`src/update.rs:308`) | `AppError::Update` |
| `NewerThanPublished { current, published }` | `this build ({current}) is newer than the published release ({published}); refusing to downgrade it` (`src/update.rs:312-316`) | `AppError::Update` |
| `Acquisition { stage, detail }` | `could not {stage}: {detail}` (`src/update.rs:319`) | `AppError::Update` |
| `ReleaseEvidence { detail }` | `release integrity evidence is unusable: {detail}` (`src/update.rs:322`) | `AppError::Update` |
| `Transaction { detail }` | `the update transaction failed: {detail}` (`src/update.rs:325`) | `AppError::Update` |
| `Receipt { disposition, detail }` | `the update did not commit cleanly ({disposition}): {detail}` (`src/update.rs:331-334`) | `AppError::Update` |
| `Provenance { provenance }` | `this installation cannot be updated by cargo-cleanme update ({code})` (`src/update.rs:336-342`) | **`AppError::Provenance(remediation_text(..))`** (`src/update.rs:359-361`) |
| `HostUnsupported { detail }` | `this host has no published cargo-cleanme binary: {detail}` (`src/update.rs:343-348`) | `AppError::Update`. Its doc comment (`src/update.rs:288-289`) names the same condition the `Display` and the only construction site (`src/update.rs:994-996`) do: the registry publishes no contracted asset for this host. |

The single special mapping is deliberate and commented: "A refused update is a
user-actionable condition, not a crash" (`src/update.rs:358`). A refusal becomes
`AppError::Provenance` carrying the *manager command*; everything else becomes
`AppError::Update(String)`. `remediation_text` (`src/update.rs:367-376`)
assembles `"{PRODUCT} is {code}; run this instead:\n  {command}"`.

### Environment, verification, version, entry points

- `trait UpdateEnvironment` (`src/update.rs:814-837`) — seven methods; §5.
- `HttpEnvironment` (`src/update.rs:854`, private) — the production
  implementation; §5.
- `LiveDigestVerifier` (`src/update.rs:220-241`, private struct) — holds
  `HashMap<MemberId, [u8; 32]>`; implements Eggup's `OwnershipVerifier`
  (`src/update.rs:224`). §6. It is the *only* ownership concept; there is no
  path- or name-based ownership anywhere in the module.
- `published_stable_version(&[u8]) -> Result<String, UpdateError>`
  (`src/update.rs:755`), `compare_release_versions(&str, &str) -> Option<Ordering>`
  (`src/update.rs:783`), `is_release_version(&str) -> bool`
  (`src/update.rs:799`). §4.
- `run(&dyn UpdateEnvironment, bool) -> Result<UpdatePlan, UpdateError>`
  (`src/update.rs:980`) — the whole transaction behind an injected environment,
  and the only reason the module is testable.
- `update(bool) -> Result<UpdatePlan, UpdateError>` (`src/update.rs:1285`) — CLI
  wrapper; constructs `HttpEnvironment` and delegates. Called from
  `src/main.rs:102`, dispatched from `src/main.rs:84`.
- `classify_provenance(&Path) -> Provenance` (`src/update.rs:386`) and
  `classify_provenance_in(&Path, Option<PathBuf>) -> Provenance`
  (`src/update.rs:403`). §3.

No `now`-style clock helper exists. The only time reads are the staging-directory
nonce (`src/update.rs:906-911`) and the test counter
(`src/update.rs:1556-1568`); neither is policy, so neither is on the seam. Time
influences no decision.

### `classify_provenance` vs `classify_provenance_in`

They differ by exactly one thing: where the Cargo home comes from.
`classify_provenance` (`src/update.rs:386-388`) is a one-line wrapper calling
`classify_provenance_in(current_exe, default_cargo_home())`, where
`default_cargo_home` reads `CARGO_HOME`, else `$HOME/.cargo`
(`src/update.rs:391-395`). `classify_provenance_in` (`src/update.rs:403`) is the
decision function, taking the home as a parameter.

The reason is stated at `src/update.rs:399-402`: `set_var` is `unsafe` in edition
2024, and a test mutating process-global environment to check a pure decision
would be "both unsound and order-dependent". The parameter is the testability
seam for the refusal path specifically (repeated on the trait method,
`src/update.rs:819-821`).

**Who calls each:** `classify_provenance_in` is called by `run`
(`src/update.rs:986`), supplied from the same seam via
`environment.cargo_home()`. `classify_provenance` is called from **production**
in the two JSON no-op branches (`src/main.rs:116`, `src/main.rs:157`) to fill
the `provenance` field, and from tests (`src/update.rs:2399`,
`src/update.rs:2414`).

> **Finding — the JSON no-op path bypasses the seam.** `src/main.rs` re-derives
> provenance by calling `classify_provenance`, which reads `CARGO_HOME`/`HOME`
> from the *process environment* (`src/update.rs:391-395`), rather than reusing
> the classification `run` already computed. In production the two agree —
> `HttpEnvironment::cargo_home` is `default_cargo_home()`
> (`src/update.rs:891-893`) — so this is not a live defect. But it is a second
> classification of the same file on a different input path, and a future
> environment override on the seam would silently make the two disagree. The
> plan already knows the shape: the updater has **no** override for the version
> authority either, unlike `packaging/install.sh:38-39`.

---

## 3. Provenance classification

`classify_provenance_in` (`src/update.rs:403-520`) is the module's most
consequential logic. The shape is a strictly ordered decision, and its rule is
one sentence: **uncertainty is never read as absence.**

**Step 0 — host gate (`src/update.rs:404-409`).** If `host_target()` is `None`,
return `CargoInstallOnlyHost` before touching the filesystem. Note the dead
binding at `src/update.rs:409` (`let _ = target;`): the target is discarded here
and recomputed at `src/update.rs:988`, showing that the classification result and
the transaction's target are two independent reads of the same function.

**Step 0b — two pre-checks that refuse outright (`src/update.rs:411-434`).**
Both were added after C018, and both are *fail-closed*:

- **Non-UTF-8 file name (`src/update.rs:415-422`).** `binary_name` downgrades a
  non-UTF-8 name to `""` (`src/update.rs:552-554`), and a `.crates.toml` value
  can only ever hold UTF-8, so such a name can never match a record. Left
  alone, the ancestor walk would read that non-match as "Cargo records nothing
  here" and fall through to hashing — the fail-open direction. The short-circuit
  returns `UnprovableOwnership` instead, before the walk starts.
- **Symlinked executable (`src/update.rs:427-434`).** `LiveDigestVerifier` maps
  a symlinked destination to `Ownership::Foreign`
  (`src/update.rs:232`), so the commit would be refused *late*. Classifying it
  upfront as `UnprovableOwnership` means the refusal arrives as a
  classification, before the registry is consulted, instead of as a commit
  failure after a download.

**Step 1 — explicit Cargo home (`src/update.rs:441-473`).** If
`current_exe.parent() == cargo_home/bin`, this is the plain `cargo install`
layout, and the outcome is read from the **typed** `CargoRecord` rather than an
`Option<String>`. `Recorded { version }` → `CargoManaged`
(`src/update.rs:445`). The other three states — `DoesNotClaim`, `Absent`,
`Uninterpretable` — all → **`UnprovableOwnership`**
(`src/update.rs:446-471`), never `VerifiableSelfManaged`.

The distinction between those three is C018's whole content. They used to
collapse into a single `None`, which made "Cargo says nothing here"
indistinguishable from "I could not read what Cargo would have said", and the
second fell through to the digest step. Inside `$CARGO_HOME/bin` the directory is
manager-sensitive by construction, so all three refuse. This remains the one
place the classifier refuses unconditionally, and it is now unconditional for a
*stated* reason rather than by accident of a null.

**Step 2 — the ancestor walk, the `cargo install --root` shape
(`src/update.rs:490-509`).**

`cargo_roots_above` walks up to `CARGO_ROOT_SEARCH_DEPTH + 1 = 5` ancestors of
the executable's parent (`src/update.rs:561-571`,
`src/update.rs:540`), because Cargo's only layout is `ROOT/bin/<exe>` with
`ROOT/.crates.toml` — the owning root is the executable's grandparent.

This is the C017 fix, and C018 changed *what makes a candidate a candidate*
without changing the walk. Before C018, every ancestor was mapped to
`<ancestor>/bin` and probed; the "bin-ness" was derived from the path shape. Now
each candidate root is read directly, and:

1. **`Recorded` → `CargoManaged`** (`src/update.rs:492-497`).
2. **`DoesNotClaim` or `Absent` → keep walking** (`src/update.rs:498`). Absence
   is *not* uncertainty: a root with no `.crates.toml` shows no evidence of being
   a Cargo root, which is precisely what keeps the published installer's
   `~/.local/bin` and `/usr/local/bin` on the self-managed path. C018 could
   have closed this by treating every ancestor `bin` as manager-owned; that would
   have made every installer layout unupdatable to fix a rarer case.
3. **`Uninterpretable` → `UnprovableOwnership`, stop**
   (`src/update.rs:499-508`). The file's *presence* is the positive evidence
   that this is a Cargo installation root, and the content that would settle
   ownership is exactly what could not be read. Walking past it would replace a
   file Cargo owns.

**Step 3 — digest, or refuse (`src/update.rs:511-519`).**
`eggup_core::hash_file(current_exe)`. Success → `VerifiableSelfManaged { digest }`.
Any I/O error → `UnprovableOwnership` carrying the error text.

What "self-managed" means, precisely: **not** "not managed by anything" and
**not** "installed by our installer". Only that no Cargo root above this path
claims it, *and* its exact bytes are hashable. The name overstates the evidence;
the doc comment at `src/update.rs:212-218` is accurate — it "never trusts the
file name, the `PATH` order, or the directory it lives in."

### `read_cargo_record` and `CargoRecord`

`read_cargo_record` (`src/update.rs:623-745`) reads `ROOT/.crates.toml`,
size-capped before the read, and returns one of four states rather than
`Option<String>`.

`Recorded { version }` requires `spec.rsplit_once(')')` to strip the
parenthesised source **from the right** (`src/update.rs:699` — a source id may
itself contain spaces), then requires the name to equal the package
(`src/update.rs:711-715`) and the binary to be in the value array
(`src/update.rs:726-730`). Requiring the binary is what makes this a statement
about *this file* rather than about a package that merely shares a Cargo home.

`Uninterpretable` covers: unreadable (`src/update.rs:659-666`), over the size
ceiling (`src/update.rs:647-655`), not a regular file
(`src/update.rs:642-646`), invalid TOML (`src/update.rs:669-679`), a document
with no `v1` table (a shape Cargo never writes, so reading it as "Cargo owns
nothing" would be trusting an assumption instead of the record —
`src/update.rs:681-692`), and an entry that *names this package* but whose
version or executable list cannot be interpreted
(`src/update.rs:733-741`). The last one is the case that matters: a malformed
entry for a **different** package is skipped, because the parser can prove it
cannot be the cargo-cleanme record (`src/update.rs:700-715`), while a malformed
entry that names this package might still be the one that claims this binary.

The unrelated-malformed-entry case is regression-tested, because the two
directions pull opposite ways: treating every broken entry as fatal would let an
unrelated broken package hide a later valid cargo-cleanme record and force a
refusal where ownership is provable.

### The consequence chain

```
Cargo-managed file
 → .crates.toml unreadable / pruned / non-UTF-8 name / malformed entry
                                              (three of the four classes in §8)
 → pre-C018: no root claims it
 → pre-C018: hash succeeds                                       → VerifiableSelfManaged
 → pre-C018: run() provenance gate passes
 → pre-C018: binary replaced; bytes no longer match the version Cargo recorded
                                                     ← C017, silent and lasting

post-C018: the record is read as typed evidence
 → Uninterpretable → UnprovableOwnership, stop the walk              (§3, step 2)
 → run() refuses a mutating update *before* contacting the registry
 → binary untouched; the manager command is printed instead
```

The chain above is quoted in its pre-C018 form because it is the failure that
was fixed, and the second form is what the code does now. The fourth class —
the owning root lying beyond the search bound — is characterised and bounded in
§8 rather than closed.

`remediation()` is the user-facing mitigation for the states a
correct classifier reaches: `cargo install cargo-cleanme --locked --force` for a
Cargo-managed file, reinstall-via-installer for unprovable ownership. `--force`
is required because the file being replaced is Cargo's.

---

## 4. The version authority

### Source

`crates_io_metadata_url()` (`src/update.rs:154-156`) returns
`https://crates.io/api/v1/crates/cargo-cleanme`. `fetch_metadata`
(`src/update.rs:943-964`) pulls it into memory bounded by
`MAX_METADATA_BYTES = 1 MiB` (`src/update.rs:100`) — "generous while still
making the fetch finitely bounded, which is the point."

The whole document is the authority; `published_stable_version`
(`src/update.rs:755-777`) then reads exactly one field,
`crate.max_stable_version`, with three failure layers and distinct `detail`
text: not UTF-8 (`src/update.rs:756-759`), not valid JSON
(`src/update.rs:760-763`), no such string (`src/update.rs:764-770`). Narrowness
is pinned by `version_authority_reads_exactly_one_field`
(`src/update.rs:1708`); an outage is a distinct tested outcome, not a guess —
`a_registry_outage_is_reported_not_guessed` (`src/update.rs:1757`).

### `is_release_version` (`src/update.rs:799-808`)

Requires **exactly three** dot-separated components, each non-empty and all ASCII
digits, with no fourth. One predicate rejects:

| Rejected | Why it matters |
|---|---|
| `1.2.3-rc.1` | A pre-release. An auto-updater that jumps to a `-beta` is a serious defect: there is no moment at which the user consented to that tradeoff. "a self-update cannot express 'I accept that tradeoff' the way an explicit `cargo install` can" (`src/update.rs:751-754`). |
| `1.2.3+build.5` | Build metadata — the fourth-component check at `src/update.rs:807` catches it |
| `1.2`, `1.2.3.4`, `x`, `""` | Not a version at all |

Yanked versions are handled by *field choice*, not this predicate: crates.io
computes `max_stable_version` and excludes yanked releases. There is no
independent yanked check and none is needed. `published_stable_version` applies
the predicate at `src/update.rs:771-775`, so it is the prerelease gate on the
authority's own output, not decoration.

### `compare_release_versions` — numeric, definitively

```rust
let parse = |value: &str| -> Option<[u64; 3]> { /* split '.', parse u64, reject 4th */ };
Some(parse(left)?.cmp(&parse(right)?))          // src/update.rs:784-795
```

Each side parses into a `[u64; 3]`, compared as a tuple. This is
**semver-aware for the plain `X.Y.Z` subset, not lexicographic** — the comparison
never sees a string, so the classic `0.1.10 < 0.1.9` bug is impossible.
`versions_order_numerically_not_lexically` (`src/update.rs:2777`) asserts
`0.1.10 > 0.1.9`, `0.10.0 > 0.9.9`, equality, ordering, and `None` for
`("1.2","1.2.3")`, `("1.2.3-rc.1","1.2.3")`, `("x","1.2.3")`.

The cost of not using the `semver` crate: this is not a general semver
comparator. It cannot order a prerelease against a release — it returns `None`.
That is the safer failure, and `run` treats `None` as a refusal "rather than
guess" (`src/update.rs:1038-1046`).

### The no-downgrade guard

**Present**, at `src/update.rs:1028-1036`:

```rust
Some(std::cmp::Ordering::Less) => {
    // Never downgrade. A build that is newer than the published stable
    // release is a local or unreleased build, and the registry is not
    // entitled to replace it.
    return Err(UpdateError::NewerThanPublished { .. });
}
```

This is the C011 addition. C011 was a *transport qualification* corrective, and
its second finding was latent behind the first: "the moment 0.1.1 was published
it would have offered every 0.1.1 install a 'downgrade' to 0.1.0"
(`plans/closure/distribution-release-update/c011-status.md:27-28`); the guard is
recorded at `plans/closure/distribution-release-update/c011-status.md:44`. It
prevents a locally bumped or development build — exactly what a developer has
after `cargo install --path .` — being replaced by the older published release.
`a_newer_local_build_is_never_downgraded` (`src/update.rs:2801`) is the guard, and
its comment records the defect was found by the live release smoke, "not by a
fixture." `an_older_published_version_is_not_offered_to_a_matching_build`
(`src/update.rs:2817`) pins `Equal → AlreadyCurrent`.

---

## 5. The environment seam

### `trait UpdateEnvironment` (`src/update.rs:814-837`)

| Method | Abstracts | Notes |
|---|---|---|
| `current_exe() -> Result<PathBuf, UpdateError>` | The running executable's absolute path | `std::env::current_exe()` (`src/update.rs:885-889`) |
| `cargo_home() -> Option<PathBuf>` | The Cargo home to consult | Exists purely so the refusal path is reachable from a test without mutating process globals (`src/update.rs:819-821`) |
| `current_version() -> String` | This build's version | `env!("CARGO_PKG_VERSION")` (`src/update.rs:895-897`) — compile-time and infallible, hence untestable-by-design in the fixture |
| `staging_dir() -> Result<PathBuf, UpdateError>` | Creating a private staging directory | `std::env::temp_dir()` + `cargo-cleanme-update-<pid>-<nanos>`, created with `fs::create_dir` (`src/update.rs:899-919`). `create_dir`, not `create_dir_all`: the latter *succeeds* on an existing path, which is not the invariant the name is supposed to have. |
| `fetch(url, destination) -> Result<u64, UpdateError>` | Streaming a URL to disk | The return value is load-bearing — the empty-asset check (`src/update.rs:1115-1119`) |
| `fetch_metadata(url, limit) -> Result<Vec<u8>, UpdateError>` | Fetching a bounded document into memory | `limit` threaded into `FetchLimits.max_metadata_bytes` (`src/update.rs:948-949`) |
| `cleanup(&Path) -> Result<(), String>` | Removing **only this run's** directory | `remove_dir_all` (`src/update.rs:966-969`). It returns the failure instead of swallowing it: up to `MAX_ARTIFACT_BYTES` (128 MiB) can remain in the temp directory, and a command that says "updated" while leaving them has not described what it did. |

### `HttpEnvironment` (`src/update.rs:854-970`)

Constructed at `src/update.rs:859-871`:

```rust
eggup_eggfetch::EggfetchConfig::strict()
    .user_agent(USER_AGENT)
    .timeouts(CONNECT_TIMEOUT, TOTAL_TIMEOUT)      // src/update.rs:863-865
```

- **It does set a User-Agent, and a descriptive one**:
  `cargo-cleanme/<CARGO_PKG_VERSION> (+https://github.com/dbowm91/cargo-cleanme)`,
  a `const` built with `concat!` (`src/update.rs:846-850`) so it is a
  compile-time string, not a runtime format.
- Timeouts `CONNECT_TIMEOUT = 10s`, `TOTAL_TIMEOUT = 120s`
  (`src/update.rs:105-106`), mirrored into `FetchLimits` with the two byte
  ceilings (`src/update.rs:874-881`). `strict()` is documented as "bounded
  deadlines, bounded redirects, explicit proxy decision, and an identifying
  User-Agent" (`src/update.rs:860-862`).
- Error mapping differentiates two kinds of absence: `FetchOutcome::NotFound`
  becomes `ReleaseEvidence` for assets (`src/update.rs:937-939`) and
  `NoPublishedStableVersion` for metadata (`src/update.rs:960-962`), because an
  absent release asset and an absent crate are different facts.
  `AcquisitionError` is flattened with `format!("{error:?}")`
  (`describe_acquisition`, `src/update.rs:972-974`).

### The testability point, honestly

`FixtureEnvironment` (`src/update.rs:1301-1550`) lets all 47 tests run with no
network, no `curl` discovery, no process globals. That is necessary — and it is
not evidence the feature works. A fixture that always answers successfully cannot
prove the real transport reaches the authority, which is exactly the C011 lesson:
the registry records that C011 was a feature whose transport failed on every
host, and that "failure reports coverage that does not exist"
(`plans/registry.md:272-284`). The module's own header says it: "This was found
by the first live release smoke, not by the fixture suite, because the fixture
suite never talks to the registry" (`src/update.rs:41-42`).

The suite's best defence is partial.
`the_production_transport_identifies_itself_descriptively`
(`src/update.rs:2828`) does construct a real `HttpEnvironment`
(`src/update.rs:2842-2843`) and reads the agent off the real transport's config
(`src/update.rs:2844`), asserting it equals `USER_AGENT`, names the product, is
non-empty, does not start with `curl/`, and contains both `/` and a space. It
proves the seam is *configured*; it cannot prove the registry *accepts* it, and
the test comment concedes exactly that (`src/update.rs:2838-2841`): "It does not
model the registry's policy, which would be a fixture that lies about the thing
most likely to change."

---

## 6. The update transaction

### `run` (`src/update.rs:980-1078`) — planning

| # | Step | Failure mode / state left |
|---|---|---|
| 1 | Resolve `current_exe` (`src/update.rs:984`) | `Transaction`; nothing written |
| 2 | Read `current_version` (`src/update.rs:985`) | infallible in production |
| 3 | Classify provenance (`src/update.rs:986`) | none — yields `UnprovableOwnership` rather than an error |
| 4 | Resolve target and asset (`src/update.rs:988-996`) | `None` target → `Provenance { CargoInstallOnlyHost }`; no asset → `HostUnsupported` |
| 5 | **Provenance gate** (`src/update.rs:1014-1016`) | `Provenance` refusal — **before any network request** (mutating runs only) |
| 6 | Fetch metadata, 1 MiB bound (`src/update.rs:1018`) | `Acquisition` / `NoPublishedStableVersion`; no writes |
| 7 | Extract `to_version` (`src/update.rs:1019`) | `NoPublishedStableVersion` |
| 8 | Compare (`src/update.rs:1021-1047`) | `Equal` → `AlreadyCurrent`; `Less` → `NewerThanPublished` (**no-downgrade**); `None` → `NoPublishedStableVersion`; `Greater` → continue |
| 9 | Build plan; tag **constructed** as `v{to_version}` (`src/update.rs:1049-1057`) | no scrape, no redirect follow |
| 10 | `check_only` → return (`src/update.rs:1059-1061`) | **No filesystem write on this path** — see below |
| 11 | Defence in depth: re-check `plan.provenance` (`src/update.rs:1063-1075`) | `Provenance` refusal; unreachable today |
| 12 | `execute` (`src/update.rs:1077`) | the transaction |

**The ordering is the fix, and it is now literally true.** The gate at step 5
sits *above* the metadata fetch at step 6. The comment above it
(`src/update.rs:998-1013`) says so, and explains why: "When local provenance
alone makes mutation impossible, there is nothing the registry can tell us that
changes the answer." A mutating run on forbidden provenance therefore makes
**zero** registry requests — asserted by
`a_mutating_run_on_forbidden_provenance_makes_zero_registry_requests`
(`src/update.rs:2212`) and by the integration test
`json_update_refusal_emits_no_document_and_exits_non_zero`
(`tests/cli_contract.rs:3260`).

**The one path that still contacts the registry on forbidden provenance is
`--dry-run`,** and that is deliberate: "reporting a candidate is the entire
point of a dry run, and the classification it reports alongside is the honest
description of why a real run would stop"
(`src/update.rs:1011-1013`). The dry run therefore *reports* the real
provenance in the plan it returns — asserted by
`a_dry_run_still_reports_a_candidate_and_names_the_real_provenance`
(`src/update.rs:2244`).

### `execute` (`src/update.rs:1080-1282`) — the transaction

1. Re-extract `live_digest` from the plan, refusing anything not
   `VerifiableSelfManaged` (`src/update.rs:1085-1092`) — **defence in depth,
   not the gate**. The gate that decides is step 5 of `run`
   (`src/update.rs:1014-1016`); by here `check_only` has already returned and the
   provenance is known to be `VerifiableSelfManaged`, so the `else` arm cannot
   be reached today. It stays because `plan.provenance` is a field a future
   step could rebuild, and a commit that trusts it must be able to refuse on
   its own. It does **not** save the user's bandwidth: by this point the
   metadata fetch has already happened.
2. Create the private staging directory (`src/update.rs:1094`).
3. All of the following runs inside a closure (`src/update.rs:1095-1240`) so
   `cleanup` is unconditional.
4. Fetch `<asset>.sha256` into staging (`src/update.rs:1096-1100`); parse with
   `eggup_core::parse_sha256_sidecar` (`src/update.rs:1106-1111`). *Failure:*
   `ReleaseEvidence`.
5. Fetch the asset (`src/update.rs:1113-1114`). Zero bytes → `ReleaseEvidence`
   (`src/update.rs:1115-1119`).
6. **Sidecar cross-check** (`src/update.rs:1121-1143`): if the sidecar names a
   file other than `plan.asset`, refuse — "so a sidecar copied from another
   release cannot silently authorize these bytes." A bare hash with no filename
   skips the binding entirely, so it is rejected just like a wrong name.
7. Build the `InstallPlan` (`src/update.rs:1145-1192`): install root is the
   executable's parent (`src/update.rs:1145-1150`); the member is declared
   `.with_permissions(PermissionsIntent::Executable)` (`src/update.rs:1174`) and
   `.with_integrity(IntegrityRequirement::Sha256(*manifest.digest()))`
   (`src/update.rs:1175`).
8. `prepare()` (`src/update.rs:1194-1199`) — Eggup stages and copies.
   `verify_integrity()` (`src/update.rs:1200-1204`) — **integrity checked here,
   before identity and before commit.**
9. `ExactIdentityValidator::new(.., expected_identity(&to_version)).args(["--version"])`
   (`src/update.rs:1218-1220`) — runs the candidate.
10. Commit with `CommitOwnership::new(&LiveDigestVerifier{..}, AbsentPolicy::DenyCreate)`
    (`src/update.rs:1231-1239`).
11. `let cleanup = environment.cleanup(&staging)` (`src/update.rs:1245`) —
    **unconditional**, both paths, and its result is captured rather than
    `?`-propagated so a leftover temp directory cannot mask the transaction's own
    outcome. A cleanup failure on an otherwise-committed transaction is reported
    as `Transaction` reading "the executable was replaced, but its staging
    directory … could not be removed" (`src/update.rs:1261-1268`) — the user is
    told plainly which half failed, because reporting plain success is the lie
    this replaced. On the *failure* path a cleanup error is written to stderr and
    the transaction error is still returned (`src/update.rs:1251-1259`).
12. Receipt (`src/update.rs:1269-1281`): `Committed` → success
    (`src/update.rs:1270`); `RolledBack` → `Receipt {"rolled_back"}`;
    `RecoveryRequired` → `Receipt {"recovery_required"}`.

### `LiveDigestVerifier` / `OwnershipVerifier`, precisely

The expected value is the SHA-256 of the live executable **as observed before the
transaction began** — `classify_provenance`'s `digest`
(`src/update.rs:511-512`), captured at step 3 of `run` and threaded into the map
at `src/update.rs:1231-1233`. `verify` (`src/update.rs:225-240`) maps eight
outcomes:

| Condition | `Ownership` | Consequence |
|---|---|---|
| Member not in the expected map (`src/update.rs:226-228`) | `Unknown` | refuse |
| `symlink_metadata` → `NotFound` (`src/update.rs:230`) | `Absent` | refused, via `AbsentPolicy::DenyCreate` (`src/update.rs:1236`) |
| Other I/O error (`src/update.rs:231`) | `Unknown` | refuse |
| **Is a symlink** (`src/update.rs:232`) | `Foreign` | refuse |
| Not a regular file (`src/update.rs:233`) | `Foreign` | refuse |
| `hash_file` == expected (`src/update.rs:235`) | `Owned` | commit proceeds |
| `hash_file` differs (`src/update.rs:236`) | `Foreign` | refuse |
| `hash_file` errors (`src/update.rs:237`) | `Unknown` | refuse |

The proof is: *the file is a regular file (not a symlink), still exists, and its
bytes still hash to what they hashed to before the download.* Eggup re-runs this
under its mutation lock, closing the observation-to-replacement window.
`AbsentPolicy::DenyCreate` closes the other direction: this updater may replace a
file it can prove it owns and may never create one.

### The identity check (C016) — fix present

`src/update.rs:1218-1220`:

```rust
let validator =
    ExactIdentityValidator::new(member_id.clone(), expected_identity(&plan.to_version))
        .args(["--version"]);
```

**Present**, and the comment at `src/update.rs:1206-1217` documents the defect in
full: with no arguments the binary runs its default routine scan, printing a
scan report to stdout and filesystem diagnostics to stderr — so an argv-less check
"compares stdout against a version string that can never match, and it scans the
whole machine on the way to failing." The same comment names why the suite
missed it: "the live commit path had never succeeded, and the fixture could not
see it because its candidate stub ignored argv and printed the identity for any
invocation" (`src/update.rs:1214-1217`). The release consequence is on the
record: v0.1.1 and v0.1.2 shipped an updater that could not complete a commit
(`plans/registry.md:219-220`).

The expected string is `expected_identity(v) = format!("cargo-cleanme {v}\n")`
(`src/update.rs:109-111`) — trailing newline included, matching `--version`
output exactly. This is the only check that the thing about to replace this
process is really us (`src/update.rs:1206-1208`).

### Atomicity of the replace

Verified against the dependency source `eggup-core-0.1.2` (version confirmed at
`Cargo.lock:461-463`):

1. The **old file is moved aside, not deleted**:
   `fs::rename(&destination, &path)` at
   `eggup-core-0.1.2/src/transaction.rs:583`, into `.eggup-backup-<nonce>-<nanos>`
   (`eggup-core-0.1.2/src/transaction.rs:723-726`) created in the installation
   root's parent.
2. The **new file is renamed over the destination**:
   `fs::rename(&staged, &destination)` at
   `eggup-core-0.1.2/src/transaction.rs:432`.
3. Eggup stages into a *sibling* of the install root —
   `.eggup-stage-<root>-<pid>-<seq>-<nanos>` at
   `eggup-core-0.1.2/src/stage.rs:196-215`, mode `0700` — moving bytes with
   `fs::copy` (`eggup-core-0.1.2/src/stage.rs:87`), not `rename`. That is why a
   source in `/tmp` on a different filesystem works.

**It is two renames, not one.** Each is atomic and same-filesystem (stage and
backup are both in the destination's parent), but the replacement is a sequence
with a window: between `transaction.rs:583` and `transaction.rs:432` the
destination path **does not exist**. A process killed in that window — SIGKILL,
power loss, OOM — leaves the installation with no `cargo-cleanme` at that path
and the previous binary intact inside `.eggup-backup-*`. Recoverable by hand, but
not atomic in the strict sense, and nothing in `src/update.rs` narrows or
documents the window.

Residue: the module's `cleanup` (`src/update.rs:1245`) removes only the temp
staging directory it created. `.eggup-stage-*` and `.eggup-backup-*` are
Eggup's, removed on the success path
(`eggup-core-0.1.2/src/transaction.rs:521`,
`eggup-core-0.1.2/src/stage.rs:192`) and after a *verified* rollback
(`eggup-core-0.1.2/src/transaction.rs:944`). A crash leaves them in the parent of
the install root — e.g. `/usr/local/.eggup-backup-*`.
`scripts/post-release-smoke.sh:209` checks only for `cargo-cleanme-update-*`
under `TMPDIR`, so it would not catch this.

### Rollback: delegated, not reimplemented

The `Cargo.toml:75-77` claim — "Generic acquisition, transport, integrity,
staging, and rollback are owned by the published Eggup crates, not reimplemented
here" — is **verified accurate**. This module contains no checksum computation,
no retry loop, no lock, and no backup/restore code. Rollback is `restore_entries`
in the dependency, renaming the backup back over the destination
(`eggup-core-0.1.2/src/transaction.rs:583` on the restore path) and reporting
`RolledBack` (`eggup-core-0.1.2/src/transaction.rs:925`) or, if that also fails,
`RecoveryRequired` (`eggup-core-0.1.2/src/transaction.rs:962`).
`src/update.rs:1269-1281` only *translates* those dispositions into
`UpdateError::Receipt`. What this module does own is rollback *policy*: refuse
rather than force.

`RecoveryRequired` also proves the guarantee is bounded. Eggup can report that it
could not put things back; this module surfaces that as a non-zero-exit
`AppError::Update` but does not attempt recovery, nor preserve the backup for
inspection.

### `--dry-run` guarantees

`check_only` returns at `src/update.rs:1059-1061`. The provenance gate is
*conditioned* on `!check_only` (`src/update.rs:1014`), so a dry run never trips
it, and it returns before `execute` (`src/update.rs:1077`). Guaranteed: no
staging directory, nothing downloaded beyond the metadata document, no file
written, no candidate executed, no lock taken. The plan is fully resolved, so a
dry run still proves version comparison and target resolution.
`a_dry_run_downloads_nothing` (`src/update.rs:2713`) asserts no release-asset URL
was requested; `scripts/post-release-smoke.sh:213-221` rehearses it live.

Consequence worth stating: a dry run on a **Cargo-managed** installation
*succeeds*, because the gate is explicitly skipped for `check_only`. The JSON's
`provenance` field carries `"cargo_managed"`
(`src/output.rs:923`, via `Provenance::code`), so the information is available,
but the exit code is 0. Only a real run refuses — and a real run refuses
*before* the network.

---

## 7. The Eggup transport decision

### Resolved dependency set

From `Cargo.lock`: `eggup-acquisition 0.1.2` (`Cargo.lock:455-457`),
`eggup-core 0.1.2` (`Cargo.lock:461-463`), `eggup-eggfetch 0.1.2`
(`Cargo.lock:470-472`), `eggfetch-core 0.2.2` (`Cargo.lock:412-414`).
Declared at `Cargo.toml:92-94`.

| Crate | Used here for | Reference |
|---|---|---|
| `eggup-acquisition` | The transport *seam*: `AcquisitionRequest`, `AcquisitionTransport`, `CancelFlag`, `FetchLimits`, `FetchOutcome`, `AcquisitionError` — all in `HttpEnvironment` | `src/update.rs:57-60`, `src/update.rs:874-881`, `src/update.rs:921-964` |
| `eggup-core` | All mechanics: `hash_file`, `parse_sha256_sidecar`, `Sha256Manifest`, `IntegrityRequirement`, `PermissionsIntent`, `InstallPlan::prepare`, `verify_integrity`, `ExactIdentityValidator`, `OwnershipVerifier`, `CommitOwnership`, `AbsentPolicy`, `TransactionReceipt` | `src/update.rs:61-66`, used at `src/update.rs:234`, `src/update.rs:511`, `src/update.rs:1107`, `src/update.rs:1175` |
| `eggup-eggfetch` | The production transport and its `User-Agent` seam | `src/update.rs:855`, `src/update.rs:863-866` |

### Why `eggup-eggfetch` and not the smaller `eggup-curl`

Documented at length in `Cargo.toml:79-91` and restated in the module header
(`src/update.rs:32-51`). The chain:

1. `eggup-curl` is smaller, because `eggup-acquisition` has zero dependencies
   (`Cargo.toml:80`) — avoiding an embedded HTTP/TLS stack in a CLI that
   self-updates rarely is a real advantage, and it was the first choice.
2. `CurlConfig` in `eggup-curl` 0.1.2 exposes **no User-Agent seam**, and the
   adapter clears the child environment (`Cargo.toml:81-83`), so every request
   goes out as `curl/x.y`.
3. crates.io answers **403** to any non-descriptive User-Agent
   (`Cargo.toml:82-84`, `src/update.rs:39-40`), so the curl transport could not
   read the version authority *at all*, on any host.
4. "A transport that cannot reach the version authority is not qualified,
   regardless of its size" (`Cargo.toml:84-85`).
5. `EggfetchConfig::user_agent` is the seam the requirement needs
   (`Cargo.toml:85-86`, `src/update.rs:44-47`).

**The cost, stated rather than assumed:** an embedded HTTP/TLS stack —
`eggfetch-core` + rustls + tokio (`Cargo.toml:88-89`), with `eggfetch-core 0.2.2`
and `eggfetch-http-connect 0.2.2` confirmed at `Cargo.lock:412-414`,
`Cargo.lock:444-446`. `Cargo.toml:89-90` says "The measured delta is recorded in
the M010C closure record" — **I did not open that record, so I cannot state the
measured figure.** The closure record for C011 does state one:
5,420,664 → 12,125,624 bytes and 82 → 177 normal dependencies
(`plans/closure/distribution-release-update/c011-status.md:34-36`).

The defect was found by the first live release smoke, not by the suite
(`src/update.rs:41-42`), corroborated by the M010C closure record: the released
binary's `update --dry-run` against the live registry failed with
`Transport("HTTP 403 ...")`
(`plans/closure/distribution-release-update/010c-status.md:115-117`).

### Exactly one transport — verified

`Cargo.toml:92-94` declares `eggup-acquisition`, `eggup-core`,
`eggup-eggfetch`. `eggup-curl` is **not** a dependency and appears nowhere in
`src/`. The claim at `Cargo.toml:90-91` — "Exactly one production transport is
wired; the curl path is not kept as a fallback, because it cannot perform the
required transaction" — is **accurate**. There is no transport `match` anywhere
in the module.

### C010: the bounded upstream request

`CurlConfig::user_agent` seam on `eggup-curl`, status `proposed`
(`plans/registry.md:119`), and "it cannot be closed from this repository, and
nothing here waits on it" (`plans/registry.md:84-85`).

If upstream accepted it, this crate could drop `eggup-eggfetch` and the
`eggfetch-core`/rustls/tokio subtree, constructing `eggup_curl` with a
descriptive agent instead. Two costs would remain: loss of
`EggfetchConfig::strict()`'s bounded-redirect and explicit-proxy behaviour
(`src/update.rs:860-862`), and reintroduction of a subprocess dependency
(`curl` on `PATH`) for a code path whose whole point is recovering a broken
installation. The migration would be a transport swap behind the
`UpdateEnvironment` seam, not a rewrite — the practical payoff of that seam
existing.

**Registry consistency, re-checked.** The previous revision of this document
flagged that `plans/registry.md` contradicted itself: stale summary rows for
C014/C016/C017 against a status list that said all were closed. That defect is
**gone** — the registry was restructured by C025 and C026
(`plans/registry.md:108-126`), and the current rows for C016, C017, C018, C010,
M011A, M011B and M011C are each internally consistent with their closure
records. One residual inconsistency remains, and it is smaller: the M011C row
still says the automatic trigger is "demonstrated three times"
(`plans/registry.md:123`), while the release-evidence table below it records
**seven** green automatic smokes
(`plans/registry.md:182-190`). The table is the newer and more specific
evidence; the row is stale. A documentation defect, not a code one.

---

## 8. Invariants and edge cases

Each item is a verified finding, not an assumption.

### Can a Cargo-managed binary ever be replaced?

**Once classified as such, no — provably not**, via three gates: `run` refuses
anything but `VerifiableSelfManaged` before the registry is contacted
(`src/update.rs:1014-1016`); `execute` re-checks
(`src/update.rs:1085-1092`); and the commit needs `Ownership::Owned`, which a
Cargo-managed file still satisfies. So the protection lives entirely in
classification, which is correctly stated rather than redundantly re-derived.

**The guarantee is exactly as strong as the classifier, and C018 is what made
the classifier fail closed.** The rule, stated exactly as the code states it:

> A Cargo installation root is read as *typed evidence*, never as a single
> optional string. `Recorded` is the only state that grants ownership;
> `Uninterpretable` — a record that exists and could not be read — is
> uncertainty and is always `UnprovableOwnership`; `Absent` is the only state
> that lets the ancestor walk continue.

The four fail-open classes this review originally recorded were:

1. `.crates.toml` absent, unreadable, or unparseable at the owning root.
2. The owning root is more than `CARGO_ROOT_SEARCH_DEPTH = 4` ancestors above the
   executable.
3. The executable's file name is not valid UTF-8.
4. The record's value is not a TOML array, or the spec string lacks `')'`.

**Classes 1, 3, and 4 are closed**; class 2 was characterised and bounded rather
than closed.

- **Class 1** — closed by the typed `CargoRecord`. An unreadable or malformed
  record at the *explicit* Cargo home is `UnprovableOwnership`
  (`src/update.rs:458-471`); an unreadable record at an *ancestor* root stops
  the walk with `UnprovableOwnership` (`src/update.rs:499-508`).
- **Class 3** — closed by the non-UTF-8 short-circuit
  (`src/update.rs:415-422`), which fires before the ancestor walk can read the
  resulting non-match as "Cargo records nothing".
- **Class 4** — closed by the parser: a malformed entry that names this package
  sets `saw_malformed_matching_package` (`src/update.rs:699-725`) and the
  function returns `Uninterpretable` (`src/update.rs:733-741`) rather than
  skipping the entry.
- **Class 2** — bounded. C018 ran real Cargo 1.89.0 (the MSRV) and 1.99.0 (the
  release builder) against `cargo install --root --locked`, and both wrote an
  identical `DIR/.crates.toml` plus `DIR/.crates2.json` beside `DIR/bin/<exe>`
  (`plans/closure/distribution-release-update/c018-status.md:41-42`). In every
  observed layout the record sits in the executable's *parent*, one level from
  the executable — the bound of 4 exists to survive container and hermetic
  nesting, and the source now says so explicitly: "The bound cannot miss a
  supported layout, and that is now checkable rather than asserted"
  (`src/update.rs:534-539`; the constant is `src/update.rs:540`). The closure
  evidence is `the_cargo_root_search_cannot_miss_a_supported_layout`
  (`src/update.rs:2165`), which builds each supported shape and proves the walk
  reaches it. The honest statement of what the bound costs is that it *could*
  miss a root five levels above; the evidence says Cargo does not write one
  (`src/update.rs:530-532`).

**A fourth closure, later than the other three.** One further case now refuses
outright rather than falling through: a **symlinked executable**
(`src/update.rs:427-434`). The symlink case matters because
`LiveDigestVerifier` would have refused the commit anyway
(`src/update.rs:232`) — but it would have done so *after* the download, and the
refusal would have been a `Transaction` rather than a classification.

**The asymmetry is unchanged and still deliberate:** over-detection costs a
refusal that names the manager command, under-detection costs a user's Cargo
install.

### Is integrity verified before replace?

**Yes, three times over.** (1) SHA-256 of the acquired artifact against the
digest in the release's own `<asset>.sha256` — `IntegrityRequirement::Sha256`
(`src/update.rs:1175`), enforced by `verify_integrity()`
(`src/update.rs:1200-1204`). (2) The sidecar must name `plan.asset`
(`src/update.rs:1121-1143`). (3) The candidate must identify itself as
`cargo-cleanme <to_version>\n` (`ExactIdentityValidator`,
`src/update.rs:1218-1229`).

**The trust root is HTTPS to GitHub, not a signature.** The expected digest comes
from a file served by the same host as the artifact it describes
(`release_asset_url`, `src/update.rs:158-160`). No signature, no pinned key, no
independent digest channel. Integrity against corruption, truncation, and a
mismatched sidecar is solid; integrity against a *compromised release host* is
delegated entirely to TLS — a bound to state rather than imply. Pinned by
`a_checksum_mismatch_leaves_the_live_binary_untouched`
(`src/update.rs:2454`) and `a_verified_candidate_replaces_the_live_binary`
(`src/update.rs:2442`).

### A read-only or unwritable install location

**Fails cleanly, before any rename.** `prepare()` creates a stage directory in
the *parent* of the install root
(`eggup-core-0.1.2/src/stage.rs:196-215`, mode `0700`) and copies bytes into it
(`eggup-core-0.1.2/src/stage.rs:87`), so for `/usr/bin` an unprivileged process
fails at `src/update.rs:1194-1199` with `Transaction`. Even if staging
succeeded, the mutation lock is `.eggup-mutation.lock` **in the install root**
(`eggup-core-0.1.2/src/lock.rs:54`) and `require_ready_parent` re-checks the
destination's parent
(`eggup-core-0.1.2/src/transaction.rs:788-806`). A read-only filesystem, full
disk, or permission-denied target therefore all surface as `Transaction` at the
staging step, with `cleanup` running unconditionally at `src/update.rs:1245`
and the live binary untouched.

### Symlinks

`LiveDigestVerifier` returns `Ownership::Foreign` for a symlinked destination
(`src/update.rs:232`) — the commit is refused. Independently,
`revalidate_destination` requires the canonicalised ancestor under the
canonicalised install root and the destination to be a regular file
(`eggup-core-0.1.2/src/transaction.rs:743-765`), and `require_ready_parent`
rejects a symlinked *parent*
(`eggup-core-0.1.2/src/transaction.rs:802-806`). And the classifier now refuses
a symlinked running executable outright, before the registry is consulted
(`src/update.rs:427-434`). One residual: `current_exe` uses
`std::env::current_exe()` (`src/update.rs:885-889`), which on Linux already
returns a resolved path, so the hazard is handled before it arrives.
`ownership_is_never_inferred_from_the_file_name` (`src/update.rs:2406`) covers
the classifier's half. **No path here follows a symlink to replace a file the
user did not intend.**

### Concurrency

**Serialized within one installation root.** Eggup takes
`<install_root>/.eggup-mutation.lock` (`eggup-core-0.1.2/src/lock.rs:54`) and
re-runs the ownership verifier under it immediately before backup/rename
(`revalidate_ownership_locked`,
`eggup-core-0.1.2/src/transaction.rs:624-640`). Staging names carry pid +
nanoseconds (`src/update.rs:906-911`), so concurrent runs do not collide there.
No test here exercises concurrency; the property is inherited from `eggup-core`,
and I read only the lock path construction, not its implementation.

### Temp-file residue on failure

The module's own temp directory is removed unconditionally at
`src/update.rs:1245` on both paths, because the transaction body is a closure
whose result is bound before `cleanup`; a failure to remove it is now reported
rather than swallowed. Covered by
`staging_is_cleaned_up_on_success_and_on_failure` (`src/update.rs:2572`) and
`a_staging_cleanup_failure_is_reported_not_swallowed`
(`src/update.rs:2603`), and live, `scripts/post-release-smoke.sh:209`. Eggup's
`.eggup-stage-*` / `.eggup-backup-*` directories are removed on success and
after a verified rollback, but a hard crash mid-commit leaves them in the
install root's parent. Nothing here, and nothing in the smoke script, asserts
their absence.

### Rollback if the new binary is non-functional

**None.** The candidate is executed *before* the commit
(`src/update.rs:1218-1229`) to prove identity, but that proves only that it
prints the right string — not that a scan, config load, or filesystem walk
still works. Once `fs::rename(staged, destination)` succeeds
(`eggup-core-0.1.2/src/transaction.rs:432`), the only outcomes are `Committed`
(`src/update.rs:1270`) or a receipt-derived error. There is no post-commit
validation, no smoke run of the new binary in place, and no
`commit_with_post_commit` use — the dependency offers that API
(`eggup-core-0.1.2/src/candidate.rs:465`) with a
`PostCommitFailurePolicy::RollBack` arm, and **this module does not use it.**

This is the real bound on the guarantee: the transaction proves *identity and
integrity before replacing*, not *function after replacing*. Recovery is manual —
`cargo install --locked --force` (`src/update.rs:198`) or the installer link
(`src/update.rs:203-206`).

### Yanked or unpublished versions

Handled by field choice, not an explicit check: `crate.max_stable_version`
excludes yanked releases, and `is_release_version` (`src/update.rs:799`) rejects
anything not plain `X.Y.Z`, which is also what rejects a prerelease. No separate
yanked check exists; the doc comment mentions "a prerelease or a yanked
version" (`src/update.rs:751-752`) without a distinct yanked path — the comment
is slightly looser than the code.

### Downgrade or wrong-target installation

- **Downgrade:** impossible via the registry (§4, `src/update.rs:1028-1036`).
  The comparison is against `env!("CARGO_PKG_VERSION")`
  (`src/update.rs:895-897`) — the version compiled into this binary — so a binary
  whose manifest disagrees with its compiled version is compared against the
  compiled one, correct for a shipped artifact.
- **Wrong target:** the asset name comes from a compile-time table keyed by the
  host triple (`src/update.rs:120`, `src/update.rs:145`) and the URL is
  constructed, not followed (`src/update.rs:158-160`,
  `src/update.rs:1054-1055`), so no release *response* can select the artifact.
  What is not verified here is that the *hosted* asset for a triple is really
  the binary for that triple — that is `scripts/check-release-contract.py` and
  the Eggpack contract's job.
  `the_published_target_table_matches_the_eggpack_contract`
  (`src/update.rs:2730`) pins the table, but only if that script runs in CI,
  which I did not verify.

### `HOME` / `CARGO_HOME` unset

`default_cargo_home` (`src/update.rs:391-395`) returns `CARGO_HOME`, else
`$HOME/.cargo`, else `None`. With `None`, step 1 is skipped
(`src/update.rs:441`) but **step 2 still runs**: the ancestor walk derives
`/home/u/.cargo/bin` from the executable's own path, so a plain `cargo install`
is still detected. Losing both variables degrades detection to the ancestor walk
alone — which is why that walk exists, since it is the layout "every hermetic
install script, CI job, and container image uses", and where the process
`CARGO_HOME` "is frequently *not* where the running binary came from"
(`src/update.rs:476-479`). It does not fail open in the common case.

### Verified comment/code drift

Both items the previous revision of this document recorded as drift have been
**fixed in the source**, and the comments are now accurate:

- "Refuse before acquiring anything" — **now true for the mutating path.** The
  gate at `src/update.rs:1014-1016` is above the metadata fetch at
  `src/update.rs:1018`, and the comment above it says so
  (`src/update.rs:998-1013`). The caveat is that it is read against the
  *mutating* path only: `--dry-run` deliberately still fetches, because
  reporting a candidate is the point of a dry run. See
  `a_dry_run_still_reports_a_candidate_and_names_the_real_provenance`
  (`src/update.rs:2244`).
- `staging_dir` "never overwrites an existing path, it fails instead" — **now
  true as written.** The implementation is `std::fs::create_dir`
  (`src/update.rs:912`), not `create_dir_all`, and the comment names exactly
  why (`src/update.rs:900-905`): "`create_dir` — not `create_dir_all`, which
  *succeeds* on an existing path — makes the comment literal".

One item remains: the dead binding `let _ = target;` at `src/update.rs:409`. The
target is recomputed independently at `src/update.rs:988`.

---

## 9. Testing

### Real numbers

| Metric | Value | Method |
|---|---|---|
| Test module starts | line 1290 (`#[cfg(test)]` at `src/update.rs:1290`, `mod tests` at `src/update.rs:1291`) | direct read |
| Production lines | 1289 (1–1289) | 1290 − 1 |
| Lines of test code | 1596 (1291–2886) | 2886 − 1291 + 1 |
| `#[test]` functions | **47** | `grep -cE '^\s*#\[test\]' src/update.rs` |
| `tests/` integration tests referencing `update` | **3** | `grep -nE '^\s*fn .*update' tests/cli_contract.rs` |

`tests/cli_contract.rs` has 63 `#[test]` functions and `tests/end_to_end.rs` has
4. **The update path is no longer untested at the integration level**: three
cases in `tests/cli_contract.rs` now drive it —

- `update_dry_run_reaches_the_same_non_mutating_path_from_either_spelling`
  (`tests/cli_contract.rs:2438`) — both spellings resolve to the same
  non-mutating updater request.
- `json_update_refusal_emits_no_document_and_exits_non_zero`
  (`tests/cli_contract.rs:3260`) — drives the **real binary** staged under a
  Cargo root with a verbatim `v1` record (`tests/cli_contract.rs:3233-3248`) and
  asserts exit 2 and no document on stdout. Its doc comment
  (`tests/cli_contract.rs:3250-3258`) is the reason this is meaningful: the
  mutating refusal happens *before* the registry is asked anything, so the case
  is hermetic and cannot pass for an unrelated network failure.
- `the_json_update_branch_prints_only_the_tested_seam`
  (`tests/cli_contract.rs:3311`) — pins that the JSON branch of `run_update`
  prints exactly `output::update_json_stream` and nothing else, so the
  library's one-document/one-newline guarantee becomes a statement about the
  process's stdout.

The previous revision of this document claimed **zero** integration coverage of
`update` and that "not one" of the integration tests mentioned it. That was
true when written and is now false; the three cases above are the correction.

### The 47 tests, grouped by what they protect

| Group | Count | Tests (line) | Protects |
|---|---|---|---|
| **Provenance classification** | 20 | `the_crates_toml_we_parse_is_the_one_cargo_writes` (1816), `a_cargo_root_install_is_refused_even_when_it_is_not_the_cargo_home` (1842), `cargo_managed_installation_is_refused_with_the_manager_command` (1866), `an_unrecorded_file_in_a_cargo_root_is_not_ours_to_replace` (1902), `unreadable_cargo_metadata_can_never_become_self_managed` (1961), `a_malformed_cargo_cleanme_record_is_not_skipped_into_self_managed` (1976), `an_unrelated_malformed_entry_does_not_hide_a_valid_record` (1993), `a_cargo_record_in_an_unsupported_shape_is_uncertain_not_absent` (2015), `an_explicit_cargo_home_bin_with_a_missing_record_is_unprovable` (2026), `an_ancestor_cargo_root_with_unreadable_metadata_is_unprovable` (2042), `a_non_utf8_executable_name_in_a_cargo_root_is_unprovable` (2056), `a_non_utf8_executable_name_under_an_ancestor_cargo_root_is_unprovable` (2088), `a_symlinked_executable_is_unprovable_not_self_managed` (2124), `a_genuinely_self_managed_installer_layout_stays_self_managed` (2147), `the_cargo_root_search_cannot_miss_a_supported_layout` (2165), `the_published_installer_layout_is_not_mistaken_for_a_cargo_root` (2298), `a_cargo_record_that_omits_this_binary_does_not_claim_this_file` (2334), `a_malformed_crates_toml_entry_does_not_hide_the_entry_after_it` (2359), `a_self_managed_installation_is_proven_by_its_digest` (2396), `ownership_is_never_inferred_from_the_file_name` (2406) | The C017 fix from both directions, and C018's fail-closed rule. Most carefully written: `an_unrecorded_file_in_a_cargo_root_is_not_ours_to_replace` (1902), whose comment explains a lenient fixture would prove nothing — with an irrelevant record both the correct and the broken answer are "self-managed" — so it plants the one near miss that could cause a wrongful claim. |
| **Refusal before the registry** | 3 | `a_mutating_run_on_forbidden_provenance_makes_zero_registry_requests` (2212), `a_dry_run_still_reports_a_candidate_and_names_the_real_provenance` (2244), `a_recorded_cargo_root_beyond_the_cargo_home_is_still_refused_before_the_registry` (2274) | C018's *ordering* fix. The first asserts zero registry requests on a mutating refusal; the second pins the deliberate exception for `--dry-run`. |
| **Version authority & comparison** | 9 | `version_authority_accepts_only_plain_releases` (1688), `version_authority_reads_exactly_one_field` (1708), `version_authority_rejects_prerelease_and_junk` (1715), `the_release_tag_is_constructed_not_scraped` (1732), `already_current_is_not_an_error_to_hide` (1744), `a_registry_outage_is_reported_not_guessed` (1757), `versions_order_numerically_not_lexically` (2777), `a_newer_local_build_is_never_downgraded` (2801), `an_older_published_version_is_not_offered_to_a_matching_build` (2817) | Narrow authority, numeric comparison, constructed tags, outage as error not guess, and the no-downgrade guard. |
| **Integrity** | 5 | `a_verified_candidate_replaces_the_live_binary` (2442), `a_checksum_mismatch_leaves_the_live_binary_untouched` (2454), `a_candidate_reporting_the_wrong_version_is_rejected_before_commit` (2482), `a_sidecar_naming_a_different_asset_is_rejected` (2497), `an_absent_release_asset_is_not_replaced_with_anything` (2519) | `a_checksum_mismatch_leaves_the_live_binary_untouched` is the most significant test in the module: it is the one proving a bad candidate cannot become the installed binary. |
| **Transaction hygiene** | 5 | `staging_is_cleaned_up_on_success_and_on_failure` (2572), `a_staging_cleanup_failure_is_reported_not_swallowed` (2603), `a_deliberately_leaked_staging_path_is_still_reported` (2655), `a_foreign_staging_directory_is_not_evidence_of_a_leak` (2686), `a_dry_run_downloads_nothing` (2713) | No residue, on evidence bound to the fixture's **own** staging path rather than a scan of the temp directory. The leak pair is C021's controls: the first proves the check can still fail, the second proves it ignores a concurrent sibling's live path. Without both, the check is indistinguishable from the process-global scan it replaced. |
| **Transport & release contract** | 5 | `the_published_target_table_matches_the_eggpack_contract` (2730), `the_production_transport_identifies_itself_descriptively` (2828), `the_version_authority_url_is_https_and_product_scoped` (2862), `release_urls_are_constructed_from_the_tag_and_contracted_asset` (2869), `the_host_target_has_a_contracted_asset` (2879) | `the_production_transport_identifies_itself_descriptively` is the C011 guard and the only test touching real transport construction. |

20 + 3 + 9 + 5 + 5 + 5 = **47 — every `#[test]` in the file is named above.**

### `FixtureEnvironment` (`src/update.rs:1301-1550`)

**What it fakes:** executable path, Cargo home, current version, the staging
directory (real on disk, via `unique()` at `src/update.rs:1556`), `fetch` (writes
canned bytes to the destination, returns the byte count), and `fetch_metadata`
(canned bytes). Three response modes — `respond` (`src/update.rs:1362`),
`fail_transport` (`src/update.rs:1372`), `absent` (`src/update.rs:1457`) — plus
`requested` (`src/update.rs:1341`), asserted at `src/update.rs:2713`.

**What it cannot fake:**

1. **The network.** No User-Agent, TLS, redirects, proxies, status codes, or
   crates.io 403 policy. This is the C011 gap exactly
   (`plans/registry.md:272-284`).
2. **`env!("CARGO_PKG_VERSION")`.** `current_version` returns whatever the test
   declares (`src/update.rs:1478-1479`), so a test can claim to be any version
   without the binary being that version — the no-downgrade tests depend on
   this (`src/update.rs:2801`).
3. **argv.** The candidate stub ignored argv and printed the identity for *any*
   invocation, which is precisely why C016 survived. The source now records
   this at `src/update.rs:1213-1217`.
4. **A real filesystem for provenance.** `run` calls `classify_provenance_in`
   with the fixture's own `exe` and `cargo_home` (`src/update.rs:986`), so
   classification *is* exercised, but against invented paths. Real layouts —
   symlinked `~/.cargo`, relocated `CARGO_HOME`, root-writable `/usr/local/bin` —
   are not modelled.
5. **Concurrency, permissions, read-only filesystems, full disks.** Nothing in
   the fixture can make a write fail.

### Honest gaps

The registry's own evidence: this module shipped two defects no test caught
(C016, C017) and a whole corrective (C011) was green for the wrong reason
(`plans/registry.md:272-284`). Concretely, these would all pass:

1. **Any change to real transport wire behaviour.** Dropping `.user_agent(...)`
   (`src/update.rs:864`) fails only if the constant changes too. Setting the
   agent to something the registry *rejects* — a bare token with no `/` or
   space, or a UA blocked by policy — passes everything, because no test models
   registry policy (`src/update.rs:2838-2841`). This is C011 re-entering
   through a different door.
2. **A provenance hole in the one *open* fail-open class in §8** — the
   `CARGO_ROOT_SEARCH_DEPTH` bound. No test builds an owning root beyond the
   bound and shows it is refused, because the closure evidence characterises
   the bound rather than closing it.
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

**Yes — automatically, on every published release.** This is the single largest
change to this document's picture, and it is the direct consequence of M011C.

- `tests/cli_contract.rs` and `tests/end_to_end.rs` exercise the *refusal* and
  *CLI* paths hermetically, never the network.
- `scripts/smoke-release-candidate.py`: a digest-computing validator over release
  bytes; no `update` invocation.
- The real transport is exercised by `scripts/post-release-smoke.sh` — "Real,
  networked self-update rehearsal against two *public* releases"
  (`scripts/post-release-smoke.sh:2`), "Work Package F of C014"
  (`scripts/post-release-smoke.sh:4`), against the *public* updater rather than a
  freshly built copy (`scripts/post-release-smoke.sh:6-8`). It rehearses a
  self-managed update (`scripts/post-release-smoke.sh:184-198`), a dry run
  (`scripts/post-release-smoke.sh:213-221`), and a Cargo-managed refusal
  asserting the exact remediation string
  (`scripts/post-release-smoke.sh:257-275`).
- That script is now the body of a **GitHub Actions workflow**,
  `.github/workflows/post-release-smoke.yml`, with two entry points
  (`.github/workflows/post-release-smoke.yml:8-15`): `release: published` —
  **automatic** — and `workflow_dispatch`, the explicit rehearsal/fallback
  surface with an exact from/to pair chosen by a human. The workflow resolves
  the transition **once, centrally**
  (`.github/workflows/post-release-smoke.yml:49-58`) so all five lanes provably
  test the same pair, and runs them `fail-fast: false`
  (`.github/workflows/post-release-smoke.yml:136-153`) across the five
  contracted targets.

**The recorded history.** The automatic trigger fired on v0.2.0
(`37419183947`) and v0.2.1 (`37507738147`) and **both failed**; the failures
stay recorded as failures (`plans/registry.md:123`,
`plans/closure/distribution-release-update/m011c-status.md:327-329`). The first
green automatic run was v0.2.2's `37561575727` — "the first green automatic smoke
in the project's history" (`plans/registry.md:116`). Every release since is
recorded green on all five lanes (`plans/registry.md:182-190`):

| Release | Automatic smoke | Result |
|---|---|---|
| `v0.2.2` | `37561575727` | green on all five targets — the first green automatic run |
| `v0.2.3` | `37652257315` | green on all five targets — the fourth consecutive demonstration of the automatic trigger (`plans/registry.md:142-144`) |
| `v0.2.4` | `37850582892` | green on all five targets |
| `v0.2.5` | `37988595432` | green on all five targets |
| `v0.2.6` | `38303253453` | green on all five targets |
| `v0.2.7` | `38454264585` | green |
| `v0.2.8` | `38683755836` | green |

So the honest answer to the question this section used to answer with "no" is
now: **the real transport and the real commit path are exercised automatically
on every publication, and manually on demand.** That rehearsal found C016 and
C017; nothing else in the repository would have found either. It is still
inherently lagging — a fix cannot be validated by it until a release carries the
fix — which is why C016 and C017 sat at `ready` for releases, and why
`v0.1.1`/`v0.1.2` are recorded as shipping an updater that cannot complete a
commit (`plans/registry.md:219-220`).

---

## 10. Review checklist

Concrete checks, each anchored to a line observed. Priority reflects blast radius.

1. **Provenance misdetection.** Confirm every non-`VerifiableSelfManaged` result
   of `classify_provenance_in` is actually refused before any rename, and that a
   mutating refusal happens *before* the registry is contacted
   (`src/update.rs:1014-1016` above `src/update.rs:1018`). The three closed
   classes are regression-tested individually
   (`unreadable_cargo_metadata_can_never_become_self_managed`,
   `a_malformed_cargo_cleanme_record_is_not_skipped_into_self_managed`,
   `a_non_utf8_executable_name_in_a_cargo_root_is_unprovable`,
   `a_mutating_run_on_forbidden_provenance_makes_zero_registry_requests`), and
   the one open class — the `CARGO_ROOT_SEARCH_DEPTH` bound — is characterised
   by `the_cargo_root_search_cannot_miss_a_supported_layout` against real Cargo
   layouts rather than asserted in a comment. Re-read the *unhappy* ordering:
   a test that only checks the classification would still pass if the gate
   were moved back after the registry fetch. The two short-circuits
   (`src/update.rs:415-422`, `src/update.rs:427-434`) are newer than most of
   this list and are the easiest to lose in a refactor.
2. **Integrity is checked before replace, against what.** Confirm
   `IntegrityRequirement::Sha256(*manifest.digest())` is attached to the member
   (`src/update.rs:1175`) and that `verify_integrity()`
   (`src/update.rs:1200-1204`) runs *before* `validate`
   (`src/update.rs:1221-1229`) and *before* `commit` (`src/update.rs:1236`).
   Then confirm the trust root: the expected digest comes from a sidecar served
   by the same host as the artifact (`src/update.rs:158-160`,
   `src/update.rs:1096-1100`) — no signature, no pinned key. Any change letting a
   digest arrive from anywhere else is a critical regression.
3. **The sidecar cross-check.** `src/update.rs:1121-1143` must refuse when
   `manifest.filename() != plan.asset` — and must also refuse a sidecar that
   names no file at all, which skips the binding entirely. Removing it is how a
   sidecar from another release would authorize arbitrary bytes.
4. **No-downgrade.** `src/update.rs:1028-1036` must remain; verify the `Less` arm
   returns `NewerThanPublished` and the comparison source is
   `env!("CARGO_PKG_VERSION")` (`src/update.rs:895-897`), not anything scraped at
   runtime.
5. **Comparison must stay numeric.** `src/update.rs:784-795` parses to
   `[u64; 3]` and compares tuples. Any refactor toward string comparison — or
   toward a general semver crate that would accept and then order prereleases —
   changes a proven property. `is_release_version` (`src/update.rs:799-808`) is
   the prerelease gate; it must reject `1.2.3-rc.1` and `1.2.3+build`.
6. **Atomicity and the crash window.** Verify the old file is *renamed aside*
   into a backup, not deleted, and the new file renamed over it
   (`eggup-core-0.1.2/src/transaction.rs:583` and
   `eggup-core-0.1.2/src/transaction.rs:432`). Then record honestly
   that this is two renames with an absent-destination window which this module
   neither narrows nor documents. Check `cleanup` (`src/update.rs:1245`) is
   still on the unconditional path via the closure
   (`src/update.rs:1095-1240`).
7. **Identity invocation.** `src/update.rs:1220` must keep
   `.args(["--version"])`. This is the C016 fix; dropping it makes the identity
   check unmatchable *and* triggers a full filesystem scan inside the commit
   path (`src/update.rs:1206-1217`).
8. **The `User-Agent` seam.** `src/update.rs:863-865` must keep
   `.user_agent(USER_AGENT)`, and `USER_AGENT` (`src/update.rs:846-850`) must
   stay a `concat!` naming the product. This is the C011 defect, and the fixture
   suite cannot catch its recurrence (`src/update.rs:41-42`).
9. **Exactly one transport.** `Cargo.toml:92-94` should still list exactly
   `eggup-acquisition`, `eggup-core`, `eggup-eggfetch`, with no `eggup-curl`
   fallback reintroduced (`Cargo.toml:90-91`). C010 (`plans/registry.md:119`)
   is upstream-only; a local curl path would be a regression, not a partial fix.
10. **Fixture-vs-reality.** Classify every new test before writing it: does it
    exercise `HttpEnvironment`, or only the fixture? `src/update.rs:2828` is the
    model — real transport construction (`src/update.rs:2842-2843`) plus an
    explicit statement of what it cannot prove (`src/update.rs:2838-2841`).
    Confirm `scripts/post-release-smoke.sh` still rehearses both directions: a
    real update (`scripts/post-release-smoke.sh:184-198`) and a Cargo-managed
    refusal with the exact remediation string
    (`scripts/post-release-smoke.sh:257-275`).
11. **Integration coverage.** The updater now has three integration cases
    (`tests/cli_contract.rs:2438`, `tests/cli_contract.rs:3260`,
    `tests/cli_contract.rs:3311`), but none of them reaches the network or the
    commit path. A hermetic *successful* update is not representable: every
    success path resolves the version authority from crates.io and then
    replaces bytes (`tests/cli_contract.rs:3302-3309`). The cheapest real
    improvement is therefore an **environment override for the version
    authority** — the updater has none (`crates_io_metadata_url`,
    `src/update.rs:154-156`, is a hard-coded const format), unlike
    `packaging/install.sh:38-39`. That asymmetry is itself a finding, and it
    is now the main blocker on a hermetic success-path test.
12. **Documentation drift.** Re-check the "refuse before acquiring anything"
    claim (`src/update.rs:998-1013` vs the gate at `src/update.rs:1014-1016`
    and the fetch at `src/update.rs:1018`), the "it fails instead" claim
    (`src/update.rs:900-905` vs `create_dir` at `src/update.rs:912`), the dead
    binding at `src/update.rs:409`, and the M011C registry row
    (`plans/registry.md:123`) against the release-evidence table
    (`plans/registry.md:182-190`).
