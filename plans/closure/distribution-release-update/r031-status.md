# cargo-cleanme v0.2.11 — release record

Release record for the **nineteenth** published release. It carries **M018**
(the proof-scoped Full cleanup gate) and **M019** (ADR 008 — the canonical
root-front-door spellings) to the public, and it closes **C042**, **C043**,
**C044**, **C045** and **C046**, all of which were implemented and
ladder-green but unpublished before this release.

| | |
|---|---|
| Version | 0.2.11 |
| Tag | `v0.2.11` at `1d679833ae6267b78a8ca0b3560bced7537aca4c` |
| GitHub release | published 2026-10-25T17:23:10Z, immutable, 15 assets, release id `423547384` |
| crates.io | published 2026-10-25T17:23:45Z, sha256 `96298f832e01c6289488f3c4869f6b06596f1986aace7523bba72c17a126b9d2`, not yanked |
| Automatic smoke | `40216211513` — see §7 |
| Closed by this release | **M018**, **M019**, **C042**, **C043**, **C044**, **C045**, **C046** |
| Implementation commits | `c74f3f2`/`445fcaf` (C042), `30bcff5`/`05f8cd7` (C043), `c9f7784` (C040), `462a996` (C038), `382192d` (C039), plus `570b6f4` (the bugs.md interrogation), `38f2759` (the platform-test fixes), `de0e9e0` (the xtask CRLF fix), `1545ab3` and `1d67983` (the documentation corrections) |

Prior record: `r030` (v0.2.10). The v0.2.7 and v0.2.8 receipts are still
missing; that debt is recorded in the registry and is **not** reopened here.

## 1. Order of operations, as performed

| # | Step | Evidence |
|---|---|---|
| 1 | The release content landed on `main` | `660325b` (the corpus re-derivation), `f54708a` (the release commit) |
| 2 | Three pre-existing hosted-lane defects were found and fixed | `38f2759` (four Linux-shaped path expectations), `de0e9e0` (the generated-artifact gate compared CRLF as content) |
| 3 | The release's **own** documentation was re-checked against the binary | `1545ab3` (two underived counts, a malformed register row, the registry's published state), `1d67983` (a false `--known` claim) |
| 4 | The version was bumped to 0.2.11 | `Cargo.toml`, `Cargo.lock`; `## [0.2.11] - 2026-10-25]` in `CHANGELOG.md` |
| 5 | The generated artifacts were regenerated | 8 man pages carry the version; `generate-docs -- --check` is green |
| 6 | **The complete local gate** | `bash scripts/release-check.sh v0.2.11` → exit 0, "release-check: passed; no publication was performed" |
| 7 | The tag was created and the gate re-run **with** it | `check-release-identity.py --tag v0.2.11` → "v0.2.11 names 0.2.11 at 1d679833ae62", exit 0 |
| 8 | The branch and the tag were pushed | `git push origin main`, `git push --force origin v0.2.11` |
| 9 | CI on the release commit | `40213835658` and `40213834218` (push) — all nine jobs green on both; `40213834203` (the drift guard) and `40213835646` (the selector qualification) green |
| 10 | The Eggpack candidate build was dispatched against the exact tag | `gh workflow run release-binaries.yml -f release_tag=v0.2.11` → run `40214164767`, all 20 jobs green |
| 11 | The staged draft was inspected | 15 assets, every digest recomputed locally — §3 |
| 12 | The staged-release validation was inspected and awaited | run `40216008270` — green; its retained log is quoted in §3 |
| 13 | The draft was published, with curated notes | `gh release edit v0.2.11 --draft=false --title "cargo-cleanme v0.2.11" --notes-file …` → published 2026-10-25T17:23:10Z, immutable |
| 14 | The crate was published from a **detached worktree of the tag** | `git worktree add --detach /tmp/cleanme-publish v0.2.11` → `cargo publish --locked` → "Published cargo-cleanme v0.2.11 at registry `crates-io`"; the worktree was removed afterwards |
| 15 | The attestation was verified, twice | §6 |
| 16 | The external smoke was run from outside the repository | §8 |
| 17 | The automatic published-release smoke was awaited | run `40216211513` — §7 |

The crate was published from a **detached worktree of the tag**, as
`docs/RELEASING.md` requires — not from `main`.

## 2. Where this release deviated, and why each is stated

### 2.1 The tag was moved three times, and the third move was a documentation correction

The tag was moved from `f54708a` to `38f2759` (the platform-test fixes), then
to `de0e9e0` (the xtask CRLF fix), and finally to `1d67983` (the
documentation corrections). All three moves happened **before** any
publication, and the first two were made because hosted CI was red — the
release could not have been published otherwise.

