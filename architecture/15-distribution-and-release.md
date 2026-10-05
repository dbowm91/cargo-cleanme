# Distribution & Release — how the binary reaches a user, and what guarantees the bytes

> Component deep dive · part of the [architecture overview](overview.md)

Everything in this document lives outside `src/`. It answers one question: once
a contributor has changed the product, what guarantees that the bytes a user
executes are the bytes that were qualified? The answer is a chain — build in a
pinned environment, produce a digest, publish that digest, and verify the digest
at the point of installation — plus a set of identity gates that check the chain
is internally consistent before publication.

Three findings dominate and are stated up front because they change how the rest
should be read:

1. **The live self-update rehearsal is manual.** It is `workflow_dispatch`-only
   with no `push` or `pull_request` trigger. Every rehearsal cited in the release
   history (`v0.1.1 → v0.1.2`, `v0.1.3 → v0.1.4`, `v0.1.5 → v0.1.6`) was
   triggered by hand against an already-published release, and the two defects it
   found (C016, C017) shipped in published versions. See §8.
2. **Installer downloads are checksum-verified, and a missing checksum is
   fatal.** Both installers download a `.sha256` sidecar and abort if it is
   absent, malformed, or mismatched. See §3.
3. **There is no signature verification anywhere in the release surface.** No
   GPG, sigstore, minisign, Apple codesign, or notarisation step exists in the
   release workflow, the pack policy, or the installers. HTTPS plus a SHA-256
   sidecar is the entire protection. See §9.

## 1. Responsibility

This surface owns **provenance**: how a user obtains the binary, and how anyone
can later determine what they obtained.

It owns:

- the two bootstrap installers (`packaging/install.sh`, `packaging/install.ps1`)
  and their shared contract with the published asset set;
- the generated shell completions and manpages (`completions/`, `man/`) and the
  generator that produces them (`xtask/`);
