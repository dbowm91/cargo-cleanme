//! Bounded self-update.
//!
//! # What this module owns, and what it deliberately does not
//!
//! cargo-cleanme owns *policy*: which release is acceptable, which host this
//! is, whether this installation may be updated at all, and what the candidate
//! must say about itself. Everything mechanical is delegated to the published
//! Eggup crates, which already own it:
//!
//! - acquisition and the transport seam: `eggup-acquisition` / `eggup-curl`;
//! - digest binding, bounded candidate execution, locked ownership
//!   revalidation, and rollback: `eggup-core`.
//!
//! Nothing here re-implements a checksum, a retry, a staging directory, a lock,
//! or a rollback. Adding a second copy of any of those is the failure mode this
//! module exists to avoid.
//!
//! # Update input
//!
//! The current update input is the exact release tag, the contracted release
//! asset, and the asset's own `.sha256` sidecar. `eggup-eggpack` release
//! manifest projection is *not* wired yet: every cargo-cleanme target is a
//! single direct artifact with no bundle, so manifest projection would add
//! `eggpack-manifest` and the archive crates for no input the sidecar does not
//! already carry. Until it is wired, the honest statement is that the tag plus
//! the asset plus its sidecar digest is the whole input, and that a real
//! release workflow run is still required to prove end-to-end agreement between
//! what Eggpack stages and what this module consumes.
//!
//! # Transport
//!
//! `eggup-eggfetch` is the only production transport.
//!
//! It was not the first choice. `eggup-curl` is smaller, because
//! `eggup-acquisition` has zero dependencies, and avoiding an embedded HTTP/TLS
//! stack in a CLI that self-updates rarely is a real advantage. It was
//! replaced because `CurlConfig` in `eggup-curl` 0.1.2 exposes no User-Agent
//! seam and the adapter clears the child environment, so every request goes out
//! as `curl/x.y`. The crates.io registry answers **403** to any non-descriptive
//! User-Agent, so the curl transport could not read the version authority at
//! all. This was found by the first live release smoke, not by the fixture
//! suite, because the fixture suite never talks to the registry.
//!
//! A transport that cannot perform the required transaction is not qualified,
//! however small it is. `EggfetchConfig::user_agent` is the seam the
//! requirement needs, and it costs an embedded TLS stack. That cost is
//! measured and recorded rather than assumed.
//!
//! Exactly one production transport is wired. The curl path is deliberately
//! not kept as a fallback, because a fallback that cannot reach the authority
//! is a second way to fail for no benefit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use eggup_acquisition::{
    AcquisitionError, AcquisitionRequest, AcquisitionTransport, CancelFlag, FetchLimits,
    FetchOutcome,
};
use eggup_core::{
    AbsentPolicy, ArtifactMember, ArtifactSet, CommitOwnership, ExactIdentityValidator,
    InstallPlan, IntegrityRequirement, MemberId, Ownership, OwnershipVerifier, PermissionsIntent,
    PreparedTransaction, ProductId, ReleaseId, Sha256Manifest, TransactionDisposition,
    TransactionReceipt,
};

use crate::error::AppError;

/// Published triples, in the contract's canonical order.
///
/// This is a compile-time copy of `release/eggpack/distribution.toml`, not an
/// independent schema. `scripts/check-release-contract.py` fails the build if
/// the two ever disagree, because a divergent copy here would produce URLs that
/// the release does not serve.
const PUBLISHED_TARGETS: &[(&str, &str)] = &[
    ("aarch64-apple-darwin", "cargo-cleanme-aarch64-apple-darwin"),
    (
        "aarch64-unknown-linux-gnu",
        "cargo-cleanme-aarch64-unknown-linux-gnu",
    ),
    ("x86_64-apple-darwin", "cargo-cleanme-x86_64-apple-darwin"),
    (
        "x86_64-pc-windows-msvc",
        "cargo-cleanme-x86_64-pc-windows-msvc.exe",
    ),
    (
        "x86_64-unknown-linux-gnu",
        "cargo-cleanme-x86_64-unknown-linux-gnu",
    ),
];

/// The crate whose published version is the release version authority.
const PRODUCT: &str = "cargo-cleanme";

/// Exact byte ceiling for the crates.io metadata document.
///
/// The real document is a few kilobytes. 1 MiB is generous while still making
/// the fetch finitely bounded, which is the point.
const MAX_METADATA_BYTES: usize = 1024 * 1024;

/// Exact byte ceiling for one release artifact.
const MAX_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(120);

/// The exact identity string a release candidate must print.
fn expected_identity(version: &str) -> String {
    format!("{PRODUCT} {version}\n")
}

// ---------------------------------------------------------------- host mapping

/// The contracted triple for the host this binary is running on, if published.
///
/// Mirrors the mapping in `packaging/install.sh` and `packaging/install.ps1`.
/// A host with no published artifact is a `CargoInstallOnly` provenance state,
/// not an error: the wrapper can still say so accurately.
pub fn host_target() -> Option<&'static str> {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Some("x86_64-unknown-linux-gnu")
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Some("aarch64-unknown-linux-gnu")
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Some("x86_64-apple-darwin")
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some("aarch64-apple-darwin")
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Some("x86_64-pc-windows-msvc")
    }
    // Any other host is a `CargoInstallOnlyHost` provenance state, not an error.
}

/// The contracted asset name for a published triple.
fn asset_for_target(triple: &str) -> Option<&'static str> {
    PUBLISHED_TARGETS
        .iter()
        .find(|(t, _)| *t == triple)
        .map(|(_, asset)| *asset)
}

// -------------------------------------------------------------- release URLs

fn crates_io_metadata_url() -> String {
    format!("https://crates.io/api/v1/crates/{PRODUCT}")
}

fn release_asset_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/dbowm91/{PRODUCT}/releases/download/{tag}/{asset}")
}

// ------------------------------------------------------------------ outcomes

/// Why this installation cannot be updated, in terms a user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// The running executable is positively recorded in a Cargo installation
    /// root's package metadata. Cargo owns this file and its bookkeeping;
    /// replacing it here would leave `cargo install --list` and uninstall
    /// lying. The correct command is printed instead.
    CargoManaged { bin_root: PathBuf, version: String },
    /// The running executable is not Cargo-managed and is positively identified
    /// by its exact digest, which is revalidated under Eggup's mutation lock
    /// immediately before replacement.
    VerifiableSelfManaged { digest: [u8; 32] },
    /// This host has no published artifact, so there is nothing to update to.
    CargoInstallOnlyHost { triple_hint: String },
    /// The running executable could not be positively identified, so
    /// replacement cannot be proven safe.
    UnprovableOwnership { detail: String },
}

impl Provenance {
    /// A short, stable, machine-readable code for JSON output.
    pub fn code(&self) -> &'static str {
        match self {
            Provenance::CargoManaged { .. } => "cargo_managed",
            Provenance::VerifiableSelfManaged { .. } => "self_managed",
            Provenance::CargoInstallOnlyHost { .. } => "cargo_install_only_host",
            Provenance::UnprovableOwnership { .. } => "unprovable_ownership",
        }
    }

    /// The exact command the user should run instead, when there is one.
    pub fn remediation(&self) -> Option<String> {
        match self {
            Provenance::CargoManaged { .. } => {
                Some(format!("cargo install {PRODUCT} --locked --force"))
            }
            Provenance::CargoInstallOnlyHost { .. } => {
                Some(format!("cargo install {PRODUCT} --locked"))
            }
            Provenance::UnprovableOwnership { .. } => Some(format!(
                "Reinstall with the published installer, then retry: \
                 https://github.com/dbowm91/{PRODUCT}"
            )),
            Provenance::VerifiableSelfManaged { .. } => None,
        }
    }
}

/// A verifier that proves ownership of the live destination by exact prior
/// SHA-256 content.
///
/// This is the whole ownership proof for a self-update: the file being replaced
/// must hash to the digest observed before the transaction began, re-read by
/// Eggup under the mutation lock. It never trusts the file name, the `PATH`
/// order, or the directory it lives in.
#[derive(Debug, Clone)]
struct LiveDigestVerifier {
    expected: HashMap<MemberId, [u8; 32]>,
}

impl OwnershipVerifier for LiveDigestVerifier {
    fn verify(&self, _member: &MemberId, destination: &Path) -> Ownership {
        let Some(expected) = self.expected.get(_member) else {
            return Ownership::Unknown;
        };
        match std::fs::symlink_metadata(destination) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ownership::Absent,
            Err(_) => Ownership::Unknown,
            Ok(meta) if meta.file_type().is_symlink() => Ownership::Foreign,
            Ok(meta) if !meta.is_file() => Ownership::Foreign,
            Ok(_) => match eggup_core::hash_file(destination) {
                Ok(actual) if &actual == expected => Ownership::Owned,
                Ok(_) => Ownership::Foreign,
                Err(_) => Ownership::Unknown,
            },
        }
    }
}

// ----------------------------------------------------------------- plan input

/// The resolved update input for one run.
#[derive(Debug, Clone)]
pub struct UpdatePlan {
    pub from_version: String,
    pub to_version: String,
    pub target: String,
    pub asset: String,
    pub tag: String,
    pub provenance: Provenance,
}