The **third** move is the interesting one, and it is stated here because the
process allows a release candidate to be corrected and this one was:

- The changelog asserted "**Seventy-six defects from three interrogations of
  the 0.2.10 tree**". That figure had no derivation anywhere — not from the
  §10 register, not from the closure records, not from the commit log. It was
  replaced with the register's own growth, which *is* derivable: fifty
  findings added while the release was prepared, the register growing from
  sixty-one entries at the 0.2.10 baseline (`8b96f86`) to one hundred and
  eleven.
- The changelog asserted "The register now holds **108 findings**". It was
  108 when written and stopped being true three findings later — 109, 110 and
  111 landed with the hosted-lane fixes — and it is 111 now.
- The changelog asserted "**`cargo cleanme --known` still resolves to the bare
  invocation**". It does not, and never did: the root has no `--known`, so
  that spelling is a usage error with exit `2`. The project's own C045 plan
  had recorded the correct fact two days earlier, and the changelog
  contradicted its own plan.
- Register item 68's row was missing its closing cell separator — the only
  row in the table that did not end the way every other row does.
- `plans/registry.md`'s Published state section claimed sixteen releases when
  eighteen were published.

Each correction was verified against the built binary or the source, not
against the prose it replaced. The cost of the move was one CI cycle and one
Eggpack build; the alternative was publishing a changelog that misstated its
own release's defect count and misdescribed a published CLI spelling.

### 2.2 The release notes were curated, not left as the Eggpack placeholder

This follows **v0.2.8** and **v0.2.10**: the notes were written while the
release was still a draft and read back before the draft was flipped public.
The placeholder body ("Draft binary release staged by Eggpack…") is a staging
artefact, not release notes.

**The notes were corrected once after publication.** The first draft of the
notes repeated the `--known` claim that §2.1 had already corrected in the
changelog — "`cargo cleanme --known` still resolves to the bare invocation" —
and the same false claim was therefore published in the release body. It was
caught by reading the published body back against the built binary, and
corrected with `gh release edit`. An **immutable** release locks the tag and
the assets; the release *body* remains editable, which is what made the
correction possible at all. The published body now says what the changelog
says: `--known` does not move to the root, `cargo cleanme --known` is a usage
error, and `cargo cleanme clean --known` still resolves to the bare
invocation.

### 2.3 The `Changed — BREAKING` heading

The changelog section is headed `Changed — BREAKING`, because the canonical
spellings move to the root and the Full cleanup's proof boundary moves from
the platform to the learned root. A 0.2.10→now mapping table sits beside it.
No script parses that heading; it exists for the reader.

### 2.4 The gate ran on a tree that was not `main`'s first commit

The gate was run on the **final** tree (`1d67983`), with the tag present, so
the identity check ran *inside* the gate rather than beside it.

## 3. The review gate, performed before publication

The publication gate requires four things, and all four were satisfied before
the draft was flipped:

1. **The Eggpack candidate build was green.** Run `40214164767`, all 20 jobs
   green (5 builds, 5 qualifications, 5 validations, plus `preflight`,
   `resolve`, `required_gate`, `aggregate`, `stage`).
2. **The staged-release validation was green for the same tag and the same
   source SHA.** Run `40216008270`. It checked out `1d679833ae62`, re-proved
   the tag/source binding, and validated the actual staged bytes. Its
   retained log reads:

   ```text
   Validating staged draft v0.2.11
   [1/6] inventory (15 assets)
     … 5 contracted binaries + 5 sidecars + manifest + 2 installers
   [2/6] sidecar integrity
     ok cargo-cleanme-aarch64-apple-darwin
     ok cargo-cleanme-aarch64-unknown-linux-gnu
     ok cargo-cleanme-x86_64-apple-darwin
     ok cargo-cleanme-x86_64-pc-windows-msvc.exe
     ok cargo-cleanme-x86_64-unknown-linux-gnu
   [3/6] release-manifest.json agreement
     manifest binds source revision 1d679833ae62 to tag v0.2.11
     manifest agrees with the bytes for 5 asset(s)
   [4/6] contract agreement
     … 5 targets → asset names, sizes and digests
   [5/6] static Linux ABI evidence (not runtime)
     aarch64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
     x86_64-unknown-linux-gnu: requires at most GLIBC_2.17.0 (static ELF evidence)
   [6/6] real installer qualification on this host
     installed the real cargo-cleanme-x86_64-unknown-linux-gnu and it reports 'cargo-cleanme 0.2.11'
     the installed bytes match the release asset digest exactly
   PASSED: the staged draft matches the release contract
   ```

   The line "… + **2** installers" is **wrong**, and it is quoted as the tool
   printed it. `scripts/validate-staged-release.py` hardcodes that count in
   its summary note; the *check* it performs uses the correct four-installer
   inventory and passed. See §11.
