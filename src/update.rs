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

    if let Some(cargo_root) = cargo_bin_root_containing(current_exe, cargo_home.as_deref()) {
        match cargo_recorded_version(&cargo_root, PRODUCT) {
            Some(version) => {
                return Provenance::CargoManaged {
                    bin_root: cargo_root,
                    version,
                };
            }
            None => {
                // A file in a Cargo root that Cargo does not record is not
                // positively owned by anything, and is certainly not
                // positively owned by us.
                return Provenance::UnprovableOwnership {
                    detail: format!(
                        "{} is inside the Cargo bin root {} but Cargo records no installed \
                         package named {PRODUCT} there",
                        current_exe.display(),
                        cargo_root.display()
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

/// The Cargo bin root that `path` lives in, if any.
///
/// This is a *location* test only. It never grants ownership on its own; the
/// caller must still find a recorded Cargo package there.
fn cargo_bin_root_containing(path: &Path, cargo_home: Option<&Path>) -> Option<PathBuf> {
    let root = cargo_home?.join("bin");
    if path.parent() == Some(root.as_path()) {
        return Some(root);
    }
    None
}

/// The version Cargo's `.crates.toml` records for `package`, if any.
fn cargo_recorded_version(bin_root: &Path, package: &str) -> Option<String> {
    let cargo_home = bin_root.parent()?;
    let record = std::fs::read_to_string(cargo_home.join(".crates.toml")).ok()?;
    let value: toml::Value = toml::from_str(&record).ok()?;
    let entry = value.get("packages")?.get(package)?;
    // Cargo writes the newest installed version as `vers`.
    let version = entry.get("vers").and_then(toml::Value::as_str)?;
    Some(version.to_owned())
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
        let validator =
            ExactIdentityValidator::new(member_id.clone(), expected_identity(&plan.to_version));
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
    fn candidate_bytes(version: &str) -> Vec<u8> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = unique(&format!("cargo-cleanme-candidate-{version}"));
            std::fs::write(&path, format!("#!/bin/sh\necho '{PRODUCT} {version}'\n")).unwrap();
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
            "[packages.cargo-cleanme]\nvers = \"0.1.0\"\n",
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
            "[packages.some-other-tool]\nvers = \"3.0.0\"\n",
        )
        .unwrap();

        let outcome = classify_provenance_in(&live, Some(cargo_home.clone()));
        match outcome {
            Provenance::UnprovableOwnership { detail } => {
                assert!(
                    detail.contains("Cargo records no installed package"),
                    "{detail}"
                );
            }
            other => panic!("expected UnprovableOwnership, got {other:?}"),
        }
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

    /// Any staging directory this test module created and did not clean up.
    #[cfg_attr(not(unix), allow(dead_code))]
    fn staging_leftovers() -> Vec<PathBuf> {
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
        // Failure path.
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("6.6.6"), None);
        let _ = run(&environment, false);
        assert!(
            staging_leftovers().is_empty(),
            "staging left behind: {:?}",
            staging_leftovers()
        );

        // Success path.
        let fx = fixture(&live_stub());
        let base = FixtureEnvironment::new(fx.live.clone(), "0.1.0");
        let environment = release(base, "0.2.0", candidate_bytes("0.2.0"), None);
        run(&environment, false).expect("update commits");
        assert!(
            staging_leftovers().is_empty(),
            "staging left behind: {:?}",
            staging_leftovers()
        );
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