/// Where the run stopped, in a form that is stable enough to reason about.
#[derive(Debug)]
pub enum UpdateError {
    /// The chosen production transport is unavailable. The manager command is
    /// printed rather than silently switching transports.
    TransportUnavailable { detail: String },
    /// crates.io did not report a usable stable version.
    NoPublishedStableVersion { detail: String },
    /// The published stable version is not newer than the running one.
    AlreadyCurrent { current: String, published: String },
    /// The published stable version is *older* than the running one.
    ///
    /// This happens when the running binary is a locally bumped or development
    /// build. Replacing it with the older published release would be a
    /// downgrade, which `update` must never perform. The registry is the
    /// version authority for *releases*, not for a build that was never
    /// published.
    NewerThanPublished { current: String, published: String },
    /// The acquisition itself failed for a non-absence reason.
    Acquisition { stage: String, detail: String },
    /// The `.sha256` sidecar was absent, malformed, or did not describe the
    /// asset.
    ReleaseEvidence { detail: String },
    /// Eggup refused preparation, integrity, validation, or commit.
    Transaction { detail: String },
    /// A transaction committed but its receipt is not a clean `Committed`.
    Receipt {
        disposition: &'static str,
        detail: String,
    },
    /// This installation may not be updated by this tool.
    Provenance { provenance: Provenance },
    /// The running executable is already the target version, but only after
    /// the destination was positively identified.
    HostUnsupported { detail: String },
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateError::TransportUnavailable { detail } => {
                write!(f, "the update transport is unavailable: {detail}")
            }
            UpdateError::NoPublishedStableVersion { detail } => {
                write!(
                    f,
                    "the registry reported no usable stable version: {detail}"
                )
            }
            UpdateError::AlreadyCurrent { current, published } => {
                write!(
                    f,
                    "already at the latest stable version ({current}; published is {published})"
                )
            }
            UpdateError::NewerThanPublished { current, published } => {
                write!(
                    f,
                    "this build ({current}) is newer than the published release \
                     ({published}); refusing to downgrade it"
                )
            }
            UpdateError::Acquisition { stage, detail } => {
                write!(f, "could not {stage}: {detail}")
            }
            UpdateError::ReleaseEvidence { detail } => {
                write!(f, "release integrity evidence is unusable: {detail}")
            }
            UpdateError::Transaction { detail } => {
                write!(f, "the update transaction failed: {detail}")
            }
            UpdateError::Receipt {
                disposition,
                detail,
            } => {
                write!(
                    f,
                    "the update did not commit cleanly ({disposition}): {detail}"
                )
            }
            UpdateError::Provenance { provenance } => {
                write!(
                    f,
                    "this installation cannot be updated by {PRODUCT} update ({})",
                    provenance.code()
                )
            }
            UpdateError::HostUnsupported { detail } => {
                write!(
                    f,
                    "this host has no published cargo-cleanme binary: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<UpdateError> for AppError {
    fn from(value: UpdateError) -> Self {
        match value {
            // A refused update is a user-actionable condition, not a crash.
            UpdateError::Provenance { provenance } => {
                AppError::Provenance(remediation_text(&provenance))
            }
            other => AppError::Update(other.to_string()),
        }
    }
}

fn remediation_text(provenance: &Provenance) -> String {
    match provenance.remediation() {
        Some(command) => format!(
            "{} is {}; run this instead:\n  {command}",
            PRODUCT,
            provenance.code()
        ),
        None => format!("{PRODUCT} is {}", provenance.code()),
    }
}

// ------------------------------------------------------------------ discovery

/// Classify the running installation without consulting a package manager's
/// opinion beyond the recorded metadata it owns.
///
/// Cargo's own `~/.cargo/.crates.toml` is read when present: it is the
/// authoritative record of what Cargo installed and owns. Nothing is inferred
/// from `PATH` order or the executable's base name.
pub fn classify_provenance(current_exe: &Path) -> Provenance {
    classify_provenance_in(current_exe, default_cargo_home())
}

/// The Cargo home this process would use, or `None` if it cannot be determined.
fn default_cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
}

/// The classification itself, with the Cargo home supplied explicitly.
///
/// Taking the home as a parameter rather than reading it from the environment
/// is what makes Cargo-managed detection testable at all: `set_var` is unsafe
/// in edition 2024, and a test that mutated global state to check a pure
/// decision would be both unsound and order-dependent.
pub fn classify_provenance_in(current_exe: &Path, cargo_home: Option<PathBuf>) -> Provenance {
    let Some(target) = host_target() else {
        return Provenance::CargoInstallOnlyHost {
            triple_hint: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        };
    };
    let _ = target;

    // An explicitly supplied Cargo home is authoritative when the executable
    // really is inside it. Being in a Cargo root that records nothing about
    // this package is itself a reason to refuse, not a reason to adopt the
    // file: the whole premise of `CargoManaged` is that Cargo's bookkeeping
    // must not be made to lie.
    if let Some(home) = cargo_home.as_deref() {
        let bin_root = home.join("bin");
        if current_exe.parent() == Some(bin_root.as_path()) {
            return match read_cargo_record(home, PRODUCT, binary_name(current_exe)) {
                CargoRecord::Recorded { version } => Provenance::CargoManaged { bin_root, version },
                CargoRecord::DoesNotClaim { reason } => Provenance::UnprovableOwnership {
                    detail: format!(
                        "{} is inside the Cargo bin root {} but {reason}",
                        current_exe.display(),
                        bin_root.display()
                    ),
                },
                // Both of these are uncertainty, and neither may fall through
                // to the self-managed path. A Cargo home whose `bin` holds this
                // executable is manager-sensitive by construction: if the
                // record that would settle ownership cannot be read, the only
                // honest answer is that ownership is unproven.
                CargoRecord::Absent => Provenance::UnprovableOwnership {
                    detail: format!(
                        "{} is inside the Cargo bin root {} but Cargo has no record file there",
                        current_exe.display(),
                        bin_root.display()
                    ),
                },
                CargoRecord::Uninterpretable { detail } => Provenance::UnprovableOwnership {
                    detail: format!(
                        "{} is inside the Cargo bin root {} and {detail}",
                        current_exe.display(),
                        bin_root.display()
                    ),
                },
            };
        }
    }

    // Otherwise look for a Cargo root anywhere above the executable. This is the
    // `cargo install --root DIR` shape, which is what every hermetic install
    // script, CI job, and container image uses -- and where the process
    // `CARGO_HOME` is frequently *not* where the running binary came from.
    //
    // Two states are distinguished, and the difference is the whole point:
    //
    // - a root with no record at all is *not a Cargo root* as far as any
    //   evidence goes, so the search continues. This is what keeps the
    //   published installer's `/usr/local/bin` and `~/.local/bin` on the
    //   self-managed path: "no Cargo evidence anywhere" must remain eligible.
    // - a root that *has* a record which cannot be interpreted is a Cargo root
    //   whose bookkeeping we failed to read. The presence of the file is the
    //   evidence; continuing past it would replace a file Cargo owns.
    for root in cargo_roots_above(current_exe) {
        match read_cargo_record(&root, PRODUCT, binary_name(current_exe)) {
            CargoRecord::Recorded { version } => {
                return Provenance::CargoManaged {
                    bin_root: root.join("bin"),
                    version,
                };
            }
            CargoRecord::DoesNotClaim { .. } | CargoRecord::Absent => continue,
            CargoRecord::Uninterpretable { detail } => {
                return Provenance::UnprovableOwnership {
                    detail: format!(
                        "{} sits under the Cargo root {} and {detail}",
                        current_exe.display(),
                        root.display()
                    ),
                };
            }
        }
    }

    match eggup_core::hash_file(current_exe) {
        Ok(digest) => Provenance::VerifiableSelfManaged { digest },
        Err(error) => Provenance::UnprovableOwnership {
            detail: format!(
                "the running executable {} could not be hashed: {error}",
                current_exe.display()
            ),
        },
    }
}

/// How many directories above the running executable are considered as
/// possible Cargo installation roots.
///
/// Cargo lays an installation out as `ROOT/bin/<exe>` with `ROOT/.crates.toml`,
/// so the owning root is the executable's grandparent. Container images and
/// hermetic test harnesses nest that a level or two deeper. The bound exists to
/// stop a deep path from turning classification into a filesystem crawl.
///
/// Under-detection is the dangerous direction here: a Cargo-owned file that is
/// misread as self-managed gets *replaced*, while a false positive only costs a
/// refusal that names the manager command. So this is deliberately generous.
///
/// The bound cannot miss a supported layout, and that is now checkable rather
/// than asserted: `a_cargo_root_beyond_the_bound_cannot_exist` walks the
/// real-shape layouts from C018's Work Package A and proves the owning root is
/// always within it. What changed in C018 is not the bound but the *marker* --
/// a candidate root now has to carry positive Cargo metadata to be considered,
/// so "an ancestor called `bin`" is no longer treated as a Cargo root.
const CARGO_ROOT_SEARCH_DEPTH: usize = 4;

/// The file name of the running executable, as Cargo's record would spell it.
///
/// A non-UTF-8 file name becomes the empty string, which is a deliberate
/// downgrade rather than an accident: a `.crates.toml` value can only ever hold
/// UTF-8, so such a name can never match a recorded executable. Returning `""`
/// means "the record cannot name this file", and every caller treats a
/// non-match in a Cargo root as unprovable rather than as absence. The previous
/// behaviour -- `unwrap_or("")` on a `&str` that then silently participated in
/// matching -- could match nothing and be read as "Cargo records nothing here",
/// which is the fail-open direction.
fn binary_name(path: &Path) -> &str {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

/// Cargo installation roots that could own `path`, nearest first.
///
/// Cargo writes `ROOT/bin/<exe>` plus `ROOT/.crates.toml`, so each candidate is
/// the parent of some ancestor `bin` directory. This is a *location* test only;
/// it never grants ownership, and the caller still has to read the root's record.
fn cargo_roots_above(path: &Path) -> Vec<PathBuf> {
    let Some(bin_dir) = path.parent() else {
        return Vec::new();
    };
    bin_dir
        .ancestors()
        .take(CARGO_ROOT_SEARCH_DEPTH + 1)
        .filter_map(Path::parent)
        .map(Path::to_path_buf)
        .collect()
}

/// What a Cargo installation root's own record says about one binary.
///
/// C018 exists because this was an `Option<String>`, and `None` meant four
/// different things at once: "there is no record", "the record is unreadable",
/// "the record is malformed", and "the record is fine and does not mention this
/// package". The caller could not tell "Cargo says nothing here" from "I could
/// not read what Cargo would have said", so a record it failed to parse fell
/// through to hashing the executable and was classified `VerifiableSelfManaged`.
/// That is the fail-open direction: the file gets *replaced*, and Cargo's
/// `install --list` and uninstall bookkeeping are left lying.
///
/// Collapsing uncertainty into absence is the defect. These four states stay
/// distinct, and the caller decides what each one means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoRecord {
    /// Cargo positively records `package` AND `binary` in this root.
    Recorded { version: String },
    /// The record is readable and parseable, and provably does not claim this
    /// package or binary. This is *not* uncertainty -- it is a positive
    /// statement that the root is not responsible for this file.
    DoesNotClaim { reason: &'static str },
    /// No record file at this root. The root shows no evidence of being a Cargo
    /// installation root, so the search continues upward.
    Absent,
    /// A record exists but could not be interpreted: unreadable, not valid
    /// TOML, or in a shape this parser does not support.
    ///
    /// This is the state that must never be read as absence. The presence of
    /// the file is positive evidence that this directory is a Cargo
    /// installation root, and the content that would settle ownership is
    /// exactly what could not be read.
    Uninterpretable { detail: String },
}

impl CargoRecord {
    /// Whether this state leaves ownership genuinely unknown, as opposed to
    /// positively established as "not Cargo's".
    pub fn is_uncertain(&self) -> bool {
        matches!(self, CargoRecord::Uninterpretable { .. })
    }
}

/// Read a Cargo installation root's `.crates.toml` and report what it says.
///
/// Bounded on every axis: the file is size-capped before it is read, the TOML is
/// parsed as a document rather than scanned, and `root` is the only directory
/// consulted -- no parent, no `PATH`, no other manager's metadata. An
/// unparseable entry is skipped only when the parser can *prove* it cannot be
/// the cargo-cleanme record; a malformed entry that still names this package is
/// `Uninterpretable`, not absence.
fn read_cargo_record(root: &Path, package: &str, binary: &str) -> CargoRecord {
    let path = root.join(".crates.toml");

    let metadata = match std::fs::metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return CargoRecord::Absent,
        Err(error) => {
            return CargoRecord::Uninterpretable {
                detail: format!(
                    "the Cargo record {} exists but could not be inspected: {error}",
                    path.display()
                ),
            };
        }
        Ok(metadata) => metadata,
    };

    // A record that is not a regular file, or that is implausibly large, is
    // still positive evidence that this root is manager-sensitive -- the file
    // is *there*. Only its absence would justify searching further up.
    if !metadata.is_file() {
        return CargoRecord::Uninterpretable {
            detail: format!("the Cargo record {} is not a regular file", path.display()),
        };
    }
    if metadata.len() > MAX_METADATA_BYTES as u64 {
        return CargoRecord::Uninterpretable {
            detail: format!(
                "the Cargo record {} is {} bytes, above the {MAX_METADATA_BYTES} byte ceiling",
                path.display(),
                metadata.len()
            ),
        };
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return CargoRecord::Uninterpretable {
                detail: format!(
                    "the Cargo record {} could not be read: {error}",
                    path.display()
                ),
            };
        }
    };

    let value: toml::Value = match toml::from_str(&text) {
        Ok(value) => value,
        Err(error) => {
            return CargoRecord::Uninterpretable {
                detail: format!(
                    "the Cargo record {} is not valid TOML: {error}",
                    path.display()
                ),
            };
        }
    };