3. **The validator's summary was read by a human.** Exact tag `v0.2.11`;
   source SHA `1d679833ae62`; upstream run `40214164767`; inventory count
   15; measured glibc floors 2.17.0 on both Linux targets; installer result:
   the real binary installed and reported its version.
4. **The crate version and the release tag agreed**, and no public release
   existed for this version beforehand: `check-release-identity.py --tag
   v0.2.11` exit 0 ("v0.2.11 names 0.2.11 at 1d679833ae62"), and the
   crates.io API answered `404` for `0.2.11` before the upload.

### The independent local inspection

The draft's 15 assets were downloaded and **every** digest recomputed from the
bytes, then compared four ways: the local recomputation, the
`release-manifest.json`, each binary's `.sha256` sidecar, and GitHub's own
reported asset digest. Sizes were compared against both GitHub and the
manifest. All fifteen agreed, for all five binaries, and the inventory was
exactly the contracted set (15 of 15, no extra, none missing).

The staged `cargo-cleanme-x86_64-unknown-linux-gnu` was also run directly:
`cargo-cleanme 0.2.11`, a stripped PIE ELF, with a measured glibc floor of
`GLIBC_2.17`.

## 4. Crate publication

`check-release-identity.py --tag v0.2.11` was run first ("v0.2.11 names
0.2.11 at 1d679833ae62", exit 0). `cargo publish --locked` uploaded and
reported:

```text
Published cargo-cleanme v0.2.11 at registry `crates-io`
```

crates.io confirms version `0.2.11`, `yanked: false`, sha256
`96298f832e01c6289488f3c4869f6b06596f1986aace7523bba72c17a126b9d2`. This is
the **nineteenth** published release and the **eighteenth** installable one;
`0.2.0` remains the only yanked release. The registry now lists **nineteen**
versions with `max_version: 0.2.11`.

## 5. Asset inventory

Every SHA-256 below was recomputed from the downloaded bytes, and each binary
digest was checked against its sidecar, against the release manifest, and
against GitHub's reported digest. All agree.

| Asset | Bytes | SHA-256 |
|---|---:|---|
| `cargo-cleanme-x86_64-unknown-linux-gnu` | 9370312 | `2ff98bba72c1593c40ab493174547ce490f47dfcc25fd2895505219cd86fe3d0` |
| `cargo-cleanme-aarch64-unknown-linux-gnu` | 8263016 | `294c90dd90d4229ff51362e984e362675514a22062a3e57a5c96c6682bc0f820` |
| `cargo-cleanme-x86_64-apple-darwin` | 11269928 | `2e3c9fd0cc9e07917a82e4832aaff56bf11a454b4d49e134fa4c853d550a0fde` |
| `cargo-cleanme-aarch64-apple-darwin` | 10784640 | `a5adf9cca06cd8d623c1f053118d67153f59eb54ae09cc6f1930ff94fbef2f75` |
| `cargo-cleanme-x86_64-pc-windows-msvc.exe` | 9812992 | `4c777fdce3263e7ef7ca32e52b828d0ac96e03c087da2b16733bbcd78dd7dfee` |
| `install.sh` | 24808 | `cc59833b0a3c5c816c63ad85e359e81dafad802a62de2ea2deb4391100031eaa` |
| `install.ps1` | 13046 | `e8268523a32e2322cfba310fcca4a4c547dc6a9b8de7fb84cdd13030694cfc19` |
| `install-exact.sh` | 6900 | `16935eca0132a3715822c705ad01430b1aa8fedc188f97a896ecae445cc75cb4` |
| `install-exact.ps1` | 4710 | `b74e24191d8e089d8ebbd0ade18d27f8f99dee82955414b94f2b66d5b203ad93` |
| `release-manifest.json` | 1340 | binds `1d679833ae6267b78a8ca0b3560bced7537aca4c` to `v0.2.11` |
| 5 × `.sha256` sidecar | 100–107 each | each matches its binary |

`install.sh` and `install.ps1` are **byte-identical to v0.2.10's**
(`cc59833b…`, `e8268523…`) — the machine-checkable form of "installer
behaviour is unchanged by this release", which touched `src/` only for the
scope composition, the proof boundary, the two selection surfaces and the
interactive frame. `install-exact.sh` / `install-exact.ps1` differ from
v0.2.10's **only** in the pinned release version in their download URLs; that
is their designed per-release change.

## 6. Public-release evidence

The release is **immutable** and carries a valid signed attestation:

```text
verify-release-attestation: v0.2.11 is immutable and carries a valid release attestation
  attested release subject: pkg:github/dbowm91/cargo-cleanme@v0.2.11 (15 asset digest(s))
```

Re-run against the downloaded bytes, the attestation and the bytes agree:

```text
verify-release-attestation: v0.2.11 is immutable and its 15 asset digest(s) match GitHub's signed attestation
```

## 7. The automatic published-release smoke

`post-release-smoke.yml` ran automatically on the `release: published` event
as run **`40216211513`**, and it is **green on all five target lanes**:

| Job | Result |
|---|---|
| Resolve the exact transition | success |
| `x86_64-unknown-linux-gnu` | success |
| `aarch64-unknown-linux-gnu` | success |
| `x86_64-apple-darwin` | success |
| `x86_64-pc-windows-msvc` | success |
| `aarch64-apple-darwin` | success |

The transition resolver bound `from_version=v0.2.10` to `to_version=v0.2.11`,
and it **waited for crates.io** before proceeding — 17:23:18Z to 17:23:54Z,
36 seconds — so the lane set was resolved against a registry that actually
carried the new version rather than one that merely would.

## 8. External smoke, from outside the repository

Run from a scratch directory outside the repository, against the published
artifacts only. **Every check passed.**

```text
=== install ===
  ok   cargo install cargo-cleanme --version 0.2.11 --locked --root <isolated>
  ok   install resolved registry-only dependencies
  ok   PATH resolves the freshly installed binary
=== version ===
  ok   cargo-cleanme --version (cargo-cleanme 0.2.11)
  ok   cargo cleanme --version (via Cargo) (cargo-cleanme 0.2.11)
=== config ===
  ok   config show exit code (0)
  ok   config show printed the effective config
=== config show --format json must be refused ===
  ok   exit code (2)
  ok   stdout is empty (no config text leaked)
  ok   refusal message names the correct reason
=== bounded scan ===
  ok   scan exit code (0)
  ok   exactly one JSON document on stdout, schema_version 1
  ok   stdout is JSON only
=== cargo cleanme --full --dry-run ===
  exit 1
  ok   exited 1 and named the roots it skipped
  ok   exited 1 and reported the incomplete traversal
  ok   every skipped unit carries a typed 'skipped_*' reason code, and the scope is not blocked
  ok   --dry-run spawned nothing (the run says so)
=== update --dry-run ===
  ok   exit code (0)
  ok   reports already-current and names the version
=== update must refuse a Cargo-managed install ===
  ok   refused with exit 2
  ok   the managed bytes are unchanged
  ok   cargo install --list still reports 0.2.11
=== published release asset ===
  ok   downloaded the published release assets
  ok   the asset's digest matches its .sha256 sidecar
  ok   the asset reports its version (cargo-cleanme 0.2.11)
  ok   the asset's update --dry-run exit code (0)
  ok   the asset's update --dry-run reports already-current

EXTERNAL SMOKE PASSED
```

### The harness needed three corrections first, and the corrections are the finding

The first run of the harness reported three failures. **All three were
defects in the harness, not in the product**, and each is the same shape as a
defect the v0.2.10 record found and wrote down:

1. **A stale binary on `PATH` shadowed the binary under test.** `cargo
   cleanme --version` resolved `/home/sugarwookie/.local/bin/cargo-cleanme`,
   which is **0.2.10** — it precedes `~/.cargo/bin` on this host's `PATH`.
   The direct spelling used an absolute path and passed; the Cargo spelling
   went through `PATH` and reported the old version. The harness now puts the
   isolated install first on `PATH` and **asserts that `PATH` resolves the
   binary it just installed**.
2. **The "did `--dry-run` spawn a `cargo clean`?" check was backwards.** It
   grepped for the substring `cargo clean`, which matches the very sentence
   that *proves* nothing was spawned: "… (simulation only — no `cargo clean`
   command was invoked)". The check now asserts that sentence is present.
