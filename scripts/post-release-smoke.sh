#!/usr/bin/env bash
# Real, networked self-update rehearsal against two *public* releases.
#
# This is Work Package F of C014 as a script, so the rehearsal is repeatable
# rather than a sequence of ad-hoc commands. It exercises the **released**
# updater, not a freshly built copy: the binary under test is the public
# `$FROM_VERSION` asset, verified against its published sidecar, and it
# self-updates to the public `$TO_VERSION`.
#
#   scripts/post-release-smoke.sh --target x86_64-unknown-linux-gnu \
#       --from 0.1.1 --to 0.1.2
#
# Two scenarios run in isolated state:
#
#   self-managed  a copy of the published binary outside any Cargo root, which
#                 must self-update in place, and whose post-update digest must
#                 equal the published `$TO_VERSION` asset digest exactly.
#   cargo-managed the same version installed by Cargo into an isolated root,
#                 which must refuse as `cargo_managed`, name the manager command
#                 for the new version, and leave the managed bytes untouched.
#
# Design constraints, each of which is a requirement rather than a preference:
#
#   * No publication credentials are read or needed. Everything comes from public
#     HTTPS endpoints.
#   * Only the per-invocation temporary directory is mutated. No system or global
#     installation is touched.
#   * A network failure is a failure. Nothing here is allowed to skip, and no
#     command is run under `|| true` where a failure would hide.
#   * The premise of every scenario is asserted before its behaviour is: the
#     installed binary's identity and digest are proven to be the published ones
#     before `update` is asked to do anything.
set -euo pipefail

PRODUCT=cargo-cleanme
REPO_SLUG=dbowm91/cargo-cleanme
TARGET=""
FROM_VERSION=""
TO_VERSION=""
KEEP_WORKDIR=0
SKIP_CARGO_MANAGED=0

fail() { printf 'post-release-smoke: FAILED: %s\n' "$1" >&2; exit 1; }
note() { printf '  %s\n' "$1"; }

usage() {
  sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
  cat <<'EOF'

Options:
  --target <triple>       host release target to rehearse
  --from <version>        published version to start from
  --to <version>          published version to update to
  --keep                  keep the temporary tree for inspection
  --skip-cargo-managed    run only the self-managed scenario
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET=${2:-}; shift 2 ;;
    --from) FROM_VERSION=${2:-}; shift 2 ;;
    --to) TO_VERSION=${2:-}; shift 2 ;;
    --keep) KEEP_WORKDIR=1; shift ;;
    --skip-cargo-managed) SKIP_CARGO_MANAGED=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; fail "unknown argument $1" ;;
  esac
done

[[ -n "$TARGET" ]] || fail "--target is required"
[[ -n "$FROM_VERSION" ]] || fail "--from is required"
[[ -n "$TO_VERSION" ]] || fail "--to is required"
[[ "$FROM_VERSION" != "$TO_VERSION" ]] || fail "--from and --to must differ"

# A release target's asset is named after the triple, with `.exe` on Windows.
case "$TARGET" in
  *-pc-windows-msvc) ASSET_SUFFIX=".exe" ;;
  *-apple-darwin|*x86_64-apple-darwin|aarch64-apple-darwin) ASSET_SUFFIX="" ;;
  *-linux-gnu) ASSET_SUFFIX="" ;;
  *) fail "unsupported release target $TARGET; this script only rehearses the five contracted targets" ;;
esac
ASSET="$PRODUCT-$TARGET$ASSET_SUFFIX"

digest_of() {
  # Portable SHA-256. python3 is present on every supported runner; the two
  # coreutils spellings are the fallback for a bare shell.
  if command -v python3 >/dev/null 2>&1; then
    python3 -c 'import hashlib,sys;print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$1"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d" " -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d" " -f1
  else
    fail "no SHA-256 implementation available; cannot verify published digests"
  fi
}

require() {
  # Every assertion is a named require: a failure says which claim failed, and
  # there is no path that reports success without having checked the claim.
  if [[ "$1" != "true" ]]; then
    fail "${2}"
  fi
}