    let Some(table) = value.get("v1").and_then(toml::Value::as_table) else {
        // A `.crates.toml` with no `v1` table is a shape this parser does not
        // support. Cargo has never written that for an install root, so reading
        // it as "Cargo owns nothing here" would be trusting an assumption
        // instead of the record.
        return CargoRecord::Uninterpretable {
            detail: format!(
                "the Cargo record {} has no `v1` table, which is not a supported Cargo record shape",
                path.display()
            ),
        };
    };

    let mut saw_malformed_matching_package = false;
    for (spec, executables) in table {
        // A spec string is "<name> <version> (<source>)". The source may itself
        // contain spaces, so the parenthesised tail is removed from the right
        // before the leading two fields are split off.
        let Some((head, _source)) = spec.rsplit_once(')') else {
            continue;
        };
        let mut fields = head.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        if name != package {
            // Provably not ours, so its shape is irrelevant. This is what keeps
            // an unrelated broken entry from hiding a later valid record.
            continue;
        }
        let Some(version) = fields.next() else {
            // Names this package but cannot be read. It might still be the
            // record that claims this binary, so it is uncertainty.
            saw_malformed_matching_package = true;
            continue;
        };
        let Some(names) = executables.as_array() else {
            saw_malformed_matching_package = true;
            continue;
        };
        if names.iter().any(|name| name.as_str() == Some(binary)) {
            return CargoRecord::Recorded {
                version: version.to_owned(),
            };
        }
    }

    if saw_malformed_matching_package {
        return CargoRecord::Uninterpretable {
            detail: format!(
                "the Cargo record {} holds an entry for {package} whose version or executable \
                 list could not be interpreted",
                path.display()
            ),
        };
    }
    CargoRecord::DoesNotClaim {
        reason: "the Cargo record for this root does not name this package",
    }
}

// --------------------------------------------------------------- version check

/// Extract the stable version crates.io reports for this package.
///
/// Deliberately narrow: it reads one bounded field and rejects anything that is
/// not a plain `X.Y.Z` release. A prerelease or a yanked version is not a
/// self-update target, because a self-update cannot express "I accept that
/// tradeoff" the way an explicit `cargo install` can.
pub fn published_stable_version(metadata: &[u8]) -> Result<String, UpdateError> {
    let text =
        std::str::from_utf8(metadata).map_err(|e| UpdateError::NoPublishedStableVersion {
            detail: format!("registry metadata is not UTF-8: {e}"),
        })?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| UpdateError::NoPublishedStableVersion {
            detail: format!("registry metadata is not valid JSON: {e}"),
        })?;
    let version = value
        .get("crate")
        .and_then(|c| c.get("max_stable_version"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| UpdateError::NoPublishedStableVersion {
            detail: "registry metadata has no crate.max_stable_version".to_owned(),
        })?;
    if !is_release_version(version) {
        return Err(UpdateError::NoPublishedStableVersion {
            detail: format!("{version:?} is not a plain X.Y.Z release"),
        });
    }
    Ok(version.to_owned())
}

/// Compare two plain `X.Y.Z` versions.
///
/// `None` when either side is not a plain release, so a caller can never
/// accidentally order a prerelease or a malformed string.
pub fn compare_release_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let parse = |value: &str| -> Option<[u64; 3]> {
        let mut parts = value.split('.');
        let mut out = [0u64; 3];
        for slot in out.iter_mut() {
            *slot = parts.next()?.parse::<u64>().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        Some(out)
    };
    Some(parse(left)?.cmp(&parse(right)?))
}

/// A plain `X.Y.Z` release with no prerelease or build suffix.
pub fn is_release_version(version: &str) -> bool {
    let mut parts = version.split('.');
    for _ in 0..3 {
        match parts.next() {
            Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {}
            _ => return false,
        }
    }
    parts.next().is_none()
}

// -------------------------------------------------------------------- the run

/// Everything the run needs from the outside world, so tests can drive it with
/// a deterministic fixture transport instead of the network.
pub trait UpdateEnvironment {
    /// The absolute path of the running executable.
    fn current_exe(&self) -> Result<PathBuf, UpdateError>;
    /// The Cargo home this run should consult for installation provenance.
    ///
    /// Part of the seam rather than a direct environment read so the refusal
    /// path for a Cargo-managed install is reachable from a test without
    /// mutating process-global state.
    fn cargo_home(&self) -> Option<PathBuf>;
    /// The version of the running executable.
    fn current_version(&self) -> String;
    /// Create a private staging directory for acquired bytes.
    fn staging_dir(&self) -> Result<PathBuf, UpdateError>;
    /// Fetch a URL into `destination`, returning bytes written.
    fn fetch(&self, url: &str, destination: &Path) -> Result<u64, UpdateError>;
    /// Fetch a URL into memory, bounded by `limit` bytes.
    fn fetch_metadata(&self, url: &str, limit: usize) -> Result<Vec<u8>, UpdateError>;
    /// Remove only this run's own staging directory.
    fn cleanup(&self, staging: &Path);
}

/// The User-Agent every outbound request identifies itself with.
///
/// The registry rejects anonymous or generic clients outright: an unspecified
/// agent and a bare `curl/x.y` both return HTTP 403, while a descriptive agent
/// returns 200. This is a hard requirement of the version authority, not
/// politeness, and it is the reason the production transport is
/// `eggup-eggfetch` rather than `eggup-curl`.
const USER_AGENT: &str = concat!(
    "cargo-cleanme/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/dbowm91/cargo-cleanme)"
);

/// The real environment: the published transport plus the process's own
/// filesystem view.
struct HttpEnvironment {
    transport: eggup_eggfetch::EggfetchTransport,
}

impl HttpEnvironment {
    fn new() -> Result<Self, UpdateError> {
        // Strict: bounded deadlines, bounded redirects, explicit proxy decision,
        // and an identifying User-Agent. Request deadlines can only tighten the
        // adapter's ceilings (see FetchLimits::effective).
        let config = eggup_eggfetch::EggfetchConfig::strict()
            .user_agent(USER_AGENT)
            .timeouts(CONNECT_TIMEOUT, TOTAL_TIMEOUT);
        let transport = eggup_eggfetch::EggfetchTransport::strict(config).map_err(|error| {
            UpdateError::TransportUnavailable {
                detail: describe_acquisition(&error),
            }
        })?;
        Ok(Self { transport })
    }

    fn limits(&self) -> FetchLimits {
        FetchLimits {
            max_metadata_bytes: MAX_METADATA_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES,
            connect_timeout: CONNECT_TIMEOUT,
            total_timeout: TOTAL_TIMEOUT,
        }
    }
}

impl UpdateEnvironment for HttpEnvironment {
    fn current_exe(&self) -> Result<PathBuf, UpdateError> {
        std::env::current_exe().map_err(|error| UpdateError::Transaction {
            detail: format!("could not resolve the running executable: {error}"),
        })
    }

    fn cargo_home(&self) -> Option<PathBuf> {
        default_cargo_home()
    }

    fn current_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    fn staging_dir(&self) -> Result<PathBuf, UpdateError> {
        // A collision-resistant, invocation-owned directory name. The
        // timestamp plus pid is enough: this never overwrites an existing path,
        // it fails instead.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let unique =
            std::env::temp_dir().join(format!("{PRODUCT}-update-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&unique).map_err(|error| UpdateError::Transaction {
            detail: format!("could not create a private staging directory: {error}"),
        })?;
        Ok(unique)
    }

    fn fetch(&self, url: &str, destination: &Path) -> Result<u64, UpdateError> {
        let request = AcquisitionRequest::new(url).map_err(|error| UpdateError::Acquisition {
            stage: "build the release request".to_owned(),
            detail: describe_acquisition(&error),
        })?;
        let cancel = CancelFlag::new();
        let limits = self.limits();
        let outcome = self
            .transport
            .fetch_artifact(&request, destination, limits, &cancel)
            .map_err(|error| UpdateError::Acquisition {
                stage: format!("download {url}"),
                detail: describe_acquisition(&error),
            })?;
        match outcome {
            FetchOutcome::Success(evidence) => Ok(evidence.bytes_written()),
            FetchOutcome::NotFound => Err(UpdateError::ReleaseEvidence {
                detail: format!("the release host reports {url} is absent"),
            }),
        }
    }

    fn fetch_metadata(&self, url: &str, limit: usize) -> Result<Vec<u8>, UpdateError> {
        let request = AcquisitionRequest::new(url).map_err(|error| UpdateError::Acquisition {
            stage: "build the registry request".to_owned(),
            detail: describe_acquisition(&error),
        })?;
        let mut bounds = self.limits();
        bounds.max_metadata_bytes = limit;
        let cancel = CancelFlag::new();
        let outcome = self
            .transport
            .fetch_metadata(&request, bounds, &cancel)
            .map_err(|error| UpdateError::Acquisition {
                stage: format!("read {url}"),
                detail: describe_acquisition(&error),
            })?;
        match outcome {
            FetchOutcome::Success(body) => Ok(body.bytes().to_vec()),
            FetchOutcome::NotFound => Err(UpdateError::NoPublishedStableVersion {
                detail: format!("the registry reports {url} is absent"),
            }),
        }
    }

    fn cleanup(&self, staging: &Path) {
        // Only this run's own directory, created by `staging_dir` above.
        let _ = std::fs::remove_dir_all(staging);
    }
}

fn describe_acquisition(error: &AcquisitionError) -> String {
    format!("{error:?}")
}

/// Plan and execute one update.
///
/// `check_only` resolves the plan and reports it without acquiring or mutating
/// anything, which is what `--dry-run` uses.
pub fn run(
    environment: &dyn UpdateEnvironment,
    check_only: bool,
) -> Result<UpdatePlan, UpdateError> {
    let current_exe = environment.current_exe()?;
    let current_version = environment.current_version();
    let provenance = classify_provenance_in(&current_exe, environment.cargo_home());

    let target = match host_target() {
        Some(target) => target,
        None => {
            return Err(UpdateError::Provenance { provenance });
        }
    };
    let asset = asset_for_target(target).ok_or_else(|| UpdateError::HostUnsupported {
        detail: format!("{target} has no contracted release asset"),
    })?;

    // A mutating update refuses forbidden provenance *before* asking the
    // registry anything (C018).
    //
    // The ordering is the fix. Refusing only after fetching and comparing
    // registry metadata meant that a Cargo-managed or unprovable installation
    // still performed a full crates.io round trip and was told the outcome was
    // "up to date" or "a newer version exists" on the way to being refused --
    // so the one case where this tool must be certain it is not allowed to
    // touch the file was also the case that learned the most about the network
    // and the least about the local state. When local provenance alone makes
    // mutation impossible, there is nothing the registry can tell us that
    // changes the answer.
    //
    // `--dry-run` deliberately still looks up the version authority: reporting a
    // candidate is the entire point of a dry run, and the classification it
    // reports alongside is the honest description of why a real run would stop.
    if !check_only && !matches!(provenance, Provenance::VerifiableSelfManaged { .. }) {
        return Err(UpdateError::Provenance { provenance });
    }

    let metadata = environment.fetch_metadata(&crates_io_metadata_url(), MAX_METADATA_BYTES)?;
    let to_version = published_stable_version(&metadata)?;

    match compare_release_versions(&to_version, &current_version) {
        Some(std::cmp::Ordering::Equal) => {
            return Err(UpdateError::AlreadyCurrent {
                current: current_version,
                published: to_version,
            });
        }
        Some(std::cmp::Ordering::Less) => {
            // Never downgrade. A build that is newer than the published stable
            // release is a local or unreleased build, and the registry is not
            // entitled to replace it.
            return Err(UpdateError::NewerThanPublished {
                current: current_version,
                published: to_version,
            });
        }
        Some(std::cmp::Ordering::Greater) => {}
        // An unorderable pair means one side is not a plain release, which
        // `published_stable_version` already rejects; refuse rather than guess.
        None => {
            return Err(UpdateError::NoPublishedStableVersion {
                detail: format!(
                    "cannot order published {to_version:?} against running {current_version:?}"
                ),
            });
        }
    }

    let plan = UpdatePlan {
        from_version: current_version,
        to_version: to_version.clone(),
        target: target.to_owned(),
        asset: asset.to_owned(),
        // The tag is constructed, never scraped from a page or a redirect.
        tag: format!("v{to_version}"),
        provenance,
    };

    if check_only {
        return Ok(plan);
    }

    // Refuse before acquiring anything if this installation is not ours to
    // replace. Checking after the download would waste the user's bandwidth on
    // a run that could not be committed.
    if let Provenance::VerifiableSelfManaged { .. } = plan.provenance {
    } else {
        return Err(UpdateError::Provenance {
            provenance: plan.provenance.clone(),
        });
    }

    execute(environment, &plan, &current_exe)
}