3. **The config-isolation step un-authenticated `gh`.** The harness sets
   `XDG_CONFIG_HOME` to an isolated directory for the tool's own config, and
   `gh` reads its credential store from `$XDG_CONFIG_HOME/gh/hosts.yml` — so
   the asset download failed with *"To get started with GitHub CLI, please run:
   `gh auth login`"*. The override is now scoped to the two config commands
   and restored immediately afterwards.

A fourth harness assertion was also relaxed: it required a *skipped* unit in
the Full cleanup's envelope, but whether any unit is skipped depends on the
machine's learned roots at that moment. The assertion now applies to the
units that **are** skipped, which is what the contract actually says.

**The reason code was looked for in the wrong place.** The harness first
looked for `skipped_incomplete_coverage` on stderr. It is not there, and it
is not supposed to be: the reason codes are the JSON envelope's
`units[].reason_code`, and the human surface renders prose. On this host the
excluded unit carried `skipped_marker_invalid` — a *different* typed reason,
because the root it excluded failed for a different cause. The contract is
"every excluded unit carries a typed `skipped_*` reason code", which is what
the harness now asserts. The documentation's claim that the code is "named on
stderr" was **false**, and is corrected — see §11 and C036 D48.

## 9. Completions, manpages, benchmark, size

The 13 generated artefacts (5 completions, 8 man pages) matched the clap model
at the release commit — `generate-docs --check` reported "13 artifacts match
the clap model", and the `generated-docs` CI job was green. The eight man
pages carry the new version.

