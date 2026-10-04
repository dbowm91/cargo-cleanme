# M010B — Bootstrap Installers and Release Artifact Qualification

Status: closed

Repository baseline: `b4662af` (M010A implementation commit; closure `plans/closure/distribution-release-update/010a-status.md`).

Runtime qualification of the real release bytes was deferred to M010D at
closure and was discharged there by release workflow run `37232595211`; see the
addendum in `plans/closure/distribution-release-update/010b-status.md`.

Source roadmap: Phase 10 — Distribution and operational polish

Hard dependency: M010A closed.

Interface dependency: Eggpack direct-release contract and exact-release bootstrap behavior established by M010A.

## 1. Objective

Add Gregg/Eggsact-style public first-install semantics for cargo-cleanme while keeping generic producer mechanics in Eggpack. Produce and qualify `install.sh` and `install.ps1` as product-owned wrappers over the same release asset contract used by Eggpack.

## 2. Canonical install semantics

The public installer is binary-first.

For a supported host:

1. resolve latest stable or explicit requested `X.Y.Z`;
2. map the host to the canonical Eggpack target;
3. acquire the exact release artifact and its release evidence;
4. verify declared size/SHA-256;
5. mark executable as needed;
6. run the candidate with `--version`;
7. require exactly `cargo-cleanme X.Y.Z`;
8. place it in the selected install scope;
9. report the resulting path and PATH guidance.

Cargo fallback is allowed only when:

- the host has no published binary target; or
- the exact selected binary is genuinely absent.

A checksum/manifest failure, candidate mismatch, TLS/download failure, timeout, malformed metadata, or destination conflict is a hard failure and must not fall back to Cargo.

## 3. Product-owned wrappers

Add:

- `packaging/install.sh`
- `packaging/install.ps1`

POSIX behavior should support latest by default and `--version X.Y.Z` for pinned installation.

Default destination:

- non-root: `$HOME/.local/bin/cargo-cleanme`;
- already root: `/usr/local/bin/cargo-cleanme`.

Do not invoke `sudo` internally. If a user wants a system install, they run the wrapper elevated themselves.

PowerShell should use a user-local application/bin location by default and a clearly documented elevated/system location only when already running elevated. It must not silently alter system PATH; emit bounded guidance if the install directory is absent.

## 4. Eggpack relationship

Eggpack's exact-release bootstrap renderer remains producer evidence for the finalized direct artifact. The public wrappers own latest-version selection, explicit version selection, source/Cargo fallback policy, destination scope, and user presentation.

Do not copy Eggpack's release-contract parsing into shell. Keep shell mapping mechanically checked against `release/eggpack/distribution.toml` using the existing Eggpack conformance/observed-mapping facilities or a generated bounded fixture.

## 5. Cargo fallback

Build into an invocation-owned temporary Cargo root:

~~~text
cargo install cargo-cleanme --locked --root <private-temp-root>
~~~

For a requested exact version, add an exact `--version =X.Y.Z`.

Never install directly into the final destination until the staged Cargo candidate passes the same exact `--version` validation as a downloaded binary.

Clean only invocation-owned temporary state.

## 6. Release candidate qualification

M010B must qualify the actual release artifact bytes for the five M010A targets. At minimum each candidate must pass:

- `--version`;
- `--help`;
- direct and Cargo-external-subcommand argv normalization where host tooling allows;
- bounded read-only scan fixture;
- `--format json --no-progress` smoke with exactly one JSON document on stdout.

No destructive cleanup is required for installer qualification.

## 7. Negative tests

Cover at least:

- unsupported OS/architecture;
- exact requested version malformed;
- binary 404 -> Cargo fallback;
- checksum/manifest 404 when binary exists -> hard failure;
- malformed/incorrect digest;
- wrong candidate product or version;
- transport failure/timeout;
- Cargo missing when fallback is required;
- Cargo build succeeds but expected binary is absent;
- existing foreign destination;
- non-writable destination;
- PATH missing destination directory;
- temporary-state cleanup on failure.

PowerShell and POSIX tests may use local fixture servers/files rather than public GitHub.

## 8. Compatibility and security

Installer scripts must not weaken platform TLS defaults or accept HTTP downgrade. Download tooling/config files that can silently inject options must be disabled where practical.

No installer performs cleanup of Cargo projects, modifies cargo-cleanme configuration, or touches discovery state.

## 9. Verification

Run existing cargo-cleanme CI plus script syntax/lint checks available on the host, deterministic fixture tests for both wrappers, Eggpack contract conformance, and hosted installer smoke on Linux/macOS/Windows.

Use release-candidate artifacts produced from an exact source revision; do not qualify locally rebuilt bytes and claim them as release evidence for different staged bytes.

## 10. Documentation

README should document:

- one-command install for Linux/macOS;
- PowerShell install;
- pinned version syntax;
- direct-download/source fallback behavior;
- no internal sudo;
- PATH behavior;
- Cargo source-install fallback;
- supported target matrix and known platform limitations.

## 11. Acceptance criteria

- both public wrappers install a verified candidate on their qualified platforms;
- supported binary hosts use the binary path;
- unsupported/missing-binary hosts use Cargo only under the explicit fallback conditions;
- integrity/identity/transport failures fail closed;
- no internal privilege escalation;
- wrapper target mapping is mechanically checked against Eggpack;
- exact release artifacts pass runtime qualification on the required target matrix.

## 12. Stop conditions

Do not ship the wrapper if shell/PowerShell logic becomes a second independent release schema, if candidate identity cannot be validated before placement, if the installer overwrites a destination without explicit safe policy, or if fallback conditions cannot distinguish absence from verification/transport failure.

## 13. Closure evidence

Record wrapper tests, hosted per-platform runs, exact release-candidate digests, Eggpack mapping conformance, Cargo-fallback fixture evidence, destination behavior, and known unsupported hosts.