fn execute(
    environment: &dyn UpdateEnvironment,
    plan: &UpdatePlan,
    current_exe: &Path,
) -> Result<UpdatePlan, UpdateError> {
    let live_digest = match &plan.provenance {
        Provenance::VerifiableSelfManaged { digest } => *digest,
        other => {
            return Err(UpdateError::Provenance {
                provenance: other.clone(),
            });
        }
    };

    let staging = environment.staging_dir()?;
    let outcome = (|| -> Result<TransactionReceipt, UpdateError> {
        let sidecar_path = staging.join(format!("{}.sha256", plan.asset));
        environment.fetch(
            &release_asset_url(&plan.tag, &format!("{}.sha256", plan.asset)),
            &sidecar_path,
        )?;
        let sidecar_text = std::fs::read_to_string(&sidecar_path).map_err(|error| {
            UpdateError::ReleaseEvidence {
                detail: format!("the release checksum sidecar is unreadable: {error}"),
            }
        })?;
        let manifest: Sha256Manifest =
            eggup_core::parse_sha256_sidecar(&sidecar_text).map_err(|error| {
                UpdateError::ReleaseEvidence {
                    detail: format!("the release checksum sidecar is malformed: {error}"),
                }
            })?;

        let acquired = staging.join(&plan.asset);
        let written = environment.fetch(&release_asset_url(&plan.tag, &plan.asset), &acquired)?;
        if written == 0 {
            return Err(UpdateError::ReleaseEvidence {
                detail: format!("the release asset {} is empty", plan.asset),
            });
        }

        // The sidecar must name the asset it is evidence for, so a sidecar
        // copied from another release cannot silently authorize these bytes.
        if let Some(name) = manifest.filename()
            && name != plan.asset
        {
            return Err(UpdateError::ReleaseEvidence {
                detail: format!(
                    "the sidecar names {name:?}, not the requested asset {:?}",
                    plan.asset
                ),
            });
        }

        let install_root = current_exe
            .parent()
            .ok_or_else(|| UpdateError::Transaction {
                detail: "the running executable has no parent directory".to_owned(),
            })?
            .to_path_buf();

        let member_id =
            MemberId::new("cargo-cleanme").map_err(|error| UpdateError::Transaction {
                detail: format!("could not name the update member: {error}"),
            })?;
        let file_name = current_exe
            .file_name()
            .ok_or_else(|| UpdateError::Transaction {
                detail: "the running executable has no file name".to_owned(),
            })?
            .to_owned();

        let member = ArtifactMember::new(member_id.clone(), acquired, file_name)
            .map_err(|error| UpdateError::Transaction {
                detail: format!("could not declare the update member: {error}"),
            })?
            // The candidate must be executable to prove its own identity, and
            // must stay executable once committed. `Preserve` would carry a
            // freshly downloaded 0644 artifact through as non-executable, so
            // bounded candidate execution would fail with EACCES on every
            // real update. Eggup applies this intent at staging, before both
            // the identity check and the commit, and maps it to an
            // owner-private 0700.
            .with_permissions(PermissionsIntent::Executable)
            .with_integrity(IntegrityRequirement::Sha256(*manifest.digest()));
        let artifacts = ArtifactSet::single(member).map_err(|error| UpdateError::Transaction {
            detail: format!("could not assemble the update set: {error}"),
        })?;

        let install_plan = InstallPlan::new(
            ProductId::new(PRODUCT).map_err(|error| UpdateError::Transaction {
                detail: format!("invalid product identity: {error}"),
            })?,
            ReleaseId::new(plan.to_version.clone()).map_err(|error| UpdateError::Transaction {
                detail: format!("invalid release identity: {error}"),
            })?,
            install_root,
            artifacts,
        )
        .map_err(|error| UpdateError::Transaction {
            detail: format!("the install plan is invalid: {error}"),
        })?;

        let prepared: PreparedTransaction =
            install_plan
                .prepare()
                .map_err(|error| UpdateError::Transaction {
                    detail: format!("could not stage the update: {error}"),
                })?;
        let verified = prepared
            .verify_integrity()
            .map_err(|error| UpdateError::Transaction {
                detail: format!("release integrity verification failed: {error}"),
            })?;

        // The candidate must identify itself as exactly the version the
        // registry named. This runs the acquired bytes, so it is the only
        // check that the thing about to replace this process is really us.
        //
        // `--version` is not optional. With no arguments the binary runs its
        // default routine scan, which prints a scan report to stdout and
        // filesystem diagnostics to stderr -- so an argv-less check compares
        // stdout against a version string that can never match, and it scans
        // the whole machine on the way to failing. This is C016: the live
        // commit path had never succeeded, and the fixture could not see it
        // because its candidate stub ignored argv and printed the identity
        // for any invocation.
        let validator =
            ExactIdentityValidator::new(member_id.clone(), expected_identity(&plan.to_version))
                .args(["--version"]);
        let validated =
            verified
                .validate(&validator)
                .map_err(|error| UpdateError::Transaction {
                    detail: format!(
                        "the candidate did not identify itself as {} {}\n{error}",
                        PRODUCT, plan.to_version
                    ),
                })?;

        let mut expected = HashMap::new();
        expected.insert(member_id, live_digest);
        let verifier = LiveDigestVerifier { expected };

        validated
            .commit(CommitOwnership::new(&verifier, AbsentPolicy::DenyCreate))
            .map_err(|error| UpdateError::Transaction {
                detail: format!("commit failed: {error}"),
            })
    })();

    environment.cleanup(&staging);

    let receipt = outcome?;
    let disposition = match receipt.disposition() {
        TransactionDisposition::Committed => return Ok(plan.clone()),
        TransactionDisposition::RolledBack => "rolled_back",
        TransactionDisposition::RecoveryRequired => "recovery_required",
    };
    let detail = receipt
        .failure()
        .map(|f| f.detail().to_owned())
        .unwrap_or_else(|| "no failure detail was recorded".to_owned());
    Err(UpdateError::Receipt {
        disposition,
        detail,
    })
}