WORK=$(mktemp -d "${TMPDIR:-/tmp}/$PRODUCT-smoke.XXXXXXXX")
cleanup() {
  if [[ "$KEEP_WORKDIR" == "1" ]]; then
    note "temporary tree kept at $WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

fetch_release_asset() {
  # $1 version, $2 asset name, $3 destination path.
  local version=$1 asset=$2 destination=$3
  local base="https://github.com/$REPO_SLUG/releases/download/v$version"
  # A missing asset must fail loudly: a rehearsal that silently installs
  # nothing would report coverage that does not exist.
  curl --fail --silent --show-error --location --retry 3 --retry-delay 2 \
    --output "$destination" "$base/$asset" \
    || fail "could not download $asset for v$version from $base (network failure or missing asset)"
  curl --fail --silent --show-error --location --retry 3 --retry-delay 2 \
    --output "$destination.sha256" "$base/$asset.sha256" \
    || fail "could not download the $asset sidecar for v$version (network failure or missing asset)"
}

verify_sidecar() {
  # $1 asset path. The sidecar is the published evidence; the asset must match it.
  # The sidecar is `<digest>  <asset name>`, so the digest is the first field.
  local asset=$1 published actual
  published=$(awk '{print $1}' "$asset.sha256" | tr -d ' \t\r\n')
  actual=$(digest_of "$asset")
  require "$([[ "$published" == "$actual" ]] && echo true || echo false)" \
    "published digest for $(basename "$asset") does not match the sidecar (sidecar $published, actual $actual)"
  printf '%s' "$actual"
}

# The updater stages into `std::env::temp_dir()` under
# `cargo-cleanme-update-<pid>-<nanos>` and documents that it removes that
# directory on both success and failure. Redirecting the process temp directory
# into this invocation's own tree is what makes "no staging residue remains"
# checkable without inspecting a shared system directory.
export TMPDIR="$WORK/tmp"
export TEMP="$WORK/tmp"
export TMP="$WORK/tmp"
mkdir -p "$WORK/tmp"

printf 'post-release-smoke: %s v%s -> v%s (public releases only)\n' "$TARGET" "$FROM_VERSION" "$TO_VERSION"
note "workdir $WORK"

# ---------------------------------------------------------------- premise ----
# The rehearsal is only meaningful if the binary under test really is the
# published FROM release, so that is proven first, for both scenarios, from the
# downloaded bytes.
FROM_ASSET="$WORK/$ASSET.from"
fetch_release_asset "$FROM_VERSION" "$ASSET" "$FROM_ASSET"
FROM_DIGEST=$(verify_sidecar "$FROM_ASSET")
note "published v$FROM_VERSION $ASSET sha256 $FROM_DIGEST"

TO_DIGEST_FILE="$WORK/$ASSET.to"
fetch_release_asset "$TO_VERSION" "$ASSET" "$TO_DIGEST_FILE"
TO_DIGEST=$(verify_sidecar "$TO_DIGEST_FILE")
note "published v$TO_VERSION $ASSET sha256 $TO_DIGEST"

# -------------------------------------------------- scenario: self-managed ----
SELF_DIR="$WORK/self-managed"
mkdir -p "$SELF_DIR/bin"
SELF_BIN="$SELF_DIR/bin/$PRODUCT$ASSET_SUFFIX"
cp "$FROM_ASSET" "$SELF_BIN"
chmod +x "$SELF_BIN"

observed=$(digest_of "$SELF_BIN")
require "$([[ "$observed" == "$FROM_DIGEST" ]] && echo true || echo false)" \
  "the installed binary is not the published v$FROM_VERSION asset (expected $FROM_DIGEST, found $observed)"

version=$("$SELF_BIN" --version | awk '{print $2}')
require "$([[ "$version" == "$FROM_VERSION" ]] && echo true || echo false)" \
  "the published v$FROM_VERSION asset reports version $version"

printf '\nself-managed: update without --dry-run\n'
update_output=$("$SELF_BIN" update --config "$SELF_DIR/config.toml" --no-progress 2>&1) \
  || fail "update failed on a self-managed installation: $update_output"

after_version=$("$SELF_BIN" --version | awk '{print $2}')
require "$([[ "$after_version" == "$TO_VERSION" ]] && echo true || echo false)" \
  "after update the binary reports $after_version, expected $TO_VERSION"

# The requirement is equality with the *published* asset, not merely a
# successful version bump: a binary that reports the right version while being
# the wrong bytes would pass everything else.
after_digest=$(digest_of "$SELF_BIN")
require "$([[ "$after_digest" == "$TO_DIGEST" ]] && echo true || echo false)" \
  "the updated binary digest $after_digest is not the published v$TO_VERSION asset digest $TO_DIGEST"
note "updated in place, digest equals the published v$TO_VERSION asset"

# Residue: the only things that may remain beside the installation are the
# installation itself and its config. Anything else would be a backup or a
# partial replacement the updater failed to clean up.
unexpected=$(find "$SELF_DIR" -mindepth 1 -maxdepth 1 ! -name bin ! -name config.toml 2>/dev/null || true)
require "$([[ -z "$unexpected" ]] && echo true || echo false)" \
  "unexpected residue beside the installation: $unexpected"

# The updater's own staging directory must be gone. TMPDIR is this
# invocation's tree, so anything here is residue from the update just performed.
staging_residue=$(find "$WORK/tmp" -mindepth 1 -maxdepth 1 -name "$PRODUCT-update-*" 2>/dev/null || true)
require "$([[ -z "$staging_residue" ]] && echo true || echo false)" \
  "the updater left staging behind: $staging_residue"

printf '\nself-managed: update --dry-run must report already-current\n'
# "Already current" is deliberately *not* a success exit: the tool exits 2 so a
# script cannot mistake "nothing to do" for "updated". So the assertion here is
# the message, and a non-zero exit carrying that message is the expected
# outcome, not a failure. Asserting exit 0 would have reported a working
# updater as broken.
dry=$("$SELF_BIN" update --config "$SELF_DIR/config.toml" --dry-run --no-progress 2>&1) && dry_rc=0 || dry_rc=$?
require "$(grep -qi 'already at the latest stable version' <<<"$dry" && echo true || echo false)" \
  "update --dry-run did not report already-current (exit $dry_rc): $dry"
require "$(grep -q "$TO_VERSION" <<<"$dry" && echo true || echo false)" \
  "the already-current report does not name v$TO_VERSION: $dry"
note "already-current reported (exit $dry_rc, which is the documented 'nothing to do' status)"

# ------------------------------------------------ scenario: cargo-managed ----
if [[ "$SKIP_CARGO_MANAGED" == "1" ]]; then
  note "cargo-managed scenario skipped by request"
else
  MANAGED_ROOT="$WORK/cargo-root"
  printf '\ncargo-managed: install exact v%s into an isolated root\n' "$FROM_VERSION"
  # --version pins the exact published version; --root keeps every byte inside
  # this invocation's temporary directory, so no system installation is touched.
  # `cargo install` has no --no-progress flag; passing one aborts the install.
  if ! command -v cargo >/dev/null 2>&1; then
    fail "cargo is required for the cargo-managed scenario"
  fi
  CARGO_HOME="$WORK/cargo-home" cargo install "$PRODUCT" --version "$FROM_VERSION" \
    --locked --root "$MANAGED_ROOT" \
    || fail "could not install the published v$FROM_VERSION through Cargo"
  MANAGED_BIN="$MANAGED_ROOT/bin/$PRODUCT$ASSET_SUFFIX"
  [[ -x "$MANAGED_BIN" ]] || fail "cargo install did not place a binary at $MANAGED_BIN"

  managed_before=$(digest_of "$MANAGED_BIN")
  # A Cargo installation is a *local rebuild* of the published source, so its
  # bytes are deliberately not the release asset's: the release asset is built
  # on the release runners. The first version of this harness asserted the two
  # digests were equal, which is false by construction, and it failed on a
  # perfectly correct install. What matters here is that the managed binary
  # reports the right version and that its bytes do not change across the
  # refusal, so the digest is recorded rather than compared to the asset.
  managed_version=$("$MANAGED_BIN" --version | awk '{print $2}')
  require "$([[ "$managed_version" == "$FROM_VERSION" ]] && echo true || echo false)" \
    "the Cargo-installed binary reports $managed_version, expected $FROM_VERSION"
  note "Cargo-managed install reports $managed_version (local build, sha256 ${managed_before:0:12}, not the release asset)"

  managed_output=$("$MANAGED_BIN" update --config "$MANAGED_ROOT/config.toml" --no-progress 2>&1) && \
    managed_rc=0 || managed_rc=$?
  require "$([[ "$managed_rc" != "0" ]] && echo true || echo false)" \
    "update succeeded on a Cargo-managed installation; it must refuse"
  require "$(grep -qi 'cargo_managed\|cargo install' <<<"$managed_output" && echo true || echo false)" \
    "the refusal did not identify Cargo ownership: $managed_output"
  # The remediation is the *manager command*, asserted exactly.
  #
  # This assertion used to require the refusal to name v$TO_VERSION, which is
  # not satisfiable by a correct refusal: `Provenance::CargoManaged::remediation`
  # returns `cargo install cargo-cleanme --locked --force`, and deliberately so.
  # That command resolves to the newest published release on its own, which is
  # the whole point of handing a Cargo user back to their package manager; a
  # version-pinned reinstall would be the wrong instruction for someone who ran
  # `update` in order to get the newer version.
  #
  # The bug was latent rather than obvious because the cargo-managed scenario
  # had never previously reached this line: before the C017 fix the binary
  # succeeded and the harness failed one assertion earlier. Asserting the exact
  # command is also stronger than looking for a version substring, which any
  # message mentioning the release would satisfy.
  require "$(grep -q 'cargo install '"$PRODUCT"' --locked --force' <<<"$managed_output" && echo true || echo false)" \
    "the remediation is not the manager command: $managed_output"

  managed_after=$(digest_of "$MANAGED_BIN")
  require "$([[ "$managed_after" == "$managed_before" ]] && echo true || echo false)" \
    "the Cargo-managed binary changed after a refusal ($managed_before -> $managed_after)"
  note "refused as Cargo-managed, remediation is the manager command, bytes untouched"
fi

printf '\npost-release-smoke: PASSED (%s v%s -> v%s)\n' "$TARGET" "$FROM_VERSION" "$TO_VERSION"