## 10. Effect on closures

- **M018 — closed.** The proof-scoped Full cleanup gate shipped in v0.2.11.
- **M019 — closed.** The canonical root-front-door spellings shipped in
  v0.2.11. The ordering constraint ADR 008 recorded — *M019 may not be
  published before M018 has shipped* — is satisfied: both are in this release.
- **C042, C043, C044, C045, C046 — closed.** Each shipped in v0.2.11.
- **C036 — unchanged.** Its remaining items stay open; this release
  discharges no new D-item of it beyond the drift corrections §2.1 records.
- **C041 — unchanged and still open.** The update-path `ETXTBSY` flake is not
  fixed by this release and is not publication-blocked.

## 11. Known limitations and unresolved findings

- **The staged-release validator mis-states its own inventory composition.**
  `scripts/validate-staged-release.py` prints "… + 2 installers" in its
  inventory note, while the inventory it *checks* is five binaries, five
  sidecars, a manifest and **four** installers. The check is correct and
  passed; the summary line is a hardcoded literal that has been wrong since
  at least v0.1.6. It was found by the human inspection of this release's
  validator output, and it is fixed after publication, because the staged
  bytes bind the release manifest to the tagged source revision and
  re-staging for a summary label would have voided a valid candidate. The
  **v0.2.10 receipt** quoted the same line as "4 installers" — it corrected
  the tool's output in prose rather than recording what the tool printed.
  This receipt quotes it as printed. Recorded as **C036 D47**; the installer
  names are now a named constant and the note derives its count from them.
- **Two documents placed a machine reason code on stderr, where no reason
  code is ever printed.** `docs/RELEASING.md` and `docs/AUTOMATION.md` said a
  learned root whose coverage could not be proven is skipped "with the reason
  code `skipped_incomplete_coverage`, **named on stderr**". The code is the
  JSON envelope's `units[].reason_code`; the human surface renders the
  root and the cause in prose and never prints a code. Both documents are
  corrected, and the correction is recorded as **C036 D48**. This one was
  found by the external smoke, which looked for the code where it does not
  live.
- **One medium reliability finding, open: C041.** The release gate's
  `update::tests::a_staging_cleanup_failure_is_reported_not_swallowed`
  failed once with `spawning bounded candidate: Text file busy (os error
  26)` (`ETXTBSY`) on the v0.2.10 release gate. It is **not** a safety
  defect, it is reachable by a user, and it is **not** fixed by this release.
- **Medium (evidence shape, not a defect): the interactive terminal
  lifecycle.** C042 and C043 shipped in this release, so the frame's bytes
  and geometry are now correct and pinned; the repository still has no pty
  harness for the CLI-level proof of the rendered surface, and the cursor
  restoration is verified by an in-test terminal emulator rather than a real
  terminal.
- **Low (coverage, not a defect):** the *supported* branch of the profile
  selector is still only executed against a real Cargo on a qualified host.
- **ADR 008 — resolved.** The open owner decisions ADR 008 left were
  resolved on 2026-10-21 (option A, the ROOT positional at the root), and
  this release is the first to carry them.

## 12. Disposition

**Closed.** The release was published on both channels — the GitHub release is
public and immutable with a valid attestation, and the crate is on crates.io
and not yanked — and every gate the project defines for a publication was
executed and recorded above. The publication conditions on M018, M019, C042,
C043, C044, C045 and C046 are discharged.

One finding is carried forward **open** rather than closed with the release:
C041, the update-path `ETXTBSY` flake. It was found by the v0.2.10 release
gate, it is not a safety defect, and it was deliberately not fixed inside the
release.