/// Convenience entry point for the CLI.
pub fn update(check_only: bool) -> Result<UpdatePlan, UpdateError> {
    let environment = HttpEnvironment::new()?;
    run(&environment, check_only)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap as Map;

    // ------------------------------------------------------------- environment

    /// A deterministic environment: no network, no curl discovery, no process
    /// globals. Every test drives the same code path a real run does, with the
    /// network and the filesystem replaced by fixtures.
    struct FixtureEnvironment {
        exe: PathBuf,
        cargo_home: Option<PathBuf>,
        version: String,
        staging: RefCell<Option<PathBuf>>,
        /// Every staging path this fixture handed to production, in order.
        ///
        /// This is the fixture's ownership record, and it is what the leak
        /// check is built on. It is deliberately *not* a scan of the temp
        /// directory: the historical detector asked "does any
        /// `cargo-cleanme-update-test-` path exist anywhere", and because every
        /// test in this module mints staging names from the same prefix, a
        /// parallel test's live directory answered that question for us. The
        /// subject could not fail for its own reason and could not pass for its
        /// own reason.
        created_staging: RefCell<Vec<PathBuf>>,
        /// How many times production reached the cleanup callback.
        ///
        /// Tracked separately from the filesystem state because the two are
        /// different claims. "The callback ran" is satisfied by setting
        /// `staging` to `None`; "the transaction left no bytes behind" is only
        /// satisfied by the path being absent. A test that asserted the first
        /// and called it cleanup evidence was asserting a memory write.
        cleanup_calls: RefCell<usize>,
        /// Leave this fixture's own staging directory on disk, so the leak
        /// check can be required to reject a genuinely leaked transaction.
        suppress_cleanup: bool,
        responses: Map<String, Vec<u8>>,
        /// URLs that must fail with a transport error rather than 404.
        transport_failures: Vec<String>,
        /// URLs reported as absent.
        absent: Vec<String>,
        requested: RefCell<Vec<String>>,
    }

    impl FixtureEnvironment {
        fn new(exe: PathBuf, version: &str) -> Self {
            Self {
                exe,
                cargo_home: None,
                version: version.to_owned(),
                staging: RefCell::new(None),
                created_staging: RefCell::new(Vec::new()),
                cleanup_calls: RefCell::new(0),
                suppress_cleanup: false,
                responses: Map::new(),
                transport_failures: Vec::new(),
                absent: Vec::new(),
                requested: RefCell::new(Vec::new()),
            }
        }

        fn respond(mut self, url: &str, bytes: Vec<u8>) -> Self {
            self.responses.insert(url.to_owned(), bytes);
            self
        }

        fn cargo_home(mut self, home: PathBuf) -> Self {
            self.cargo_home = Some(home);
            self
        }

        fn fail_transport(mut self, url: &str) -> Self {
            self.transport_failures.push(url.to_owned());
            self
        }

        /// Reach the cleanup callback but leave the staging directory behind.
        ///
        /// The negative control for `leaked_staging`: without a fixture that
        /// can produce a real leftover, "no leftovers" is an assertion that
        /// cannot fail, and this repository does not close tests on those.
        fn leaking_staging(mut self) -> Self {
            self.suppress_cleanup = true;
            self
        }

        /// How many times production reached `cleanup` for this fixture.
        fn cleanup_calls(&self) -> usize {
            *self.cleanup_calls.borrow()
        }

        /// Staging paths **this fixture created** that still exist on disk.
        ///
        /// Scoped to the fixture's own recorded paths, so a concurrent test's
        /// live staging directory is invisible here — which is the property
        /// `a_foreign_staging_directory_is_not_evidence_of_a_leak` pins, and
        /// the property the historical process-wide scan did not have.
        fn leaked_staging(&self) -> Vec<PathBuf> {
            self.created_staging
                .borrow()
                .iter()
                .filter(|path| path.exists())
                .cloned()
                .collect()
        }

        /// Assert this fixture's own staging transaction left nothing behind.
        ///
        /// Two independent claims, deliberately not collapsed into one:
        /// production reached the cleanup callback, *and* the directory it
        /// owned no longer exists on disk.
        ///
        /// Only the `#[cfg(unix)]` cases call this, because the success path
        /// executes the candidate as a real script. On Windows it is
        /// therefore unreachable, which is the same reason `absent` carries
        /// the attribute below — and the first cut of this method omitted it,
        /// so `clippy -D warnings` failed the Windows lane while Linux was
        /// green. The unused method was the evidence of a platform gap, not a
        /// style problem.
        #[cfg_attr(not(unix), allow(dead_code))]
        fn assert_no_staging_leak(&self, what: &str) {
            assert_eq!(
                self.created_staging.borrow().len(),
                1,
                "{what}: expected exactly one staging transaction, saw {:?}",
                self.created_staging.borrow()
            );
            assert!(
                self.cleanup_calls() > 0,
                "{what}: production never reached the staging cleanup callback"
            );
            assert!(
                self.leaked_staging().is_empty(),
                "{what}: staging left behind: {:?}",
                self.leaked_staging()
            );
        }

        #[cfg_attr(not(unix), allow(dead_code))]
        fn absent(mut self, url: &str) -> Self {
            self.absent.push(url.to_owned());
            self
        }

        fn registry(&self, version: &str) -> String {
            format!(r#"{{"crate":{{"max_stable_version":"{version}"}}}}"#)
        }

        fn asset_url(tag: &str, asset: &str) -> String {
            release_asset_url(tag, asset)
        }
    }

    impl UpdateEnvironment for FixtureEnvironment {
        fn current_exe(&self) -> Result<PathBuf, UpdateError> {
            Ok(self.exe.clone())
        }
        fn cargo_home(&self) -> Option<PathBuf> {
            self.cargo_home.clone()
        }
        fn current_version(&self) -> String {
            self.version.clone()
        }
        fn staging_dir(&self) -> Result<PathBuf, UpdateError> {
            let path = unique("cargo-cleanme-update-test");
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).map_err(|e| UpdateError::Transaction {
                detail: e.to_string(),
            })?;
            *self.staging.borrow_mut() = Some(path.clone());
            self.created_staging.borrow_mut().push(path.clone());
            Ok(path)
        }
        fn fetch(&self, url: &str, destination: &Path) -> Result<u64, UpdateError> {
            self.requested.borrow_mut().push(url.to_owned());
            if self.transport_failures.iter().any(|u| u == url) {
                return Err(UpdateError::Acquisition {
                    stage: format!("download {url}"),
                    detail: "fixture transport failure".to_owned(),
                });
            }
            if self.absent.iter().any(|u| u == url) {
                return Err(UpdateError::ReleaseEvidence {
                    detail: format!("the release host reports {url} is absent"),
                });
            }
            let bytes = self
                .responses
                .get(url)
                .ok_or_else(|| UpdateError::ReleaseEvidence {
                    detail: format!("fixture has no response for {url}"),
                })?;
            std::fs::write(destination, bytes).map_err(|e| UpdateError::Acquisition {
                stage: format!("stage {url}"),
                detail: e.to_string(),
            })?;
            Ok(bytes.len() as u64)
        }
        fn fetch_metadata(&self, url: &str, _limit: usize) -> Result<Vec<u8>, UpdateError> {
            self.requested.borrow_mut().push(url.to_owned());
            if self.transport_failures.iter().any(|u| u == url) {
                return Err(UpdateError::Acquisition {
                    stage: format!("read {url}"),
                    detail: "fixture transport failure".to_owned(),
                });
            }
            self.responses
                .get(url)
                .cloned()
                .ok_or_else(|| UpdateError::NoPublishedStableVersion {
                    detail: format!("fixture has no metadata for {url}"),
                })
        }
        fn cleanup(&self, staging: &Path) {
            *self.cleanup_calls.borrow_mut() += 1;
            if self.suppress_cleanup {
                // Deliberately leave the directory in place so
                // `leaked_staging` has something real to report.
                return;
            }
            let _ = std::fs::remove_dir_all(staging);
            *self.staging.borrow_mut() = None;
        }
    }

    // -------------------------------------------------------------- fixtures

    /// A per-call unique name. Tests run in parallel, so a name derived only
    /// from the pid races with itself across cases that reuse a version.
    fn unique(prefix: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{nanos}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ))
    }

    /// A candidate executable that prints the identity a real release would.
    ///
    /// It answers **only** for `--version`, and refuses anything else on
    /// stderr, exactly as the real binary does: `cargo-cleanme` with no
    /// arguments performs a routine scan and writes a report to stdout.
    ///
    /// The earlier version of this stub printed the identity unconditionally,
    /// ignoring argv. That made it blind to C016: production invoked the
    /// candidate with no arguments, the real binary produced a scan report
    /// instead of an identity, and every commit-path test stayed green because
    /// the stub could not distinguish the two invocations. A fixture that
    /// accepts any input cannot detect an input that was never supplied.
    fn candidate_bytes(version: &str) -> Vec<u8> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = unique(&format!("cargo-cleanme-candidate-{version}"));
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\n\
                     if [ \"$1\" = \"--version\" ]; then\n\
                     \x20 printf '{PRODUCT} {version}\\n'\n\
                     \x20 exit 0\n\
                     fi\n\
                     printf 'fixture candidate: unexpected argv\\n' >&2\n\
                     exit 2\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let _ = std::fs::remove_file(&path);
            bytes
        }
        #[cfg(not(unix))]
        {
            let _ = version;
            b"MZ fixture candidate".to_vec()
        }
    }

    /// SHA-256 hex over bytes, computed by Eggup's own file hashing so the
    /// test and production agree by construction rather than through a second
    /// digest implementation.
    fn sha2_digest(bytes: &[u8]) -> String {
        let temp = unique("cc-digest");
        std::fs::write(&temp, bytes).unwrap();
        let digest = eggup_core::hash_file(&temp).unwrap();
        let _ = std::fs::remove_file(&temp);
        let mut out = String::with_capacity(64);
        for byte in digest {
            out.push_str(&format!("{byte:02x}"));
        }
        out
    }

    struct Fixture {
        // Held so the whole tree outlives the test body.
        _dir: tempfile::TempDir,
        live: PathBuf,
    }

    fn fixture(live_bytes: &[u8]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let install_dir = dir.path().join("bin");
        std::fs::create_dir_all(&install_dir).unwrap();
        let live = install_dir.join(if cfg!(windows) {
            "cargo-cleanme.exe"
        } else {
            "cargo-cleanme"
        });
        std::fs::write(&live, live_bytes).unwrap();
        // `cfg!` is a runtime check, so the platform module must be gated with
        // `#[cfg]` or the Windows target fails to compile this test module.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&live, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Fixture { _dir: dir, live }
    }

    fn live_stub() -> Vec<u8> {
        if cfg!(unix) {
            b"#!/bin/sh\necho 'cargo-cleanme 0.0.1'\n".to_vec()
        } else {
            b"MZ live".to_vec()
        }
    }

    /// Build a complete, consistent release response set.
    fn release(
        environment: FixtureEnvironment,
        to: &str,
        candidate: Vec<u8>,
        sidecar_for: Option<&str>,
    ) -> FixtureEnvironment {
        let tag = format!("v{to}");
        let asset = asset_for_target(host_target().unwrap()).unwrap();
        let digest = sha2_digest(&candidate);
        let sidecar_name = sidecar_for.unwrap_or(asset);
        let registry = environment.registry(to).into_bytes();
        environment
            .respond(&crates_io_metadata_url(), registry)
            .respond(
                &FixtureEnvironment::asset_url(&tag, &format!("{asset}.sha256")),
                format!("{digest}  {sidecar_name}\n").into_bytes(),
            )
            .respond(
                &FixtureEnvironment::asset_url(&tag, asset),
                candidate.clone(),
            )
    }

    // ------------------------------------------------------- version authority

    #[test]
    fn version_authority_accepts_only_plain_releases() {
        assert!(is_release_version("0.1.0"));
        assert!(is_release_version("10.20.30"));
        for rejected in [
            "1.2",
            "1.2.3.4",
            "1.2.3-rc.1",
            "1.2.3+build",
            "v1.2.3",
            "1.2.x",
            "",
        ] {
            assert!(
                !is_release_version(rejected),
                "{rejected} should be rejected"
            );
        }
    }

    #[test]
    fn version_authority_reads_exactly_one_field() {
        let good = published_stable_version(br#"{"crate":{"max_stable_version":"1.4.2"}}"#)
            .expect("plain release accepted");
        assert_eq!(good, "1.4.2");
    }

    #[test]
    fn version_authority_rejects_prerelease_and_junk() {
        for junk in [
            &br#"{"crate":{"max_stable_version":"1.4.2-rc.1"}}"#[..],
            &br#"{"crate":{"newest_version":"1.4.2"}}"#[..],
            &br#"{"crate":{}}"#[..],
            &b"not json"[..],
            &b""[..],
        ] {
            assert!(
                published_stable_version(junk).is_err(),
                "{:?} should be rejected",
                String::from_utf8_lossy(junk)
            );
        }
    }

    #[test]
    fn the_release_tag_is_constructed_not_scraped() {
        // A hostile or redirected metadata document cannot influence the tag.
        let metadata = br#"{"crate":{"max_stable_version":"2.0.0"},"evil":{"tag":"../../etc"}}"#;
        let plan = FixtureEnvironment::new(PathBuf::from("/nonexistent"), "0.1.0")
            .respond(&crates_io_metadata_url(), metadata.to_vec());
        let environment = release(plan, "2.0.0", candidate_bytes("2.0.0"), None);
        let plan = run(&environment, true).expect("dry run resolves");
        assert_eq!(plan.tag, "v2.0.0");
        assert_eq!(plan.to_version, "2.0.0");
    }

    #[test]
    fn already_current_is_not_an_error_to_hide() {
        let base = FixtureEnvironment::new(PathBuf::from("/nonexistent"), "1.0.0");
        let environment = release(base, "1.0.0", candidate_bytes("1.0.0"), None);
        match run(&environment, true) {
            Err(UpdateError::AlreadyCurrent { current, published }) => {
                assert_eq!(current, "1.0.0");
                assert_eq!(published, "1.0.0");
            }
            other => panic!("expected AlreadyCurrent, got {other:?}"),
        }
    }

    #[test]
    fn a_registry_outage_is_reported_not_guessed() {
        let base = FixtureEnvironment::new(PathBuf::from("/nonexistent"), "0.1.0")
            .fail_transport(&crates_io_metadata_url());
        let error = run(&base, true).expect_err("transport failure must not resolve");
        assert!(
            matches!(error, UpdateError::Acquisition { .. }),
            "{error:?}"
        );
    }

    // ------------------------------------------------------------- provenance

    /// A `.crates.toml` in the schema cargo actually writes.
    ///
    /// Captured from cargo 1.99.0 after
    /// `cargo install cargo-cleanme --version 0.1.3 --locked --root DIR`:
    ///
    /// ```toml
    /// [v1]
    /// "cargo-cleanme 0.1.3 (registry+https://github.com/rust-lang/crates.io-index)" = ["cargo-cleanme"]
    /// ```
    ///
    /// These tests used to hand-write `[packages.cargo-cleanme]` with a `vers`
    /// field. No cargo emits that any more, so the parser was being checked
    /// against its own assumption and agreed with itself. The schema is built
    /// here by a function rather than pasted per-test so that it is asserted in
    /// one place, and `the_crates_toml_we_parse_is_the_one_cargo_writes` pins
    /// the rendering against captured real output.
    fn real_crates_toml(package: &str, version: &str, executables: &[&str]) -> String {
        let names = executables
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "[v1]\n\"{package} {version} \
             (registry+https://github.com/rust-lang/crates.io-index)\" = [{names}]\n"
        )
    }

    /// A Cargo-managed installation as `cargo install --root DIR` builds it.
    ///
    /// Returns the live binary and the root Cargo recorded it in. The point of
    /// the helper is that `cargo_root` is *not* the process `CARGO_HOME`: the
    /// binary is in `<root>/bin` and the record is in `<root>/.crates.toml`.
    fn cargo_root_install(dir: &Path, version: &str) -> PathBuf {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join(PRODUCT);
        std::fs::write(&live, live_stub()).unwrap();
        std::fs::write(
            dir.join(".crates.toml"),
            real_crates_toml(PRODUCT, version, &[PRODUCT]),
        )
        .unwrap();
        live
    }

    #[test]
    fn the_crates_toml_we_parse_is_the_one_cargo_writes() {
        // Verbatim cargo 1.99.0 output, not a rendering of our own assumption.
        let captured = "[v1]\n\"cargo-cleanme 0.1.3 \
            (registry+https://github.com/rust-lang/crates.io-index)\" = [\"cargo-cleanme\"]\n";
        assert_eq!(
            real_crates_toml(PRODUCT, "0.1.3", &[PRODUCT]),
            captured,
            "our fixture drifted from the bytes cargo writes"
        );

        // And it must survive a real round trip through the parser.
        let dir = tempfile::tempdir().unwrap();
        let live = cargo_root_install(dir.path(), "0.1.3");
        assert_eq!(
            read_cargo_record(dir.path(), PRODUCT, PRODUCT),
            CargoRecord::Recorded {
                version: "0.1.3".to_owned()
            }
        );
        assert!(matches!(
            classify_provenance_in(&live, None),
            Provenance::CargoManaged { .. }
        ));
    }

    #[test]
    fn a_cargo_root_install_is_refused_even_when_it_is_not_the_cargo_home() {
        // The published defect: `cargo install --root DIR` puts the binary in a
        // root that is not the process CARGO_HOME. Reading only $CARGO_HOME/bin
        // missed it, and the binary was then classified as self-managed and
        // *replaced* -- overwriting a file Cargo owns and leaving
        // `cargo install --list` reporting a version that is no longer there.
        let dir = tempfile::tempdir().unwrap();
        let live = cargo_root_install(dir.path(), "0.1.3");

        // A CARGO_HOME that is somewhere else entirely, as in the rehearsal.
        let elsewhere = tempfile::tempdir().unwrap();
        let provenance = classify_provenance_in(&live, Some(elsewhere.path().to_path_buf()));
        assert!(
            matches!(provenance, Provenance::CargoManaged { .. }),
            "a --root installation was not recognised as Cargo-managed: {provenance:?}"
        );
        assert_eq!(
            provenance.remediation().as_deref(),
            Some("cargo install cargo-cleanme --locked --force"),
            "the manager command is the only correct answer for a Cargo-owned file"
        );
    }

    #[test]
    fn cargo_managed_installation_is_refused_with_the_manager_command() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_home = dir.path().to_path_buf();
        let bin = cargo_home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join("cargo-cleanme");
        std::fs::write(&live, live_stub()).unwrap();
        std::fs::write(
            cargo_home.join(".crates.toml"),
            real_crates_toml(PRODUCT, "0.1.0", &[PRODUCT]),
        )
        .unwrap();

        let provenance = classify_provenance_in(&live, Some(cargo_home.clone()));
        assert!(
            matches!(provenance, Provenance::CargoManaged { .. }),
            "{provenance:?}"
        );
        assert_eq!(
            provenance.remediation().as_deref(),
            Some("cargo install cargo-cleanme --locked --force")
        );

        // And a refused run must not download anything.
        let environment = FixtureEnvironment::new(live, "0.1.0").cargo_home(cargo_home.clone());
        let environment = release(environment, "9.9.9", candidate_bytes("9.9.9"), None);
        let error = run(&environment, false).expect_err("cargo-managed must be refused");
        assert!(matches!(error, UpdateError::Provenance { .. }), "{error:?}");
        let requested = environment.requested.borrow();
        assert!(
            requested.iter().all(|url| url == &crates_io_metadata_url()),
            "a refused run fetched release bytes: {requested:?}"
        );
    }

    #[test]
    fn an_unrecorded_file_in_a_cargo_root_is_not_ours_to_replace() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_home = dir.path().to_path_buf();
        let bin = cargo_home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join("cargo-cleanme");
        std::fs::write(&live, live_stub()).unwrap();
        // Cargo's root, but no record for this package.
        std::fs::write(
            cargo_home.join(".crates.toml"),
            real_crates_toml("some-other-tool", "3.0.0", &["sot"]),
        )
        .unwrap();

        let outcome = classify_provenance_in(&live, Some(cargo_home.clone()));
        match outcome {
            Provenance::UnprovableOwnership { detail } => {
                assert!(detail.contains("does not name this package"), "{detail}");
            }
            other => panic!("expected UnprovableOwnership, got {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // C018 regression evidence.
    //
    // Every test below fails against the pre-C018 implementation, because that
    // implementation returned `Option<String>` and then *fell through to
    // hashing* whenever it got `None`. Asserting the classification is not
    // enough on its own: the meaningful assertion is that an installation whose
    // ownership record could not be read is never classified self-managed,
    // because that classification is what authorizes replacement.
    // ------------------------------------------------------------------

    /// A live executable inside a Cargo root, with the root's record replaced by
    /// whatever the test wants to write.
    fn broken_cargo_root(dir: &Path, record: Option<&str>) -> PathBuf {
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join(PRODUCT);
        std::fs::write(&live, live_stub()).unwrap();
        match record {
            Some(text) => std::fs::write(dir.join(".crates.toml"), text).unwrap(),
            None => {
                let _ = std::fs::remove_file(dir.join(".crates.toml"));
            }
        }
        live
    }

    fn assert_not_self_managed(provenance: &Provenance, context: &str) {
        assert!(
            !matches!(provenance, Provenance::VerifiableSelfManaged { .. }),
            "{context} was classified self-managed, which authorizes replacing a file whose \
             ownership was never proven: {provenance:?}"
        );
    }

    #[test]
    fn unreadable_cargo_metadata_can_never_become_self_managed() {
        // A record file that exists but is not TOML. The baseline returned
        // `None` here, fell through to `hash_file`, and returned
        // `VerifiableSelfManaged` -- the fail-open direction.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(dir.path(), Some("this is not = valid = toml [[[\n"));
        let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
        assert_not_self_managed(&provenance, "malformed Cargo metadata in a Cargo home");
        assert!(
            matches!(provenance, Provenance::UnprovableOwnership { .. }),
            "expected UnprovableOwnership, got {provenance:?}"
        );
    }

    #[test]
    fn a_malformed_cargo_cleanme_record_is_not_skipped_into_self_managed() {
        // The record names this package but its executable list is the wrong
        // shape. It might still be the entry that claims this binary, so it is
        // uncertainty -- not absence.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(
            dir.path(),
            Some("[v1]\n\"cargo-cleanme 0.1.3 (registry+https://example.invalid)\" = 7\n"),
        );
        let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
        assert_not_self_managed(
            &provenance,
            "a malformed cargo-cleanme record in a Cargo home",
        );
    }

    #[test]
    fn an_unrelated_malformed_entry_does_not_hide_a_valid_record() {
        // The mirror image: a broken entry for a *different* package must not
        // make the whole record unreadable, or the search would stop at a
        // refusal and never reach the valid cargo-cleanme entry after it.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(
            dir.path(),
            Some(
                "[v1]\n\
                 \"broken-tool (registry+https://example.invalid)\" = 7\n\
                 \"cargo-cleanme 0.1.3 (registry+https://github.com/rust-lang/crates.io-index)\" \
                 = [\"cargo-cleanme\"]\n",
            ),
        );
        let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
        assert!(
            matches!(&provenance, Provenance::CargoManaged { version, .. } if version == "0.1.3"),
            "a valid record after an unrelated broken entry was not found: {provenance:?}"
        );
    }

    #[test]
    fn a_cargo_record_in_an_unsupported_shape_is_uncertain_not_absent() {
        // A `.crates.toml` with no `v1` table. Its presence says "this is a
        // Cargo root"; reading it as "Cargo owns nothing" would be trusting an
        // assumption instead of the record.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(dir.path(), Some("[v2]\n\"anything\" = []\n"));
        let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
        assert_not_self_managed(&provenance, "a .crates.toml in an unsupported shape");
    }

    #[test]
    fn an_explicit_cargo_home_bin_with_a_missing_record_is_unprovable() {
        // Manager-sensitive by construction: the executable is inside a Cargo
        // home's `bin` and Cargo's record has gone missing entirely.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(dir.path(), None);
        let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
        assert_not_self_managed(&provenance, "a Cargo home bin with no record file");
        match provenance {
            Provenance::UnprovableOwnership { detail } => {
                assert!(detail.contains("no record file"), "{detail}");
            }
            other => panic!("expected UnprovableOwnership, got {other:?}"),
        }
    }

    #[test]
    fn an_ancestor_cargo_root_with_unreadable_metadata_is_unprovable() {
        // The `cargo install --root` shape, where the owning root is above the
        // executable rather than being the process CARGO_HOME.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(dir.path(), Some("[[[ not toml at all\n"));
        let provenance = classify_provenance_in(&live, None);
        assert_not_self_managed(&provenance, "a --root install with unreadable metadata");
        assert!(
            matches!(provenance, Provenance::UnprovableOwnership { .. }),
            "expected UnprovableOwnership, got {provenance:?}"
        );
    }

    #[test]
    fn a_non_utf8_executable_name_in_a_cargo_root_is_unprovable() {
        // `.crates.toml` values are UTF-8, so a non-UTF-8 file name can never
        // be claimed by a record. The baseline turned it into `""` and then
        // read the resulting non-match as "Cargo records nothing here".
        //
        // Linux only, and narrower than `#[cfg(unix)]`: the premise is that a
        // raw-byte file name can exist on disk at all. APFS rejects one, so
        // this test failed on macOS at the `write` below — a fixture whose
        // premise does not hold on the lane, not a product failure. This is the
        // same narrowing `discovery.rs` applies to its own non-UTF-8 fixture.
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStrExt;
            let dir = tempfile::tempdir().unwrap();
            let bin = dir.path().join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            let live = bin.join(std::ffi::OsStr::from_bytes(b"cargo-cleanme-\xff"));
            std::fs::write(&live, live_stub()).unwrap();
            std::fs::write(
                dir.path().join(".crates.toml"),
                real_crates_toml(PRODUCT, "0.1.3", &["cargo-cleanme"]),
            )
            .unwrap();
            let provenance = classify_provenance_in(&live, Some(dir.path().to_path_buf()));
            assert_not_self_managed(
                &provenance,
                "a non-UTF-8 executable name inside a Cargo root",
            );
        }
    }

    #[test]
    fn a_genuinely_self_managed_installer_layout_stays_self_managed() {
        // The invariant that stops C018 from over-correcting: an ordinary
        // installer-managed `.../bin/cargo-cleanme` with no Cargo evidence
        // anywhere above it must still be updatable, or the corrective would
        // have broken every published install to fix a rarer case.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join(PRODUCT);
        std::fs::write(&live, live_stub()).unwrap();
        let provenance = classify_provenance_in(&live, None);
        assert!(
            matches!(provenance, Provenance::VerifiableSelfManaged { .. }),
            "an installer layout with no Cargo evidence became {provenance:?}"
        );
    }

    #[test]
    fn the_cargo_root_search_cannot_miss_a_supported_layout() {
        // The bound is now backed by the real layouts rather than asserted in a
        // comment. Every supported shape -- default `CARGO_HOME`, `cargo
        // install --root DIR`, a relocated `CARGO_HOME` -- puts the record in
        // the executable's *parent* directory, and container/hermetic images
        // nest that a level or two. Each of those is found here.
        for nesting in 0..=CARGO_ROOT_SEARCH_DEPTH {
            let dir = tempfile::tempdir().unwrap();
            let mut root = dir.path().to_path_buf();
            for level in 0..nesting {
                root = root.join(format!("nest-{level}"));
            }
            std::fs::create_dir_all(&root).unwrap();
            let live = cargo_root_install(&root, "0.1.3");
            let roots = cargo_roots_above(&live);
            assert!(
                roots.contains(&root),
                "a Cargo root {nesting} level(s) above the executable was missed: {root:?}"
            );
            // The nearest candidate is always the executable's own parent, which
            // is the layout Cargo actually writes.
            assert_eq!(
                roots.first().map(|r| r.as_path()),
                live.parent().and_then(Path::parent),
                "the nearest candidate root is not the executable's parent"
            );
        }

        // And the search is bounded rather than a filesystem crawl. The count is
        // `min(bound, available ancestors)`, so a deep path must still produce no
        // more candidates than the bound allows -- asserted as an upper bound
        // rather than an equality, because a shallow path legitimately runs out
        // of ancestors before it runs out of budget.
        let deep = tempfile::tempdir().unwrap();
        let mut deep_root = deep.path().to_path_buf();
        for level in 0..24 {
            deep_root = deep_root.join(format!("deep-{level}"));
        }
        let deep_live = cargo_root_install(&deep_root, "0.1.3");
        let count = cargo_roots_above(&deep_live).len();
        assert!(
            count <= CARGO_ROOT_SEARCH_DEPTH + 1,
            "24 levels of nesting produced {count} candidate roots, above the {CARGO_ROOT_SEARCH_DEPTH} bound"
        );
    }

    #[test]
    fn a_mutating_run_on_forbidden_provenance_makes_zero_registry_requests() {
        // Work package D, observed rather than asserted in prose: the refused
        // run must not have contacted the version authority at all.
        for (label, record) in [
            ("malformed record", Some("[[[ not toml\n")),
            ("missing record", None),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let live = broken_cargo_root(dir.path(), record);
            let environment =
                FixtureEnvironment::new(live.clone(), "0.1.0").cargo_home(dir.path().to_path_buf());
            let environment = release(environment, "9.9.9", candidate_bytes("9.9.9"), None);
            let error = run(&environment, false).expect_err("must refuse before the registry");
            assert!(
                matches!(error, UpdateError::Provenance { .. }),
                "{label}: {error:?}"
            );
            let requested = environment.requested.borrow();
            assert!(
                requested.is_empty(),
                "{label}: a refused run still contacted {requested:?}"
            );
            // And the live bytes are untouched.
            assert_eq!(
                std::fs::read(&live).unwrap(),
                live_stub(),
                "{label}: a refused run modified the live executable"
            );
        }
    }

    #[test]
    fn a_dry_run_still_reports_a_candidate_and_names_the_real_provenance() {
        // `--dry-run` is not mutating, so it keeps the version-authority
        // lookup. What it must never do is report a candidate for an
        // installation a real run would refuse -- the ordering fix in Work
        // package D would be a lie if the dry run hid the refusal.
        let dir = tempfile::tempdir().unwrap();
        let live = broken_cargo_root(dir.path(), Some("[[[ not toml\n"));
        let environment =
            FixtureEnvironment::new(live.clone(), "0.1.0").cargo_home(dir.path().to_path_buf());
        let environment = release(environment, "9.9.9", candidate_bytes("9.9.9"), None);
        let plan = run(&environment, true).expect("a dry run still resolves a candidate");
        assert_eq!(plan.to_version, "9.9.9");
        assert!(
            matches!(plan.provenance, Provenance::UnprovableOwnership { .. }),
            "the dry run hid the provenance a real run would refuse on: {:?}",
            plan.provenance
        );
        let requested = environment.requested.borrow();
        assert!(
            !requested.is_empty(),
            "a dry run is expected to consult the version authority"
        );
        assert_eq!(
            std::fs::read(&live).unwrap(),
            live_stub(),
            "a dry run wrote to the live executable"
        );
    }

    #[test]
    fn a_recorded_cargo_root_beyond_the_cargo_home_is_still_refused_before_the_registry() {
        // C017's `--root` success case must not regress: a real Cargo install
        // under `--root` is refused with the manager command and no network.
        let dir = tempfile::tempdir().unwrap();
        let live = cargo_root_install(dir.path(), "0.1.3");
        let elsewhere = tempfile::tempdir().unwrap();
        let environment =
            FixtureEnvironment::new(live, "0.1.3").cargo_home(elsewhere.path().to_path_buf());
        let environment = release(environment, "9.9.9", candidate_bytes("9.9.9"), None);
        let error = run(&environment, false).expect_err("a Cargo root must be refused");
        match error {
            UpdateError::Provenance {
                provenance: Provenance::CargoManaged { version, .. },
            } => assert_eq!(version, "0.1.3"),
            other => panic!("expected CargoManaged, got {other:?}"),
        }
        let requested = environment.requested.borrow();
        assert!(
            requested.is_empty(),
            "the refused --root install still contacted {requested:?}"
        );
    }

    #[test]
    fn the_published_installer_layout_is_not_mistaken_for_a_cargo_root() {
        // The ancestor walk is the other half of the fix, and its failure mode
        // is over-detection: a binary the published installer placed in
        // `~/.local/bin` (or `/usr/local/bin`) must still be self-managed.
        //
        // The neighbouring record below is deliberately pessimistic, because a
        // lenient fixture proves nothing here: with a record that mentions
        // nothing relevant, the correct answer and the broken answer are both
        // "self-managed" and the test cannot fail. It therefore plants the one
        // near miss that could by itself cause a wrongful claim -- a record for
        // a *different package* that installs a binary carrying our name. (The
        // mirror-image trap, our package installing a different binary, is
        // `a_cargo_record_that_omits_this_binary_does_not_claim_this_file`.)
        // Requiring both a package match and a binary match survives both.
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join(".local").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join(PRODUCT);
        std::fs::write(&live, live_stub()).unwrap();

        std::fs::write(
            home.path().join(".crates.toml"),
            real_crates_toml("some-other-tool", "1.0.0", &[PRODUCT]),
        )
        .unwrap();

        assert!(
            matches!(
                classify_provenance_in(&live, None),
                Provenance::VerifiableSelfManaged { .. }
            ),
            "an installer placement was wrongly claimed by a neighbouring Cargo record"
        );
    }

    #[test]
    fn a_cargo_record_that_omits_this_binary_does_not_claim_this_file() {
        // The record names the package but not the executable we are running.
        // Two `cargo install --root` layouts can share a root, and the value
        // list is what ties a record to a specific file.
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let live = bin.join(PRODUCT);
        std::fs::write(&live, live_stub()).unwrap();
        std::fs::write(
            dir.path().join(".crates.toml"),
            real_crates_toml(PRODUCT, "0.1.3", &["cargo-cleanme-elsewhere"]),
        )
        .unwrap();

        assert!(
            !matches!(
                classify_provenance_in(&live, None),
                Provenance::CargoManaged { .. }
            ),
            "a record for a different binary was used to claim this one"
        );
    }

    #[test]
    fn a_malformed_crates_toml_entry_does_not_hide_the_entry_after_it() {
        // `.crates.toml` is Cargo's file. One key we cannot parse -- a future
        // spec shape, say -- must not stop us reading the rest, or a Cargo
        // installation would quietly become unprovable and, on the pre-fix
        // detection, adoptable.
        //
        // The bad key has to land *inside* the `[v1]` table. An earlier draft of
        // this test put it above the header, which made it a root-level key the
        // parser never looks at: the test passed against code that aborted the
        // scan on the first bad entry. That is the blind-fixture shape this
        // subsystem has now paid for five times, so the splice is explicit and
        // the premise-negative in the closure record is the proof it lands.
        let dir = tempfile::tempdir().unwrap();
        let live = {
            let bin = dir.path().join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            let live = bin.join(PRODUCT);
            std::fs::write(&live, live_stub()).unwrap();
            live
        };
        // No version and no parenthesised source: unparseable as a spec string.
        let malformed = format!("\"{PRODUCT}\" = [\"{PRODUCT}\"]\n");
        let record = real_crates_toml(PRODUCT, "0.1.3", &[PRODUCT]);
        assert!(record.starts_with("[v1]\n"), "fixture shape changed");
        let spliced = record.replacen("[v1]\n", &format!("[v1]\n{malformed}"), 1);
        std::fs::write(dir.path().join(".crates.toml"), spliced).unwrap();

        assert!(
            matches!(
                classify_provenance_in(&live, None),
                Provenance::CargoManaged { .. }
            ),
            "an unparseable leading entry hid the real record"
        );
    }

    #[test]
    fn a_self_managed_installation_is_proven_by_its_digest() {
        let fx = fixture(&live_stub());
        let expected = eggup_core::hash_file(&fx.live).unwrap();
        match classify_provenance(&fx.live) {
            Provenance::VerifiableSelfManaged { digest } => assert_eq!(digest, expected),
            other => panic!("expected VerifiableSelfManaged, got {other:?}"),
        }
    }

    #[test]
    fn ownership_is_never_inferred_from_the_file_name() {
        // A file that merely *looks* like ours, in a directory that merely looks
        // like an install root, with a directory path it cannot be hashed from.
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("bin").join("cargo-cleanme");
        std::fs::create_dir_all(fake.parent().unwrap()).unwrap();
        std::fs::create_dir(&fake).unwrap();
        assert!(matches!(
            classify_provenance(&fake),
            Provenance::UnprovableOwnership { .. }
        ));
    }

    // ------------------------------------------------------------ transaction

    // Platform scope for the commit-path cases below.
    //
    // Eggup's `ExactIdentityValidator` *executes* the acquired candidate to
    // prove its own identity, so the fixture candidate has to be a real
    // executable for the platform under test. `candidate_bytes` therefore
    // writes a `#!/bin/sh` script on Unix and inert `MZ` bytes elsewhere: a
    // script is not a Windows executable, and pretending otherwise would make
    // every Windows lane green while the commit path never ran.
    //
    // The cases that actually execute the candidate are consequently
    // `#[cfg(unix)]`. The cases above them are not scoped, because they never
    // execute anything: the transport is a trait fixture
    // (`FixtureEnvironment`), the version authority is fixture metadata, and
    // the assertions are about planning, ownership, and refusal. Those run
    // everywhere, truthfully.
    //
    // Windows commit-path evidence therefore comes from C014's live
    // self-update rehearsal against a real published release, not from a
    // fixture that could not run.
    #[cfg(unix)]
    #[test]
    fn a_verified_candidate_replaces_the_live_binary() {
        let fx = fixture(&live_stub());
        let candidate = candidate_bytes("0.2.0");
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate.clone(), None);
        let plan = run(&environment, false).expect("update commits");
        assert_eq!(plan.to_version, "0.2.0");
        assert_eq!(std::fs::read(&fx.live).unwrap(), candidate);
    }

    #[cfg(unix)]
    #[test]
    fn a_checksum_mismatch_leaves_the_live_binary_untouched() {
        let fx = fixture(&live_stub());
        let before = std::fs::read(&fx.live).unwrap();
        let asset = asset_for_target(host_target().unwrap()).unwrap();
        let tag = "v0.2.0";
        let environment = FixtureEnvironment::new(fx.live.clone(), "0.1.0")
            .respond(
                &crates_io_metadata_url(),
                br#"{"crate":{"max_stable_version":"0.2.0"}}"#.to_vec(),
            )
            .respond(
                &FixtureEnvironment::asset_url(tag, &format!("{asset}.sha256")),
                b"0000000000000000000000000000000000000000000000000000000000000000  ".to_vec(),
            )
            .respond(
                &FixtureEnvironment::asset_url(tag, asset),
                candidate_bytes("0.2.0"),
            );
        let error = run(&environment, false).expect_err("mismatch must fail");
        assert!(
            matches!(error, UpdateError::Transaction { .. }),
            "{error:?}"
        );
        assert_eq!(std::fs::read(&fx.live).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn a_candidate_reporting_the_wrong_version_is_rejected_before_commit() {
        let fx = fixture(&live_stub());
        let before = std::fs::read(&fx.live).unwrap();
        // Registry and sidecar agree; the *bytes* lie about who they are.
        let lying = candidate_bytes("6.6.6");
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", lying.clone(), None);
        let error = run(&environment, false).expect_err("wrong identity must fail");
        let text = error.to_string();
        assert!(text.contains("0.2.0"), "{text}");
        assert_eq!(std::fs::read(&fx.live).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn a_sidecar_naming_a_different_asset_is_rejected() {
        let fx = fixture(&live_stub());
        let before = std::fs::read(&fx.live).unwrap();
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        // Sidecar digest is correct for these bytes, but it is evidence for a
        // different file, so it must not authorize this download.
        let environment = release(
            base,
            "0.2.0",
            candidate_bytes("0.2.0"),
            Some("cargo-cleanme-some-other-target"),
        );
        let error = run(&environment, false).expect_err("mismatched sidecar must fail");
        assert!(
            matches!(error, UpdateError::ReleaseEvidence { .. }),
            "{error:?}"
        );
        assert_eq!(std::fs::read(&fx.live).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn an_absent_release_asset_is_not_replaced_with_anything() {
        let fx = fixture(&live_stub());
        let before = std::fs::read(&fx.live).unwrap();
        let asset = asset_for_target(host_target().unwrap()).unwrap();
        let environment = FixtureEnvironment::new(fx.live.clone(), "0.1.0")
            .respond(
                &crates_io_metadata_url(),
                br#"{"crate":{"max_stable_version":"0.2.0"}}"#.to_vec(),
            )
            .respond(
                &FixtureEnvironment::asset_url("v0.2.0", &format!("{asset}.sha256")),
                format!("{}  {asset}\n", sha2_digest(&candidate_bytes("0.2.0"))).into_bytes(),
            )
            .absent(&FixtureEnvironment::asset_url("v0.2.0", asset));
        let error = run(&environment, false).expect_err("absent asset must fail");
        assert!(
            matches!(error, UpdateError::ReleaseEvidence { .. }),
            "{error:?}"
        );
        assert_eq!(std::fs::read(&fx.live).unwrap(), before);
    }

    /// Every process-wide temp path whose basename carries the
    /// `cargo-cleanme-update-test-` prefix.
    ///
    /// This is the **historical** detector, retained only as the premise-negative
    /// in `a_foreign_staging_directory_is_not_evidence_of_a_leak`. It is not a
    /// leak check and must never be used as one.
    ///
    /// Every fixture in this module mints staging names through
    /// `unique("cargo-cleanme-update-test")`, so this prefix is shared by all
    /// of them, and Rust runs test functions in parallel. The question it
    /// answers — "does any matching path exist right now?" — is therefore
    /// answered partly by whichever other test happens to be mid-transaction.
    /// It could fail for a transaction that leaked nothing, and it could pass
    /// for a transaction that leaked everything.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn process_wide_staging_paths() -> Vec<PathBuf> {
        let mut found = Vec::new();
        if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("cargo-cleanme-update-test-") {
                    found.push(entry.path());
                }
            }
        }
        found
    }

    #[cfg(unix)]
    #[test]
    fn staging_is_cleaned_up_on_success_and_on_failure() {
        // Failure path: the candidate reports the wrong version, so the
        // transaction aborts after staging. Its staging directory must be gone
        // all the same.
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("6.6.6"), None);
        let _ = run(&environment, false);
        environment.assert_no_staging_leak("failed transaction");

        // Success path.
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("0.2.0"), None);
        run(&environment, false).expect("update commits");
        environment.assert_no_staging_leak("successful transaction");
    }

    /// The negative control for the corrected check.
    ///
    /// A leak detector that has never seen a leak is not a detector. This
    /// fixture reaches the production cleanup callback and deliberately leaves
    /// its own staging directory behind, and the fixture-owned check must
    /// report it.
    ///
    /// It also pins the distinction the old test blurred: `cleanup_calls` is
    /// greater than zero *and* the path still exists. Clearing an in-memory
    /// `Option` is not filesystem cleanup evidence, and a test that accepted
    /// the former in place of the latter would pass here.
    #[test]
    fn a_deliberately_leaked_staging_path_is_still_reported() {
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("6.6.6"), None).leaking_staging();
        let _ = run(&environment, false);

        assert!(
            environment.cleanup_calls() > 0,
            "premise: production must still have reached the cleanup callback, \
             otherwise this case proves nothing about a leaked path"
        );
        let leaked = environment.leaked_staging();
        assert_eq!(
            leaked,
            environment.created_staging.borrow().clone(),
            "the leak check must report exactly the paths this fixture created"
        );
        for path in leaked {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    /// The concurrency control, and the proof of the historical diagnosis.
    ///
    /// A second fixture-owned staging directory with the *same* prefix is held
    /// live across the subject's transaction. The subject must pass, which
    /// proves the corrected check consumes only its own state. In the same
    /// breath, the historical process-wide scan must report that foreign path —
    /// which is precisely the input that made the old assertion fail.
    #[cfg(unix)]
    #[test]
    fn a_foreign_staging_directory_is_not_evidence_of_a_leak() {
        let foreign = unique("cargo-cleanme-update-test");
        std::fs::create_dir_all(&foreign).unwrap();
        assert!(
            process_wide_staging_paths().contains(&foreign),
            "premise: the historical process-wide scan must see the foreign \
             directory, or this case proves nothing about the old detector"
        );

        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("0.2.0"), None);
        run(&environment, false).expect("update commits");

        // The subject completed correctly and left nothing of its own behind,
        // even though a same-prefix foreign path was live the whole time.
        environment.assert_no_staging_leak("subject beside a foreign staging path");
        assert!(
            process_wide_staging_paths().contains(&foreign),
            "premise: the foreign directory must still be live, or the \
             concurrency control did not test anything"
        );

        let _ = std::fs::remove_dir_all(&foreign);
    }

    #[test]
    fn a_dry_run_downloads_nothing() {
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("0.2.0"), None);
        run(&environment, true).expect("dry run resolves");
        let requested = environment.requested.borrow();
        assert_eq!(
            requested.len(),
            1,
            "dry run fetched more than metadata: {requested:?}"
        );
        assert_eq!(requested[0], crates_io_metadata_url());
    }

    // --------------------------------------------------------------- contract

    #[test]
    fn the_published_target_table_matches_the_eggpack_contract() {
        // The compiled-in table is a copy of release/eggpack/distribution.toml.
        // scripts/check-release-contract.py proves the same invariant in CI;
        // this asserts the table itself is exactly the contract's five targets.
        let contract = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/release/eggpack/distribution.toml"
        ))
        .expect("the distribution contract is part of the repository");
        for (triple, asset) in PUBLISHED_TARGETS {
            assert!(
                contract.contains(&format!("triple = \"{triple}\"")),
                "{triple} is missing from the contract"
            );
            // The contracted asset template is `{product}-{target}` with an
            // `.exe` suffix on Windows; expanding it here must reproduce the
            // compiled-in name exactly.
            let expected = if triple.ends_with("-pc-windows-msvc") {
                format!("cargo-cleanme-{triple}.exe")
            } else {
                format!("cargo-cleanme-{triple}")
            };
            assert_eq!(
                *asset, expected,
                "asset name for {triple} is not the contracted expansion"
            );
        }
        assert_eq!(PUBLISHED_TARGETS.len(), 5);
    }

    #[test]
    fn versions_order_numerically_not_lexically() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_release_versions("0.1.10", "0.1.9"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_release_versions("0.10.0", "0.9.9"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_release_versions("1.0.0", "1.0.0"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_release_versions("0.1.0", "0.2.0"),
            Some(Ordering::Less)
        );
        for bad in [("1.2", "1.2.0"), ("1.2.3-rc.1", "1.2.3"), ("x", "1.2.3")] {
            assert_eq!(compare_release_versions(bad.0, bad.1), None, "{bad:?}");
        }
    }

    #[test]
    fn a_newer_local_build_is_never_downgraded() {
        // Regression guard: `update` reached the registry, found 0.1.0, and
        // offered to "update" a 0.1.1 build down to 0.1.0. Found by the live
        // release smoke, not by a fixture.
        let base = FixtureEnvironment::new(PathBuf::from("/nonexistent"), "0.1.1");
        let environment = release(base, "0.1.0", candidate_bytes("0.1.0"), None);
        match run(&environment, true) {
            Err(UpdateError::NewerThanPublished { current, published }) => {
                assert_eq!(current, "0.1.1");
                assert_eq!(published, "0.1.0");
            }
            other => panic!("expected NewerThanPublished, got {other:?}"),
        }
    }

    #[test]
    fn an_older_published_version_is_not_offered_to_a_matching_build() {
        // A build that is exactly the published version is already current.
        let base = FixtureEnvironment::new(PathBuf::from("/nonexistent"), "0.1.0");
        let environment = release(base, "0.1.0", candidate_bytes("0.1.0"), None);
        assert!(matches!(
            run(&environment, true),
            Err(UpdateError::AlreadyCurrent { .. })
        ));
    }

    #[test]
    fn the_production_transport_identifies_itself_descriptively() {
        // Regression guard for the 0.1.0 defect.
        //
        // The original production transport was `eggup-curl`, which has no
        // User-Agent seam and clears the child environment, so every request
        // went out as `curl/x.y`. The crates.io registry answers HTTP 403 to any
        // non-descriptive User-Agent, so `update` could not read the version
        // authority at all. No fixture test could catch that, because no fixture
        // talks to the registry.
        //
        // This asserts the property the requirement actually is: whatever
        // transport is wired must send an agent that names the product. It does
        // not model the registry's policy, which would be a fixture that lies
        // about the thing most likely to change.
        let environment =
            HttpEnvironment::new().expect("the production transport must be constructible");
        let agent = environment.transport.config().user_agent.clone();
        assert!(
            agent.contains(PRODUCT),
            "the update User-Agent must name the product, got {agent:?}"
        );
        assert!(
            !agent.starts_with("curl/") && agent != *"",
            "the update User-Agent must not be a generic tool name, got {agent:?}"
        );
        assert_eq!(agent, USER_AGENT);
        // A bare token is exactly what a registry rejects.
        assert!(
            agent.contains('/') && agent.contains(' '),
            "expected a descriptive agent, got {agent:?}"
        );
    }

    #[test]
    fn the_version_authority_url_is_https_and_product_scoped() {
        let url = crates_io_metadata_url();
        assert!(url.starts_with("https://"), "{url}");
        assert!(url.ends_with(PRODUCT), "{url}");
    }

    #[test]
    fn release_urls_are_constructed_from_the_tag_and_contracted_asset() {
        let asset = asset_for_target(host_target().unwrap()).unwrap();
        let url = release_asset_url("v9.9.9", asset);
        assert!(url.starts_with("https://github.com/dbowm91/cargo-cleanme/releases/download/"));
        assert!(url.ends_with(asset), "{url}");
        // No traversal or injection is possible from a validated tag.
        assert!(!url.contains(".."), "{url}");
    }

    #[test]
    fn the_host_target_has_a_contracted_asset() {
        let target = host_target().expect("the CI hosts are all published");
        assert!(
            asset_for_target(target).is_some(),
            "{target} has no contracted asset"
        );
    }
}