- the published-crate packaging manifest (`Cargo.toml`'s `include` allowlist);
- the binary release pipeline and its identity gates
  (`.github/workflows/release-*`, `scripts/check-release-identity.py`,
  `scripts/gen-release-workflow-shape.py`, `scripts/validate-staged-release.py`);
- post-release verification (`.github/workflows/post-release-smoke.yml`,
  `scripts/post-release-smoke.sh`, `scripts/smoke-release-candidate.py`).

It does not own product behaviour, and **nothing in `src/` depends on any of
it** — the dependency edge does not exist in that direction, which is what lets
this surface be revised without touching the product.

The dependency runs the other way, in two places, and both are load-bearing:

- **The packaging allowlist constrains compilation.** `Cargo.toml`'s
  `include` list dictates what a published crate contains, and
  `src/config.rs` embeds `/config.toml` at compile time with
  `include_str!`. A file outside the allowlist cannot be referenced at compile
  time without the published crate failing to build. See §5.
- **The installers constrain the self-updater.** `src/update.rs` classifies an
  on-disk installation into a `Provenance` variant, and self-update may only
  replace installations it is entitled to replace. The installers must produce a
  layout the classifier recognises. See §2, §3, and
  [12-self-update.md](12-self-update.md).

## 2. Distribution channels

`Provenance` (`src/update.rs:166-181`) has exactly four variants, and each
channel maps to one of them. The classification is computed by
`classify_provenance_in` (`src/update.rs:403`), which is a positive-evidence
decision: it never infers ownership from a filename or a directory shape.

| Channel | Mechanism | Platforms | Install location | `Provenance` |
| --- | --- | --- | --- | --- |
| `cargo install cargo-cleanme` | Cargo builds the registry crate | Any host Cargo targets, incl. `linux:armv7` where no binary is published | `<cargo root>/bin` | `CargoManaged { bin_root, version }` |
| `packaging/install.sh` (prebuilt path) | Download asset + `.sha256` | Linux + macOS, x86_64/aarch64 | `/usr/local/bin` when elevated, else `$HOME/.local/bin` | `VerifiableSelfManaged { digest }` |
| `packaging/install.sh` (Cargo fallback) | Resolves `cargo` and builds | Any recognised triple with no published binary (notably `linux:armv7`) | same as above | `CargoManaged` or `VerifiableSelfManaged`, decided by Cargo's own record |
| `packaging/install.ps1` (prebuilt path) | HTTPS download of `.exe` + `.sha256` | Windows x86_64/aarch64 | `%ProgramFiles%\cargo-cleanme\bin` when elevated, else `%LOCALAPPDATA%\cargo-cleanme\bin` | `VerifiableSelfManaged { digest }` |
| `packaging/install.ps1` (Cargo fallback) | Resolves `cargo` and builds | Windows triples with no published binary | same as above | as above |
| Direct GitHub asset download | User fetches the asset by URL | One target per asset (5 targets) | Wherever the user puts it | `VerifiableSelfManaged { digest }` |
| `cargo cleanme update` | Replaces the running binary in place | Hosts with a published artifact | Unchanged | Preserves the existing variant |

Two details in that table are load-bearing and non-obvious.

**The installers deliberately do *not* install into a Cargo bin root.** That is
not an oversight; it is what keeps the installation updatable. `install.sh`
chooses `/usr/local/bin` (elevated) or `$HOME/.local/bin` (normal user)
(`packaging/install.sh:157-163`), and `install.ps1` chooses
`%ProgramFiles%\cargo-cleanme\bin` or `%LOCALAPPDATA%\cargo-cleanme\bin`
(`packaging/install.ps1:105-113`). Both are outside any Cargo root, so
`classify_provenance_in` falls through to hashing the executable and returns
`VerifiableSelfManaged`. The source comment on that fall-through names the
installers directly: "the published installer's `/usr/local/bin` and
`~/.local/bin` keep resolving to the self-managed path"
(`src/update.rs:440-441`). **If either installer were changed to install into
`$CARGO_HOME/bin`, every installation it created would immediately become
un-updatable.** That is a cross-component coupling invisible from either side
alone.

**The classification is by positive record, not by location.** A binary sitting
inside `<cargo root>/bin` is `CargoManaged` only if Cargo's own metadata records
this package and this binary name there; if the root records nothing, the result
is `UnprovableOwnership`, not `CargoManaged` (`src/update.rs:416-431`). The
same applies to the ancestor scan that covers `cargo install --root DIR`
(`src/update.rs:442-447`). A directory that merely *looks* like an install root
is not evidence.

The load-bearing property is therefore **distinguishability**, and it holds with
four outcomes rather than two. Self-update is entitled to replace only a
`VerifiableSelfManaged` installation, and only after re-proving the destination
by its exact prior digest under the mutation lock
(`src/update.rs:172-175`, `LiveDigestVerifier` at `src/update.rs:220-229`). A
`CargoManaged` installation must be refused with the corrective command printed
instead — `cargo install cargo-cleanme --locked --force`
(`src/update.rs:197-199`) — because replacing it in place would make
`cargo install --list` and Cargo's uninstall bookkeeping lie. `UnprovableOwnership`
is refused too, with its own remediation (`src/update.rs:203-206`).

That distinction is not cosmetic. The `v0.1.3 → v0.1.4` rehearsal (C017) found a
`CargoManaged` binary that was misdetected and silently replaced, and it shipped
in those two published versions
(`plans/closure/distribution-release-update/c017-status.md`).

Current version is **0.1.6**; the changelog records 0.1.0 through 0.1.6 and
**nothing has been yanked**. Yanking was explicitly considered and rejected: the
defects in 0.1.1 and 0.1.2 are updater-only, their scan, clean, config, and
reporting behaviour is correct, and "yanking would misdescribe releases whose
scan, clean, config, and reporting behavior is correct"
(`plans/closure/distribution-release-update/c014-status.md`, "Disposition"). The
C017 defect in 0.1.1–0.1.4 was instead disclosed in the 0.1.5 changelog entry
with a safe upgrade route, on the grounds that a silent-success updater bug does
not make the rest of the release wrong.

## 3. The bootstrap installers

The two installers are one contract with two implementations, and
`scripts/check-installer-contract.py` is the machine check that keeps them
aligned. It parses `release/eggpack/distribution.toml` for the contracted asset
set, then parses each installer to extract the triples it can select
(`posix_mapping` at `scripts/check-installer-contract.py:66`,
`powershell_mapping` at `scripts/check-installer-contract.py:89`) and fails if
either can select a triple with no published asset.

### 3.1 `packaging/install.sh`

| Concern | Behaviour | Reference |
| --- | --- | --- |
| Platform detection | `uname -s` → `linux`/`macos`; anything else fails, pointing the user at `cargo install --locked` | `install.sh:124-131` |
| Architecture detection | `uname -m` normalised: `x86_64\|amd64`→`x64`, `aarch64\|arm64`→`arm64`, `armv7l\|armv7\|armhf`→`armv7`; anything else fails with the same source-build pointer | `install.sh:132-138` |
| Triple selection | `case "$os_family:$arch_family"` mapping to one of the contracted triples, or `""` meaning "no prebuilt binary" | `install.sh:141-148` |
| Download source | `${CARGO_CLEANME_INSTALL_BASE_URL}` or `${CARGO_CLEANME_INSTALL_LATEST_URL}`, defaulting to the GitHub release URLs. Both env vars exist so the fixture server can stand in for the release host | `install.sh:37-38` |
| Asset naming | `$PRODUCT-$TARGET`, plus `.exe` on Windows | `install.sh:261-265` |
| Integrity verification | **Yes — SHA-256 sidecar, mandatory.** Non-2xx on the sidecar is fatal ("The digest sidecar is mandatory evidence. Its absence is a hard failure, not a fallback signal"); the digest must match `^[0-9a-f]{64}$`; then compared against the downloaded file | `install.sh:305-327` |
| Post-download identity check | The staged binary is executed and must report a version, or the install aborts | `install.sh:333-337` |
| Transport | HTTPS only. A non-HTTPS URL is refused unless `CARGO_CLEANME_INSTALL_ALLOW_INSECURE=1`; an unrecognised scheme is always refused. A connection-level failure prints `000` and is fatal | `install.sh:209-221` |
| Install target | `/usr/local/bin` when `id -u` is 0, else `$HOME/.local/bin`; overridable with `--dir` | `install.sh:157-163` |
| Existing install | **Refuses.** `fail "$DEST already exists. Re-run with --force to replace it."` — replacement requires an explicit `--force` | `install.sh:174-176` |
| Cargo fallback | When no prebuilt binary matches, resolves `cargo` and builds from source. Entered on an unrecognised triple or a genuine **404** only; a 5xx or transport fault stays fatal "so a release host outage can never silently become a local build" | `install.sh:150-154`, `install.sh:277-285` |
| Dependencies | Requires `curl` plus `sha256sum` or `shasum` for the prebuilt path | `install.sh:179-195` |
| Failure behaviour | Diagnostics to stderr, non-zero exit; the binary is staged in a temp dir and moved only after every check passes | `install.sh:255`, `install.sh:322-337` |

### 3.2 `packaging/install.ps1`

The PowerShell installer is the same contract with Windows-native
mechanisms, and the correspondence is close enough to enumerate:

| Concern | Behaviour | Reference |
| --- | --- | --- |
| Architecture detection | Normalised to the same arch families; a family with no published Windows binary sets `$cargoFallbackHost` | `install.ps1:96-100` |
| Download source | Same two env-var overrides with the same GitHub defaults | `install.ps1:55-56` |
| Integrity verification | `Get-FileHash -Algorithm SHA256`; the sidecar is mandatory, its status must be 2xx, and the digest must match `^[0-9a-f]{64}$` | `install.ps1:125`, `install.ps1:225-235` |
| TLS | HTTPS enforced by a scheme check, with TLS 1.2 as the enforced floor and the previous `SecurityProtocol` restored afterwards | `install.ps1:134`, `install.ps1:143-165` |
| Post-download identity check | The candidate "must print exactly `cargo-cleanme X.Y.Z`" | `install.ps1:16` |
| Install target | `%ProgramFiles%\cargo-cleanme\bin` when elevated, else `%LOCALAPPDATA%\cargo-cleanme\bin`; overridable with `-Directory` | `install.ps1:105-113` |
| Existing install | **Refuses** unless `-Force`, mirroring the POSIX `--force` | `install.ps1:118-119` |
| Cargo fallback | Present and equivalent: a **404** on the asset enters the fallback, while a non-2xx other than 404 or a transport failure stays fatal, with the same stated reason | `install.ps1:208-215` |
| Atomic install | Copies to the destination only after verification; cleans up the work directory in a `finally` | `install.ps1:301-324` |

### 3.3 Divergence between the two installers

The known divergence risk here is not a behavioural bug in the current code — it
is a process failure. The release history records a Windows installer fixture that
had never executed on a Windows host, producing a green result that meant nothing
(C012). The repair was to make the suite assert its own premises, and the CI lane
now runs the PowerShell wrapper for real on `windows-latest`
(`.github/workflows/ci.yml:37-63`).

Reading the two side by side, the **contract-equivalent** behaviours line up:
mandatory sidecar, 404-only fallback, refuse-to-overwrite without an explicit
force flag, post-download version identity check, and a destination outside any
Cargo root. The **non-contract differences** are all deliberate and all
shell-appropriate:

- Destination: POSIX uses `/usr/local/bin` / `$HOME/.local/bin`; Windows uses
  `%ProgramFiles%\cargo-cleanme\bin` / `%LOCALAPPDATA%\cargo-cleanme\bin`. The
  elevation branch exists on both sides (POSIX checks `id -u`, Windows checks the
  Windows principal role) but resolves differently. Only the *asset* and
  *triple* contract is machine-checked between the installers
  (`scripts/check-installer-contract.py:66-91`), not the destination rule — which
  is correct, because the destinations cannot be the same across OSes. What
  *can* be machine-checked and is not: that neither destination is ever inside a
  Cargo root, which is the property §2 depends on.
- The insecure-transport escape hatch is POSIX-only
  (`CARGO_CLEANME_INSTALL_ALLOW_INSECURE`, `install.sh:213-215`); PowerShell has
  no equivalent because it enforces TLS at the protocol level instead.
- The POSIX Cargo fallback is driven by a static `case` over triples
  (`install.sh:141-148`); the Windows one is reached from a runtime
  `$target` emptiness check (`install.ps1:96-100`). Same outcome, different
  mechanism.

### 3.4 The installer fixtures

`packaging/tests/test_installers.py` (991 lines) is the installer qualification
suite, and `packaging/tests/fixture_server.py` (160 lines) is a local fixture
server that stands in for the GitHub release service. **No test in this suite
contacts the public GitHub service or a real release** — the CI job comment
states this explicitly (`.github/workflows/ci.yml:38-40`).

**The Windows cases do execute on a Windows runner.** The job matrix is
`[ubuntu-latest, macos-latest, windows-latest]` (`.github/workflows/ci.yml:47`)
and the suite is invoked on all three. The suite contains 15 named cases
(`packaging/tests/test_installers.py:766-782`):

| Case | What it pins down |
| --- | --- |
| `case_happy_path` | Normal install succeeds |
| `case_exact_version` | An exact version can be requested |
| `case_binary_404_falls_back` | A missing asset falls back rather than hard-failing |
| `case_checksum_absent_is_fatal` | **A missing `.sha256` aborts the install** |
| `case_malformed_digest` | A malformed sidecar aborts |
| `case_digest_mismatch` | A wrong digest aborts |
| `case_wrong_candidate` | A correctly-named but wrong binary is caught by the digest |
| `case_transport_failure` | Transport errors abort |
| `case_existing_destination` | An existing installation is handled |
| `case_unwritable_destination` | A non-writable target fails cleanly |
| `case_bad_version_syntax` | Bad version input is rejected |
| `case_cargo_missing` | Absent `cargo` on the fallback path is handled |
| `case_fake_cargo_is_actually_resolved` | **The `cargo` fallback resolves the real tool, not a stub** |
| `case_cargo_produces_nothing` | A `cargo` invocation producing no binary fails |
| `case_temp_cleanup` | Temporary files are removed |

The last three are the C012 repair. `case_fake_cargo_is_actually_resolved`
exists specifically because a stub `cargo` earlier in `PATH` could have made a
green result meaningless, and `self_test()` (`packaging/tests/test_installers.py:785`)
proves the guards reject a deliberately broken setup. The premise-negative
self-test runs on every matrix lane, including Windows, specifically because
that is where the defect lived (`.github/workflows/ci.yml:57-63`).

## 4. Generated documentation

### 4.1 The generator and its gate

`xtask/src/main.rs` is a separate `[[bin]]` in the same package, named
`generate-docs`, declared with `required-features = ["dev-tools"]`
(`Cargo.toml`). It compiles only when that feature is enabled. A normal build
and a `cargo install cargo-cleanme` never compile it, and — critically for
§5 — because it lives under `xtask/` rather than `src/`, it is outside the
package allowlist and is never published.

It has exactly one input and two output kinds. The input is the clap command
model from `src/cli.rs`; there is no hand-maintained copy of the CLI anywhere in
the generator, so the docs cannot disagree with the parser by construction — only
by staleness. The outputs are:

- **5 completion files** in `completions/`, one per shell in the `SHELLS` array
  (`xtask/src/main.rs:23-29`): Bash, Zsh, Fish, PowerShell, Elvish. Elvish is
  included deliberately, with a comment saying so
  (`xtask/src/main.rs:21-22`). Each is generated with
  `clap_complete::generate` (`xtask/src/main.rs:46`).
- **8 manpages** in `man/`, one per command node, via `clap_mangen`
  (`xtask/src/main.rs:120-165`).

The `--check` flag is what makes CI enforcement possible: it regenerates in
memory and compares against the checked-in tree instead of writing
(`xtask/src/main.rs:8`, `32`).

### 4.2 Naming convention in `completions/`

The output path per shell is chosen explicitly in `completion_path`
(`xtask/src/main.rs:107-115`), and the naming is **not** uniform — deliberately:

| File | Shell | Convention note |
| --- | --- | --- |
| `cargo-cleanme.bash` | bash | Suffix-style |
| `_cargo-cleanme` | zsh | **Underscore-prefixed** — zsh's completion system sources a `$fpath` file as a function definition, and the underscore is what marks it as one |
| `cargo-cleanme.fish` | fish | Suffix-style |
| `_cargo-cleanme.ps1` | PowerShell | **Underscore-prefixed**, matching the PowerShell loader's own convention |
| `cargo-cleanme.elv` | Elvish | Suffix-style |

The two underscore-prefixed files are the correct ones for their shells; the
three suffix files are correct for theirs. **A "make these consistent" cleanup
would be a regression**, because consistency here means ignoring two different
shell conventions. The mapping lives in one function precisely so that the
per-shell differences are visible in a single place instead of emerging as five
independent filename decisions.

### 4.3 The manpage structure

Eight pages, one per command:

`cargo-cleanme.1` plus `cargo-cleanme-clean.1`, `cargo-cleanme-config.1`,
`cargo-cleanme-edit.1`, `cargo-cleanme-path.1`, `cargo-cleanme-scan.1`,
`cargo-cleanme-show.1`, `cargo-cleanme-update.1`.

One page per subcommand means **the manpage set is a direct projection of the
`src/cli.rs` command enum**, and the generator says so: it "renders one man page
per command node, the conventional multi-page layout," because
`clap_mangen::Man::new` only renders the node it is given and would otherwise
produce a single page (`xtask/src/main.rs:120-122`). The file name is
`man/cargo-cleanme.1` for the root and `man/cargo-cleanme-{name}.1` for each
subcommand (`xtask/src/main.rs:163-165`).

Adding a subcommand without regenerating leaves the docs set one page short;
removing one leaves a page describing a command that no longer exists. The
structure makes the CLI's shape legible in the documentation surface, which is the
point — and it means the documentation surface inherits the completions' exact
shape-change fragility. Both are caught by the same `--check`.

### 4.4 The output is a tracked artefact, and CI enforces freshness

`completions/` and `man/` are **checked into git** (13 tracked files; confirmed
via `git ls-files`). The generator's output is therefore a tracked artefact and
is subject to drift by construction: it is regenerated only when someone
remembers to run it.

The `generated-docs` job in `.github/workflows/ci.yml:64` closes that gap. It runs
the generator in its `--check` mode — `cargo run --quiet --features dev-tools
--bin generate-docs -- --check` (`ci.yml:74`) — which regenerates in memory and
fails if the result differs from what is committed, rather than writing over it.
The same job also runs `scripts/check-release-contract.py` (`ci.yml:76`). The
practical consequence is unambiguous:

> **Any change to the CLI surface must be accompanied by regenerated completions
> and manpages, or CI fails.**

This is the one drift risk in this component that is genuinely closed rather
than merely policed.

## 5. The published crate contract

### 5.1 The `include` allowlist

`Cargo.toml` carries an explicit `include` allowlist, and
`scripts/check-release-contract.py:460-464` fails if that allowlist is absent or
omits `config.toml`:

```
include = [entry.lstrip("/") for entry in cargo["package"].get("include", [])]
if not include:
    fail("Cargo.toml has no explicit package include allowlist")
if "config.toml" not in include:
    fail("Cargo.toml include allowlist omits the embedded config.toml template")
```

The allowlist admits eight entries and nothing else (`Cargo.toml:22-31`):
`/src/**`, `/config.toml`, `/Cargo.toml`, `/Cargo.lock`, `/README.md`,
`/CHANGELOG.md`, `/LICENSE-MIT`, `/LICENSE-APACHE`. Every compile-time input is
present; every repository-only surface is absent. The `Cargo.toml` comment states
the intent in the same terms: the published crate is "a deliberate subset of the
checkout rather than whatever Cargo happens to infer."

What it excludes is everything that exists to build, verify, document, or release
the project: `tests/`, `scripts/`, `xtask/`, `docs/`, `man/`, `completions/`,
`packaging/`, `plans/`, `examples/`, and `architecture/`. The exclusion is
load-bearing, not merely tidy — a crate shipping its own CI scripts, installers,
and generator would invite consumers to depend on files that are not a stable
API. The check enforces the boundary from the outside too:
`check-release-contract.py:468-471` fails if the allowlist is ever widened to
include the xtask generator.

### 5.2 Why `/config.toml` is a hard requirement

This is the most easily-broken item in the whole surface, and the reason is a
**compile-time coupling between a source file and a packaging manifest**.

`src/config.rs:10` embeds the template as a compile-time constant:

```rust
pub const CONFIG_TEMPLATE: &str = include_str!("../config.toml");
```

`include_str!` is resolved by the compiler, not at runtime. Therefore:

- If `config.toml` is missing from the published crate, **the crate does not
  compile at all** — `cargo install cargo-cleanme` fails with a compile error in
  `src/config.rs`, not with a runtime error and not with a degraded feature.
- First-use config creation is impossible without it. The constant is the only
  source of the default template; `src/config.rs:322` writes those bytes as the
  new config file, and `src/config.rs:493` parses them to prove the checked-in
  template is well-formed. Both are compile-time-dependent facts about a file
  that lives outside `src/`.

The `Cargo.toml` comment claims that `check-release-contract.py` fails the build
if `/config.toml` is ever dropped from the allowlist. **Verified: it does**, by
the explicit `if "config.toml" not in include` check above. The comment is
accurate. But note what the check protects against — it catches the
*manifest-level* omission. It does not catch a rename of the file on disk, or a
change to the `include_str!` path, if both the manifest and the source were
changed together and the crate was never built from the published package.

### 5.3 The `[[bin]]` split

Two `[[bin]]` targets are declared in one package (`Cargo.toml:38-48`):

| Bin | Path | Gate |
| --- | --- | --- |
| `cargo-cleanme` | `src/main.rs` | none — this is the product |
| `generate-docs` | `xtask/src/main.rs` | `required-features = ["dev-tools"]` |

Two independent mechanisms keep the generator out of consumer hands, and both are
needed. `required-features` means a default-features build — which is what
`cargo install` performs — never compiles it. And the `xtask/` path keeps its
*source* out of the published package, since `xtask/` is outside both `src/` and
the `include` allowlist. `Cargo.toml:42-44` says exactly this in a comment on the
second `[[bin]]`. Either mechanism alone would leave a hole: without the feature
gate the binary would be compiled on every install, and without the path
exclusion its source would ship.

### 5.4 A note for the reviewer about this document set

**`architecture/` is not in the allowlist, so nothing in this directory set
will ship in the published crate.** That is correct and intentional — these are
internal design notes, not consumer documentation. It is worth stating
explicitly because it is a surprising property of adding the directory: the
architecture documentation is invisible to every user of the tool, and a
contributor who assumed otherwise would be wrong. `docs/` is excluded on the
same grounds, with the same result.

## 6. Binary releases and the Eggpack contract

### 6.1 What is built

`.github/workflows/release-binaries.yml` is titled "Eggpack candidate builds" and
builds **five release targets**, one per contracted triple, each on its own
runner:

| Target | Runner | Build |
| --- | --- | --- |
| `aarch64-apple-darwin` | `macos-14` | `cargo +1.99.0 build --release --locked --target …` |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | `cargo +1.99.0 zigbuild --release --locked --target aarch64-unknown-linux-gnu.2.17` |
| `x86_64-apple-darwin` | `macos-15-intel` | `cargo +1.99.0 build --release --locked --target …` |
| `x86_64-pc-windows-msvc` | `windows-latest` | `cargo +1.99.0 build --release --locked --target …` |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `cargo +1.99.0 zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.17` |

The five-target set is stated three ways that must agree: the `--selected` list
passed to `eggpack ci _resolve-release`
(`release-binaries.yml:55`), the runner matrix, and the contracted asset set in
`release/eggpack/distribution.toml`. `check-release-contract.py` reports
`len(contracted) * 2 + 1` release assets — a binary and a `.sha256` sidecar per
target, plus one aggregate (`check-release-contract.py:534-537`).

Two details carry real weight. The **toolchain is pinned to an exact version**,
`+1.99.0`, not to a channel — that is what `check_release_toolchains` exists to
enforce, and the CI comment calls it "the gate that keeps a rebuild of one tag
from using a different compiler." And **every build is `--locked` and
`--package cargo-cleanme --bin cargo-cleanme`**, so the maintenance-only binary
cannot be produced as a release artefact even accidentally.

The Linux targets are cross-compiled with `cargo-zigbuild` pinned to `0.23.3`
(`release-binaries.yml:143`) with the `.2.17` target suffix, which establishes
the glibc floor as a **build** fact. The contract check enforces a specific and
subtle invariant about how that fact may be described: the README may *mention*
the glibc 2.17 floor, but only alongside an explicit non-claim such as "not yet
claimed" or "no runtime evidence" (`check-release-contract.py:486-509`). A plain
"the string may never appear" rule was deliberately rejected as forbidding the
honest explanation of the floor's status.

**There is no signing and no notarisation step.** No `gpg`, `sigstore`,
`minisign`, `codesign`, or notarisation reference appears anywhere in the
release workflow or in the `release/eggpack/` policy files. The artefacts are
unauthenticated. See §9.

### 6.2 The critical property: the workflow cannot publish

The most important thing this workflow does is **refuse to publish**. The final
job is named `stage`, its steps are `_prepare-stage` and `_stage-github-draft`
(`release-binaries.yml:1047-1052`), and `check_workflow_cannot_publish` (called at
`check-release-contract.py:527`) fails the contract check if the generated
workflow contains any publication or clobber path. The candidate build stages a
**draft** release and stops. Publication is a separate, deliberate, human step.

This split is what makes the identity gates meaningful. If the build could publish
itself, "the bytes in the draft are the bytes from the tagged commit" would be a
claim about a single irreversible action rather than a property of a reviewable
staged artefact. `validate-staged-release.py` (§7.3) exists to inspect that
artefact *before* anyone acts on it.

The whole workflow is also `workflow_dispatch`-only, requires an exact existing
`release_tag` as input (`release-binaries.yml:3-7`), checks out that tag
(`release-binaries.yml:34-36`), and holds `permissions: contents: read` — the
build job has no write scope of its own.

### 6.3 What Eggpack is, and what cargo-cleanme inherits

`release/eggpack/` is not a source tree. It is a **declarative producer
contract**: a set of TOML and JSON files that describe how the release must be
produced, what the GitHub surface must look like, and how a consumer must be
able to validate the result. It contains ten files, including:

| File | Role |
| --- | --- |
| `pack.toml` | The build plan: targets, cross-build strategy, toolchain |
| `distribution.toml` | The contracted asset set and install names — the single source both installers and the contract check are validated against |
| `build-bindings.toml`, `qualification-bindings.toml` | Which build steps and which per-target smokes run |
| `consumer-validators.json` | The per-target consumer validator (all 5 targets invoke `scripts/smoke-release-candidate.py`) |
| `github-policy.json`, `github-template.json` | The GitHub surface policy and draft template |
| `install-policy.toml`, `installer-presentation.json` | Installer policy and the presentation contract |
| `workflow-shape.json` | The machine-readable shape the generated workflow must have |

The project narrative is that Eggpack is an **external producer whose existing
contract, manifest, bootstrap, CI-generation, drift, and draft-staging surfaces
already covered cargo-cleanme's direct single-binary release**, which is why no
upstream implementation plan was needed. In concrete terms, cargo-cleanme
inherits: a declarative description of its own release, generation of the release
workflow from that description, a draft-staging flow that cannot publish, and a
consumer-validation step per target.

What it would have to do itself without Eggpack: define the asset contract in a
machine-readable form; generate and keep a workflow in sync with that contract;
guarantee the workflow stages rather than publishes; run a per-target consumer
smoke; and express the policy gates this project actually cares about (the
installer contract, the identity gate, the packaging allowlist) as declarative
rules rather than bespoke checks. That is the majority of the work in
`.github/workflows/release-binaries.yml` and `scripts/check-release-contract.py`.
Eggpack supplies the mechanism, not the policy — every cargo-cleanme-specific
rule quoted in §5, §6, and §7 is a local script, not an Eggpack-provided
constraint.

### 6.4 `release/baseline-benchmark.json`

A 28-line record of the release-relevant runtime and size baseline, consumed by
`scripts/release-benchmark.py` and refreshed explicitly
(`scripts/release-benchmark.py:22-23`):

| Field | Recorded value | What it catches |
| --- | --- | --- |
| `fixture` | 2 projects, 8 artifacts, 53248 bytes | A fixture-shape change invalidating the numbers |
| `counters` | manifests 2, workspaces 2, locate 2, metadata 2; `groups_measured`, `reportable`, `bytes`, `failures`, `unresolved_ownership` all 0 | **Zero-work regressions**: a drop in reportable groups or bytes, or any non-zero `failures` / `unresolved_ownership` |
| `timings_observed_seconds` | discovery 0.00063, locate 0.02624, metadata 0.02703, source_activity 0.00083, elapsed 0.07 | Per-phase performance drift, **for visibility only** |
| `release_binary_bytes` | 12125624 | Binary size blowup |
| `normal_dependency_count` | 177 | Dependency creep |

The script's own docstring draws the line that makes the design defensible, and
it is worth quoting because it is the difference between a useful gate and a
flaky one. It keeps "two kinds of evidence … deliberately separate because they
have different failure meanings":

- **Semantic counters are hard regression evidence.** They are deterministic for
  a fixed fixture, so a change is a real behaviour change and fails the gate.
- **Wall-clock timings are trend evidence only.** They are recorded and diffed
  "for visibility, never used as a hosted pass/fail threshold, because a shared
  runner's timing is dominated by noise that has nothing to do with this code."

The counter fields are the load-bearing part, and they are the part a release
reviewer should actually read. A change that makes the tool *silently fail to
find work* — the worst possible release regression — shows up here as
`reportable: 0` or a non-zero `failures` count, and nothing else in this document
would surface it. `dependency_count` also refuses to record a failed `cargo tree`
as zero, so a broken measurement cannot silently become a passing baseline
(`scripts/release-benchmark.py:162-165`).

The fixture is built by the script rather than borrowed from the test suite, so
the numbers describe a documented, reproducible workload instead of whatever a
test happens to construct (`scripts/release-benchmark.py:17-19`).

## 7. Release identity gates

This is where the project has been most rigorous, and the reason is a specific
failure mode: **building artefacts from a commit that is not the tagged commit**.
In that case the published bytes were never qualified. Every test that ran
against the tag was run against different bytes than the ones a user would
execute, and no amount of green CI afterwards would be evidence.

### 7.1 `scripts/check-release-identity.py`

This is the machine-checked tag/source identity gate, and it is the most
direct answer to "could the published bytes be unqualified?".

- `check_identity(tag, expect_revision)` (line 91) is the core. It rejects a tag
  that is not an exact `v<major.minor.patch>` with no pre-release or build
  metadata; a tag version that disagrees with `Cargo.toml`; a `Cargo.lock` that
  records a different version ("a `--locked` release build would fail, or would
  build a different version than the tag names"); a tag that does not resolve to
  a commit; a tag whose commit is not the revision being released; and a
  `CHANGELOG.md` with no entry for that version.
- `check_clean_tree()` (line 147) rejects a dirty checkout, with the reasoning
  stated in its own docstring: "A dirty tree means the bytes being published are
  not the bytes that were reviewed, which is the whole failure this gate exists
  to prevent." It also names the v0.1.1 deviation that motivated it — the tag
  held the reviewed bytes but publication ran from a later commit.
- `check_manifest(manifest_path, tag, expect_revision)` (line 169) binds the
  release manifest that ships beside the binaries: its `release_id` must equal
  the tag, its `source_revision` must equal the revision being released, and its
  `product_id` must name this product. The docstring connects it back to §2 and
  §3: if the manifest disagrees, "the updater's identity check and the
  installer's version check are reasoning about bytes from a different commit
  than the one being promoted."
- `self_test()` (line 209) proves the gate rejects a bad setup, the same
  premise-negative pattern the installer suite adopted after C012.

The three things that would go wrong without this gate, in increasing severity:
a release cut from a dirty tree; a release cut from a commit other than the tag;
and a release whose published manifest names a different source revision than
the binaries beside it. The first two make the green CI history irrelevant. The
third is subtler and is the one that would corrupt the *updater's* reasoning on
every user machine.

### 7.2 `scripts/gen-release-workflow-shape.py` and `workflow-shape.json`

`release/eggpack/workflow-shape.json` (272 lines) is the static, identity-free
seam that `eggpack ci generate --workflow-shape` and `eggpack ci check` consume.
`scripts/gen-release-workflow-shape.py` **derives** it from the other Eggpack
inputs rather than maintaining it by hand, and its own docstring gives the
reason: a hand-maintained shape "would make it a second copy of the producer
facts already held in `pack.toml`, `build-bindings.toml`,
`qualification-bindings.toml`, `consumer-validators.json`, and
`distribution.toml`, which is exactly the drift this subsystem is meant to
eliminate."

`release-drift.yml` runs it with `--check` ("Check reusable workflow shape is
derived from the Eggpack inputs"), so a hand-edit to the checked-in shape, or an
edit to any input that does not regenerate the shape, fails CI.

This is what makes "the workflow still stages rather than publishes" (§6.2) a
property of the actual file rather than of the generator's intent. The script
also states its own limit: it "adds no producer semantics: it copies validated
input documents verbatim and only chooses canonical ordering and the staging
intent."

### 7.3 `scripts/validate-staged-release.py`

It runs **after** the build workflow has staged a draft and **before** anything is
published — its own docstring: "Run after `release-binaries.yml` stages a draft
and before anything is published. Nothing here publishes; it only inspects and
reports" (`scripts/validate-staged-release.py:3-5`). That position in the
sequence is the whole reason the draft-staging design in §6.2 is worth having:
this is the last point at which a disagreement can still be fixed.

Its six checks, in the script's own order:

1. **Exact inventory** — five binaries, five `.sha256` sidecars,
   `release-manifest.json`, and both bootstrap installers. A missing *or extra*
   asset is a hard failure, because "exactly the required qualified inventory"
   is a publication precondition.
2. **Sidecar integrity** — every sidecar must name its own asset and its digest
   must match the bytes actually served.
3. **Manifest agreement** — `release-manifest.json` must agree with the bytes for
   every asset it lists, by size and SHA-256. A disagreement is treated as a
   stale-build indicator.
4. **Contract agreement** — asset names must be exactly what
   `release/eggpack/distribution.toml` expands to. Per the docstring, "This is
   the check that the installer and the updater point at the same release
   contract" — the §2/§3 coupling in its single most direct form.
5. **Static ABI evidence** for the two Linux targets: the maximum glibc version
   each binary actually requires, read from the binary and labelled as static
   rather than runtime evidence. This is the counterpart to the glibc
   non-claim rule in §6.1.
6. **Real installer qualification on this host** — the staged artefacts are served
   over a local HTTP fixture and the product's own `packaging/install.sh` is run
   against them, so the qualification exercises the real bytes rather than a
   synthetic candidate.

Check 6 is what the closure record means by "including a real installer run on
this host" (6/6 against `v0.1.2`,
`plans/closure/distribution-release-update/c014-status.md:76`).

**Stated limitation, from that same record:** step 6 runs **only on the host
platform**. The other four targets' installer paths are covered by the contract
check and hosted CI, not by this script (same file, line 196 and line 212). So
the live, real-bytes installer evidence is single-platform; the five-target
coverage for the installer contract comes from §3.4's fixtures and the
`check-installer-contract.py` static check instead.

### 7.4 `release-drift.yml`

`.github/workflows/release-drift.yml` ("Release drift guard") runs on every push
and PR to `main` and installs the pinned Eggpack tool at the exact revision
recorded in `release/eggpack/github-policy.json:48-50`. Its checks, in order:

| Check | What it detects |
| --- | --- |
| `gen-release-workflow-shape.py --check` | The checked-in workflow shape no longer matches what the Eggpack inputs derive |
| `eggpack ci check` (shape, contract, github-policy, workflow) | The generated workflow no longer matches the Eggpack configuration — the external producer's own view |
| `check-release-contract.py` | The packaging, installer, and workflow contract has drifted |
| `check-release-contract.py --self-test` | The toolchain rule has rotted into a check that always passes |
| `check-release-identity.py --self-test` | The identity gate has rotted into a check that always passes |
| `check-installer-contract.py` | The installers have drifted from the contracted asset set |
| `check-post-release-smoke-contract.py` (+ `--self-test`) | The post-release smoke matrix has drifted from the release runners, or that check has rotted |

The two `--self-test` invocations are the part that distinguishes this workflow
from a list of assertions. The CI comment states the intent: "The tag/source
identity gate and the exact-toolchain rule are both verified in the failing
direction here, so neither can rot into a check that always passes"
(`release-drift.yml:49-51`). That is the C012 lesson generalised to the whole
release surface — a gate that has only ever been observed to pass has not been
shown to work.

Drift detection exists because a release workflow is a long-lived file whose
assumptions decay. A change to the CLI surface, the allowlist, the asset contract,
or the toolchain can invalidate a release that qualified correctly on its own
terms. Separating "the release was broken" from "the world moved after the
release" is otherwise impossible — both present as the same red build.

### 7.5 The chain of evidence, and which links are machine-checked

The claim is: **the public bytes are the qualified bytes.** The chain, with each
link's enforcement:

| # | Link | Machine-checked? | Enforced by |
| --- | --- | --- | --- |
| 1 | Tag ↔ source revision are the same commit | **Yes** | `check-release-identity.py:check_identity` |
| 2 | The tree being released is clean | **Yes** | `check-release-identity.py:check_clean_tree` |
| 3 | Manifest and lockfile agree on the version | **Yes** | `check-release-identity.py:crate_version`, `locked_version` |
| 4 | `Cargo.toml` version ↔ `CHANGELOG.md` entry ↔ git tag | **Yes** | `check-release-identity.py:105-113`, `136-143` |
| 5 | The build runs in a pinned, recorded toolchain (`+1.99.0`, `cargo-zigbuild 0.23.3`) | **Yes** | `check_release_toolchains` against `pack.toml`; `--self-test` in drift CI |
| 6 | The workflow cannot publish; it stages a draft | **Yes** | `check_workflow_cannot_publish` + `workflow-shape.json` + `eggpack ci check` |
| 7 | The staged draft's inventory, digests, manifest, and contract all agree | **Yes** | `validate-staged-release.py` 1-5, before publication |
| 8 | The published installer verifies the staged digest, on real bytes | **Yes, one host platform** | `validate-staged-release.py:6`; §3.4 fixtures for the other targets |
| 9 | Each target's binary actually runs and self-identifies | **Yes** | Per-target smoke + `consumer-validators.json` |
| 10 | The **installed** binary self-updates correctly, and a Cargo-managed one refuses | **No — manual** | The live rehearsal, `workflow_dispatch`-only. See §8 |

Links 1-9 are machine-checked, with link 8's live-installer half limited to one
host platform and link 5's timing data explicitly excluded from the pass/fail
line (§6.4). **Link 10 is not machine-checked at all, and link 10 is where both
shipped defects were found.** The one step that verifies the composed system
against real published bytes is the one step a human has to remember to trigger.

## 8. Post-release verification

### 8.1 What is checked

Two distinct things live under this heading, and conflating them would be a
mistake.

**The per-target artefact smoke** — the release-side consumer validation. All five
targets in `release/eggpack/consumer-validators.json` invoke
`scripts/smoke-release-candidate.py`, with a 180-second timeout and 64 KiB
output limits per target. The per-target release step itself is deliberately
minimal — `--version`, 10-second timeout
(`release/eggpack/qualification-bindings.toml`) — and the reason is stated in that
file's own comment: a single fixed argv cannot also cover `--help`, a bounded
read-only scan, and JSON output, so the richer runtime smoke is expressed once,
portably, as the per-target consumer validator. A failure here means a published
artefact does not run or does not identify itself as the version it claims to be
— a real signal about the published bytes that no pre-publication gate can give
you.

**The live self-update rehearsal** —
`.github/workflows/post-release-smoke.yml` driving `scripts/post-release-smoke.sh`.
This is a different and much stronger test, and the script's own header states
what it is: "Real, networked self-update rehearsal against two *public*
releases… it exercises the **released** updater, not a freshly built copy." The
binary under test is the public `$FROM_VERSION` asset verified against its
published sidecar, and it self-updates to the public `$TO_VERSION`.

It runs two scenarios in isolated state:

- **self-managed** — a copy of the published binary outside any Cargo root, which
  must self-update in place, and whose **post-update digest must equal the
  published `$TO_VERSION` asset digest exactly**.
- **cargo-managed** — the same version installed by Cargo into an isolated root,
  which must **refuse** as `cargo_managed`, name the manager command for the new
  version, and **leave the managed bytes untouched**.

Those two scenarios are, respectively, the direct assertions of the §2
distinguishability property. The second one is C017's regression test, written
down as a live scenario.

The workflow runs a five-target matrix on purpose-matched runners
(`post-release-smoke.yml:32-42`: `macos-14`, `ubuntu-24.04-arm`,
`macos-15-intel`, `windows-latest`, `ubuntu-latest`), and that matrix is itself
guarded: `scripts/check-post-release-smoke-contract.py` asserts the runners
against the generated release workflow "so this matrix cannot silently drift into
a second, unguarded target list." The workflow holds `permissions: contents: read`
only and needs no publication credentials, and the script's design constraints
are explicit that a network failure is a failure, that nothing runs under
`|| true` where a failure would be hidden, and that **the premise of every
scenario is asserted before its behaviour is** — the installed binary's identity
and digest are proven to be the published ones before `update` is asked to do
anything.

### 8.2 The live self-update rehearsal: manual, and that is the finding

**The rehearsal is manual. It is `workflow_dispatch`-only, with no `push` or
`pull_request` trigger, and nothing else in `.github/workflows/` performs it.**
The workflow says so itself: "Manual only. This rehearses the *real* self-update
transaction against two public releases, so it must be triggered deliberately
during release qualification and must not run on every push"
(`post-release-smoke.yml:3-5`). It takes `from_version` and `to_version` as
required inputs, defaulting to `0.1.1` and `0.1.2` — two *already published*
versions, which is precisely why it cannot be a CI job: it can only run once the
thing it tests already exists in public.

The machinery is good — five targets, purpose-matched runners, two scenarios,
asserted premises, `contents: read`, no `|| true`. **The trigger is a human
decision, and nothing enforces that anyone makes it.**

This matters because the rehearsal is **the only thing that has ever caught real
self-update defects**:

- **`v0.1.1 → v0.1.2`** found C016: the identity check could never match, so
  self-update failed. It failed *loudly* and mutated nothing, which is why 0.1.1
  and 0.1.2 were judged correct-except-for-the-updater and not yanked.
- **`v0.1.3 → v0.1.4`** found C017: a Cargo-managed binary was misdetected and
  **silently replaced** — a silent success rather than a loud failure, and a
  violation of the exact property in §2. Both versions shipped published.
- `v0.1.4 → v0.1.5` was an *expected-failure* rehearsal (it must fail, because
  the installing binary would still be the broken published 0.1.4), and
  `v0.1.5 → v0.1.6` was the proving rehearsal that found two further defects.

The diagnosis behind C017 is stated in the suite itself: the case "was green for
an entire release cycle precisely because nothing ever checked the guards in the
direction that matters: with the stub *not* reachable, and with the real tool
winning the lookup" (`packaging/tests/test_installers.py:786-791`). The fixtures
had been built to pass, not to be shown capable of failing. The premise-negative
harness added afterwards is the right structural answer and it now runs in CI —
but it addresses the *static* half. The rehearsal, which is where the defect
actually lived, is still a human action against a published release.

The consequence, stated without softening: **a defect of the C016 or C017 class
can ship again.** The scenario for it is short. A maintainer publishes v0.1.7. CI
is green — the workflow cannot even attempt a rehearsal, because
`post-release-smoke.yml` has no push trigger and the default `to_version` still
points at 0.1.2. If nobody remembers to dispatch the workflow with
`--from 0.1.6 --to 0.1.7`, the release ships unverified on precisely the
dimension where the two shipped defects lived, and no gate in §7 fails.

Two things make this worse than it first appears. First, the shipped defects were
*silent* (C017) or *loud but untested* (C016) — neither would show up as a red
build, a failed install, or a failed smoke, because both live in the commit path
that no pre-publication check exercises. Second, the rehearsal is the one place
where the §2 provenance property is tested end to end against real published
bytes; everything else in this document is static analysis of a manifest or a
script.

What the project has done about the *class* of problem is real and worth
recording: the fixtures were made premise-negative, the identity gate got a
`self_test()`, and the rehearsal itself was hardened — C017's own record notes
that the `v0.1.5 → v0.1.6` run **failed on the harness, not the product** on its
first attempt, and that this was the third harness bug the rehearsal line had
found. The lesson the project drew is correct: a live rehearsal is a program, and
like any program its first runs find bugs in itself. But the gap is the trigger,
not the program. The release line's own status record rates the updater rehearsal
**partial** (`plans/closure/distribution-release-update/010d-status.md:83`) and
notes that the commit path "cannot be until a version newer than 0.1.1 exists" —
a constraint that was true when written and remains structurally true, because it
is a function of what has been published, not of what the automation supports.

## 8.4 Hosted release automation (Phase 11)

Three workflows were added in Phase 11, and the common property is that each one
turns a previously manual or undocumented step into an enforcement point **while
keeping the human publication action where it was**.

| Workflow | Subscribes to | Owns | Cannot |
| --- | --- | --- | --- |
| `validate-staged-release.yml` | `workflow_run` on `Eggpack candidate builds` | The hosted enforcement point for `validate-staged-release.py` (§7.3) | Publish. Holds `contents: read` only. |
| `post-release-smoke.yml` | `release: published`, plus manual dispatch | The real self-update transaction across all five targets | Mutate anything outside its own job directory |
| `qualify-cargo-selectors.yml` | weekly, dispatch, and path-filtered push/PR | Real-Cargo evidence for the selector support claim | Edit the support allowlist, or change a verdict |

`validate-staged-release.yml` exists because `validate-staged-release.py` was
"wired into nowhere" (§7.3) — a documented manual command that ran only when an
operator remembered. It lives in its own file because `release-binaries.yml` is
generated by Eggpack and protected by a drift gate. Its two quiet failure modes
are guarded statically by `check-staged-validation-contract.py`: a `push` trigger
would run the validator on an arbitrary branch, and a default-branch checkout
would validate whatever `main` currently contains while genuinely passing.

`post-release-smoke.yml` gained a `release: published` trigger in M011C, which
introduced two new failure shapes and two new pieces of tested machinery in
`resolve-smoke-transition.py`. The first is resolving a predecessor from
whatever is available rather than from a proven exact pair — hence numeric
version ordering (`0.1.9 < 0.1.10`, which string comparison gets backwards) and
a resolver that never silently skips a known-bad predecessor. The second is
ordering: publication is GitHub-release-first and `cargo publish`-second, so the
event can fire before the version the updater resolves exists. The wait is
bounded, and **exhausting it is a failure rather than a skip**, because "the
smoke did not run" and "the smoke passed" must never be confusable in a closure
record.

`qualify-cargo-selectors.yml` exists because the product fails closed for an
unknown Cargo version, which is exactly why a stale support allowlist is
invisible: nothing breaks, the product just quietly stops offering a selector.
`release/selector-qualification.json` is the single authority, the runtime
allowlist in `workspace.rs` is proved against it, the hosted matrix names its
toolchains explicitly so a dropped version is detectable, and the exploratory
lane is structurally incapable of promoting anything — see §7 of
`architecture/07-workspace.md`.

## 9. Invariants and gaps

Reported as determined, not assumed.

| Question | Determination |
| --- | --- |
| **Is the published crate guaranteed to compile and work standalone?** | Partly. `config.toml` is in the allowlist and the check enforces it, so the `include_str!` coupling holds. But the check is static: it validates the manifest, not a build of the *published package*. Nothing I found builds the staged `.crate` standalone. A file needed at runtime but outside the allowlist would fail at **compile** time if referenced via `include_str!`/`include_bytes!` (loud, CI would likely catch it), and at **runtime** if referenced by path or as a data file (quiet, and not covered by any check I found). |
| **Can `completions/` and `man/` drift from the parser?** | Yes, by construction — they are tracked generated artefacts. The `generated-docs` job in `ci.yml:64` regenerates and diffs, so drift is caught at PR time. This is the one drift risk in this component that is genuinely closed. |
| **Is the installer's download integrity verified?** | **Yes — SHA-256 sidecar, mandatory, on both installers.** `install.sh` requires a 2xx on the sidecar, requires the digest to match `^[0-9a-f]{64}$`, and compares it to the downloaded file (`install.sh:305-327`); `install.ps1` does the same via `Get-FileHash -Algorithm SHA256` (`install.ps1:125`, `225-235`). Both comment the rule in the same words: "The digest sidecar is mandatory evidence. Its absence is a hard failure, not a fallback signal." Both also *execute* the staged binary afterwards and abort if it does not report a version (`install.sh:333-337`, `install.ps1:16`). Three fixture cases pin the negative cases (`case_checksum_absent_is_fatal`, `case_malformed_digest`, `case_digest_mismatch`) and a fourth (`case_wrong_candidate`) pins that a correctly-named but wrong binary is caught. **What it is not: a signature.** See the last row. |
| **Do the two installers behave equivalently for every user-facing case?** | For the contract — supported triples, asset names, install names, mandatory sidecar, 404-only fallback, refuse-to-overwrite without an explicit force flag, post-download version check — yes, and it is machine-checked or fixture-covered on both sides. The differences that remain are shell-appropriate (destination rule, TLS mechanism) rather than behavioural. The one property I could **not** find a check for: that neither destination is ever inside a Cargo root. That property is what keeps installer-created installations updatable (§2), and it is currently enforced only by convention and by reading the source. |
| **Unsupported platform/arch — clear message or confusing 404?** | A clear message, and deliberately not a raw 404. An unrecognised OS or arch fails immediately with a pointer to `cargo install --locked` (`install.sh:130`, `install.sh:137`); a recognised family with no published binary enters the Cargo source build (`install.sh:150-154`, `install.ps1:96-100`); a 404 on a contracted asset is treated as genuine absence and also falls back, while a 5xx or transport fault stays fatal precisely so an outage cannot silently become a local build (`install.sh:277-285`, `install.ps1:208-215`). Fixtures cover the path: `case_binary_404_falls_back`, `case_cargo_missing`, `case_cargo_produces_nothing`. |
| **Is `cargo install --locked` guaranteed to resolve with zero non-registry sources?** | The lockfile is in the allowlist so resolution data ships, and the identity gate checks that `Cargo.lock` agrees with `Cargo.toml` on the version. I did **not** find a check that asserts a zero-non-registry-source count for the published crate, and the installers' own fallback builds invoke Cargo against whatever source configuration the host has. Not established. |
| **Are `Cargo.toml` version, `CHANGELOG.md` version, git tag, and published crate version checked against each other?** | **Yes, and in one function.** `check_identity` (`scripts/check-release-identity.py:91-144`) asserts, in order: the tag is an exact `v<major.minor.patch>` with no pre-release or build metadata; the tag's version equals `Cargo.toml`'s; `Cargo.lock` records the same version (with the stated reason — "a `--locked` release build would fail, or would build a different version than the tag names"); the tag resolves to an existing commit **and** points at the revision being released; and `CHANGELOG.md` has an entry for that exact version. `check_clean_tree` (line 147) then rejects a dirty checkout. The published crate version is the manifest version, so the four are tied together transitively. |
| **Any manual step in the release process?** | Yes, enumerated: (1) **publishing the staged draft** — the release workflow deliberately cannot do it, and that separation is the point; (2) **dispatching the live self-update rehearsal** against two published versions — since M011C this also runs automatically on `release: published`, with `workflow_dispatch` retained as the rehearsal surface (§8.2); (3) the staging validation's **real-installer step runs on one host platform only** — since M011B the validation itself is a hosted workflow, but that step is unchanged — the other four targets rest on the contract check and hosted CI (`c014-status.md:196,212`); (4) deciding **whether the glibc 2.17 floor may be advertised** — it is a build fact from `cargo_zigbuild` with no runtime qualification behind it, which is why the contract check requires an explicit non-claim in the README (`check-release-contract.py:486-509`). |
| **Blast radius of a compromised release — is there signature verification?** | **Closed for the first time by M011A, and narrowly.** Immutable releases are enabled for `dbowm91/cargo-cleanme` (repository policy, enabled 2026-10-05), so a published release's assets and tag can no longer be replaced, and GitHub issues a signed attestation binding the release to its repository, tag, commit, and every asset digest. `scripts/verify-release-attestation.py` verifies it and fails closed on policy-off, release-mutable, attestation-missing/unparseable, wrong-repository/tag, unattested-asset, digest-mismatch, inventory drift, and missing-or-old-`gh` states. **The trust root is still GitHub's release and attestation infrastructure.** This is not an independent maintainer-key signature and not SLSA build provenance, and it does not become one by being described loosely. A compromise of the GitHub release account still yields a self-consistent artefact every check accepts. Releases v0.1.0–v0.1.6 predate the policy, remain mutable, and must not be described as attested or recreated. |

## 10. Review checklist

For anyone changing the CLI surface, the packaging manifest, the installers, or
the release workflows.

1. **Did you change the `src/cli.rs` command surface?** Regenerate completions
   and manpages in the same commit, or the `generated-docs` job fails
   (`.github/workflows/ci.yml:64`). Adding a subcommand means a ninth manpage;
   removing one means deleting a page.
2. **Do not "normalise" the underscore prefix** on `completions/_cargo-cleanme`
   or `completions/_cargo-cleanme.ps1`. It is required by zsh and PowerShell
   respectively; removing it breaks completion loading (§4.2).
3. **Are you adding an `include_str!`/`include_bytes!` to a file outside
   `src/`?** That file must be added to the `Cargo.toml` `include` allowlist in
   the same change, or the *published* crate fails to compile while your local
   build passes. The existing instance is `include_str!("../config.toml")` in
   `src/config.rs` (`Cargo.toml:14-21`).
4. **Did you add anything to the allowlist that is maintenance-only?** The
   contract check rejects the xtask generator explicitly
   (`scripts/check-release-contract.py:468-471`); the same reasoning applies to
   any new build-time tool.
5. **Does your installer change add a triple, an asset name, or an install
   name?** `scripts/check-installer-contract.py` compares both installers
   against `release/eggpack/distribution.toml`; a change to one without the other
   fails the contract check.
6. **Does your installer change touch digest handling?** Three fixture cases
   exist specifically to prove a missing, malformed, or mismatched `.sha256`
   aborts the install (`packaging/tests/test_installers.py:766-782`). If you
   make the checksum optional, you are removing a guarantee, and the fixtures
   must be changed deliberately rather than incidentally.
8. **Are your new fixtures capable of failing?** After C012, a green fixture
   that has only ever passed is not evidence. If you add a case, prove it rejects
   a broken premise, following `self_test()`
   (`packaging/tests/test_installers.py:785`).
9. **Did you touch the release workflow?** The contract check fails if a
   publication or clobber path reappears
   (`scripts/check-release-contract.py:527`), and `release-drift.yml` fails if
   the workflow's shape stops matching what `pack.toml`, the bindings, and
   `distribution.toml` derive (`scripts/gen-release-workflow-shape.py --check`).
   Do not add a publish step to the build workflow; the draft-staging separation
   is the property that makes the identity gates meaningful.
10. **Does your change affect tag, revision, manifest, or lockfile identity?**
    `check_identity` requires the tag's version to equal `Cargo.toml`'s, the
    lockfile to agree, the tag to resolve and to point at the revision being
    released, and the changelog to have that version's entry
    (`scripts/check-release-identity.py:91-144`); `check_clean_tree` (line 147)
    rejects a dirty tree. Run these locally before tagging.
11. **Are you adding a README claim about the glibc floor?** Mentioning it
    requires an explicit non-claim nearby
    (`scripts/check-release-contract.py:486-509`). The floor is a build fact
    until runtime qualification exists.
12. **Are you relying on the smoke as proof the updater works?** It is not. The
    per-target consumer validator proves the artefact runs and self-identifies
    (`release/eggpack/consumer-validators.json`); it does not exercise the
    self-replacement commit path. Only the live rehearsal covers that, and it is
    `workflow_dispatch`-only (`post-release-smoke.yml:6-7`).
13. **If you cut a release, dispatch the rehearsal before you consider the line
    closed.** C016 and C017 both shipped because nothing forced this step. The
    `v0.1.5 → v0.1.6` pattern — run the rehearsal, expect the first run to fail
    on the harness, fix the harness, re-run — is the documented practice, and
    it has found real defects each time.

See also [12-self-update.md](12-self-update.md) for the provenance classifier
these installers must satisfy, and
[14-testing-and-verification.md](14-testing-and-verification.md) for the
verification-side treatment of the same scripts.
