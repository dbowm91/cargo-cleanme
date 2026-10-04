#!/bin/sh
# cargo-cleanme public bootstrap installer.
#
# Binary-first, fail-closed installer for the verified GitHub release assets.
# Producer asset names and target triples are owned by Eggpack
# (`release/eggpack/distribution.toml`); this wrapper owns latest/exact version
# selection, host mapping, integrity and identity verification, destination
# scope, and the Cargo source fallback policy. It is deliberately not a second
# release schema: `scripts/check-installer-contract.py` proves that every
# target triple and asset name below is a mechanical projection of the
# contract.
#
# Security posture:
#   * HTTPS only; no HTTP downgrade, no redirect to another host scheme.
#   * curl option injection from the environment is disabled where the
#     platform curl supports it.
#   * The SHA-256 sidecar is mandatory. A missing or malformed digest is a
#     hard failure, never a Cargo fallback.
#   * The downloaded candidate must print exactly `cargo-cleanme X.Y.Z`
#     before it is placed.
#   * No internal privilege escalation: this script never calls sudo, su, or
#     doas. Run it yourself as root for a system-wide install.
#
# Usage:
#   install.sh                    # latest stable release
#   install.sh --version 0.1.0    # exact release
#   install.sh --dir <path>       # override the install directory
#   install.sh --no-path          # do not print PATH guidance
#   install.sh --help

set -eu

PRODUCT="cargo-cleanme"
# Overridable for deterministic local fixture tests. The default is the public
# release origin; a non-HTTPS override is refused by the transport checks.
REPO="dbowm91/cargo-cleanme"
BASE_URL="${CARGO_CLEANME_INSTALL_BASE_URL:-https://github.com/$REPO/releases/download}"
LATEST_URL="${CARGO_CLEANME_INSTALL_LATEST_URL:-https://github.com/$REPO/releases/latest/download}"
USER_AGENT="cargo-cleanme-installer"

VERSION=""
INSTALL_DIR=""
SHOW_PATH_HINT=1
FORCE=0

TMP_DIR=""
cleanup() {
	if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
		rm -rf "$TMP_DIR"
	fi
}
trap cleanup EXIT HUP INT TERM

fail() {
	printf '%s: %s\n' "$PRODUCT" "$1" >&2
	exit 1
}

info() {
	printf '%s\n' "$1"
}

usage() {
	cat <<EOF
$PRODUCT installer

Usage:
  install.sh [--version X.Y.Z] [--dir PATH] [--force] [--no-path] [--help]

Options:
  --version X.Y.Z  Install an exact release instead of the latest stable one.
  --dir PATH       Install into PATH instead of the default location.
  --force          Replace an existing cargo-cleanme in the install directory.
  --no-path        Do not print PATH guidance.
  --help           Show this message.

Default install directory:
  non-root   \$HOME/.local/bin
  root       /usr/local/bin
EOF
}

while [ $# -gt 0 ]; do
	case "$1" in
	--version)
		[ $# -ge 2 ] || fail "--version requires a value"
		VERSION="$2"
		shift 2
		;;
	--version=*)
		VERSION="${1#--version=}"
		shift
		;;
	--dir)
		[ $# -ge 2 ] || fail "--dir requires a value"
		INSTALL_DIR="$2"
		shift 2
		;;
	--dir=*)
		INSTALL_DIR="${1#--dir=}"
		shift
		;;
	--force)
		FORCE=1
		shift
		;;
	--no-path)
		SHOW_PATH_HINT=0
		shift
		;;
	-h | --help)
		usage
		exit 0
		;;
	*)
		fail "unknown argument: $1"
		;;
	esac
done

# ---------------------------------------------------------------- host mapping
# Triples and aliases below are contract projections; keep them in sync via
# scripts/check-installer-contract.py.
os=$(uname -s 2>/dev/null || echo unknown)
arch=$(uname -m 2>/dev/null || echo unknown)

case "$os" in
Linux) os_family=linux ;;
Darwin) os_family=macos ;;
*) fail "unsupported operating system: $os. Build from source with 'cargo install $PRODUCT --locked'." ;;
esac

case "$arch" in
x86_64 | amd64) arch_family=x64 ;;
aarch64 | arm64) arch_family=arm64 ;;
armv7l | armv7 | armhf) arch_family=armv7 ;;
*) fail "unsupported architecture: $arch. Build from source with 'cargo install $PRODUCT --locked'." ;;
esac

# The published target set. armv7 has no prebuilt binary and is Cargo-only.
case "$os_family:$arch_family" in
linux:x64) TARGET="x86_64-unknown-linux-gnu" ;;
linux:arm64) TARGET="aarch64-unknown-linux-gnu" ;;
macos:x64) TARGET="x86_64-apple-darwin" ;;
macos:arm64) TARGET="aarch64-apple-darwin" ;;
windows:x64) TARGET="x86_64-pc-windows-msvc" ;;
linux:armv7) TARGET="" ;; # recognized, no prebuilt binary
*) TARGET="" ;;
esac

CARGO_FALLBACK_HOST=0
if [ -z "$TARGET" ]; then
	CARGO_FALLBACK_HOST=1
	info "$PRODUCT: $os_family/$arch_family has no prebuilt binary; using the Cargo source fallback."
fi

# ------------------------------------------------------------- destination
if [ -z "$INSTALL_DIR" ]; then
	if [ "$(id -u 2>/dev/null || echo 1000)" = "0" ]; then
		INSTALL_DIR="/usr/local/bin"
	else
		INSTALL_DIR="${HOME:-/tmp}/.local/bin"
	fi
fi

if [ "$os_family" = "windows" ]; then
	BINARY_NAME="$PRODUCT.exe"
else
	BINARY_NAME="$PRODUCT"
fi

DEST="$INSTALL_DIR/$BINARY_NAME"

if [ -e "$DEST" ] && [ "$FORCE" -ne 1 ]; then
	fail "$DEST already exists. Re-run with --force to replace it."
fi

# ------------------------------------------------------------- dependencies
need_curl=0
command -v curl >/dev/null 2>&1 || need_curl=1
command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 || need_curl=1

if [ "$CARGO_FALLBACK_HOST" -eq 0 ] && [ "$need_curl" -eq 1 ]; then
	fail "curl and a SHA-256 utility (sha256sum or shasum) are required to install a prebuilt $PRODUCT."
fi

# ------------------------------------------------------------------- helpers
sha256_of() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d' ' -f1
	elif command -v shasum >/dev/null 2>&1; then
		shasum -a 256 "$1" | cut -d' ' -f1
	else
		fail "no SHA-256 utility available (need sha256sum or shasum)."
	fi
}

# Download to a file. Any transport, TLS, or timeout failure is a hard error.
# Curl configuration and option injection from the environment are disabled.
#
# The protocol allowlist is derived from the URL check above, so the test-only
# insecure override stays internally consistent instead of being rejected by a
# stricter --proto than the scheme check that just permitted it.
#
# Prints the HTTP status code on stdout. A connection-level failure prints 000
# and is fatal; only 404 is ever treated as absence.
fetch_to() {
	_url="$1"
	_out="$2"
	_proto='=https'
	case "$_url" in
	https://*) ;;
	http://*)
		if [ "${CARGO_CLEANME_INSTALL_ALLOW_INSECURE:-0}" != "1" ]; then
			fail "refusing a non-HTTPS release URL: $_url"
		fi
		_proto='=https,http'
		;;
	*)
		fail "refusing an unrecognized release URL scheme: $_url"
		;;
	esac

	# `--fail` alone cannot distinguish 404 from 503: both exit 22. Ask curl
	# for the status and classify here, so a server fault can never be
	# mistaken for a genuinely absent asset and turned into a Cargo fallback.
	curl --silent --show-error --location \
		--proto "$_proto" --proto-redir "$_proto" \
		--tlsv1.2 \
		--connect-timeout 15 --max-time 300 \
		--retry 0 \
		--user-agent "$USER_AGENT" \
		--disable \
		--output "$_out" \
		--write-out '%{http_code}' \
		-- "$_url" 2>/dev/null || printf '000'
}

is_version() {
	case "$1" in
	[0-9]*.[0-9]*.[0-9]*) printf '%s' "$1" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' ;;
	*) return 1 ;;
	esac
}

# Resolve the version string printed by the candidate itself.
candidate_version() {
	"$1" --version 2>/dev/null | head -n 1 | cut -d' ' -f2
}

# ------------------------------------------------------------------- install
TMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t "$PRODUCT")
chmod 700 "$TMP_DIR" 2>/dev/null || true

STAGED="$TMP_DIR/$BINARY_NAME"
INSTALL_OK=0

install_binary() {
	_resolved_version="$1"

	ASSET="$PRODUCT-$TARGET"
	if [ "$os_family" = "windows" ]; then
		ASSET="$ASSET.exe"
	fi
	SIDECAR="$ASSET.sha256"

	if [ -z "$VERSION" ]; then
		BIN_URL="$LATEST_URL/$ASSET"
		SUM_URL="$LATEST_URL/$SIDECAR"
	else
		BIN_URL="$BASE_URL/v$VERSION/$ASSET"
		SUM_URL="$BASE_URL/v$VERSION/$SIDECAR"
	fi

	info "$PRODUCT: downloading $ASSET for $TARGET..."

	BIN_STATUS=$(fetch_to "$BIN_URL" "$STAGED")
	case "$BIN_STATUS" in
	404)
		# Genuine absence: the one condition that may enter the Cargo source
		# fallback. Anything else is a transport or server fault and must stay
		# fatal, so a release host outage can never silently become a local
		# build.
		printf '%s: release artifact not found: %s\n' "$PRODUCT" "$BIN_URL" >&2
		return 2
		;;
	2??)
		: # success
		;;
	000)
		printf '%s: could not reach %s (transport failure)\n' "$PRODUCT" "$BIN_URL" >&2
		return 1
		;;
	*)
		printf '%s: release host returned HTTP %s for %s\n' "$PRODUCT" "$BIN_STATUS" "$BIN_URL" >&2
		return 1
		;;
	esac

	if [ ! -s "$STAGED" ]; then
		printf '%s: downloaded artifact is empty: %s\n' "$PRODUCT" "$BIN_URL" >&2
		return 1
	fi

	# The digest sidecar is mandatory evidence. Its absence is a hard
	# failure, not a fallback signal.
	SUM_STATUS=$(fetch_to "$SUM_URL" "$TMP_DIR/$SIDECAR")
	case "$SUM_STATUS" in
	2??) : ;;
	*)
		printf '%s: release checksum unavailable (HTTP %s): %s\n' "$PRODUCT" "$SUM_STATUS" "$SUM_URL" >&2
		return 1
		;;
	esac

	EXPECTED=$(head -n 1 "$TMP_DIR/$SIDECAR" | cut -d' ' -f1)
	if ! printf '%s' "$EXPECTED" | grep -Eq '^[0-9a-f]{64}$'; then
		printf '%s: malformed checksum file: %s\n' "$PRODUCT" "$SUM_URL" >&2
		return 1
	fi

	ACTUAL=$(sha256_of "$STAGED")
	if [ "$ACTUAL" != "$EXPECTED" ]; then
		printf '%s: checksum mismatch for %s\n' "$PRODUCT" "$ASSET" >&2
		printf '  expected %s\n  actual   %s\n' "$EXPECTED" "$ACTUAL" >&2
		return 1
	fi

	chmod 755 "$STAGED" 2>/dev/null || true

	OBSERVED=$(candidate_version "$STAGED" || true)
	if [ -z "$OBSERVED" ]; then
		printf '%s: the downloaded candidate did not report a version\n' "$PRODUCT" >&2
		return 1
	fi
	if ! is_version "$OBSERVED"; then
		printf '%s: the downloaded candidate reported a non-release version: %s\n' "$PRODUCT" "$OBSERVED" >&2
		return 1
	fi
	if [ -n "$VERSION" ] && [ "$OBSERVED" != "$VERSION" ]; then
		printf '%s: requested %s but the candidate reports %s\n' "$PRODUCT" "$VERSION" "$OBSERVED" >&2
		return 1
	fi
	# Only a pinned request has a pre-resolved expectation to agree with. For a
	# latest install the candidate's own report *is* the version authority, so
	# there is nothing to compare it against.
	if [ -n "$_resolved_version" ] && [ "$OBSERVED" != "$_resolved_version" ]; then
		printf '%s: version authority disagreement (%s vs %s)\n' "$PRODUCT" "$_resolved_version" "$OBSERVED" >&2
		return 1
	fi

	mkdir -p "$INSTALL_DIR" 2>/dev/null || true
	if [ ! -d "$INSTALL_DIR" ]; then
		printf '%s: install directory does not exist: %s\n' "$PRODUCT" "$INSTALL_DIR" >&2
		return 1
	fi
	if [ ! -w "$INSTALL_DIR" ]; then
		printf '%s: install directory is not writable: %s\n' "$PRODUCT" "$INSTALL_DIR" >&2
		printf '  re-run this installer yourself as root for a system-wide install; it never escalates privileges for you.\n' >&2
		return 1
	fi

	# Copy then verify the placed bytes, so a partial write cannot masquerade
	# as a successful install.
	cp "$STAGED" "$TMP_DIR/.place" || return 1
	mv -f "$TMP_DIR/.place" "$DEST" || {
		printf '%s: could not place %s\n' "$PRODUCT" "$DEST" >&2
		return 1
	}
	chmod 755 "$DEST" 2>/dev/null || true

	PLACED=$(sha256_of "$DEST")
	if [ "$PLACED" != "$ACTUAL" ]; then
		rm -f "$DEST"
		printf '%s: installed bytes do not match the verified candidate\n' "$PRODUCT" >&2
		return 1
	fi

	INSTALL_OK=1
	return 0
}

install_cargo() {
	_resolved_version="$1"

	command -v cargo >/dev/null 2>&1 || {
		printf '%s: cargo is required for the source fallback but was not found\n' "$PRODUCT" >&2
		return 1
	}

	# Build into an invocation-owned private root so a failed build never
	# touches the destination or the user's Cargo installation state.
	CARGO_ROOT="$TMP_DIR/cargo-root"
	mkdir -p "$CARGO_ROOT" || return 1

	if [ -n "$VERSION" ]; then
		info "$PRODUCT: building $PRODUCT $VERSION from source with Cargo..."
		cargo install "$PRODUCT" --locked --version "=$VERSION" --root "$CARGO_ROOT" >&2 || return 1
	else
		info "$PRODUCT: building $PRODUCT from source with Cargo..."
		cargo install "$PRODUCT" --locked --root "$CARGO_ROOT" >&2 || return 1
	fi

	BUILT="$CARGO_ROOT/bin/$BINARY_NAME"
	if [ ! -s "$BUILT" ]; then
		printf '%s: Cargo reported success but %s was not produced\n' "$PRODUCT" "$BUILT" >&2
		return 1
	fi

	OBSERVED=$(candidate_version "$BUILT" || true)
	if ! is_version "$OBSERVED"; then
		printf '%s: the Cargo-built candidate reported an unusable version: %s\n' "$PRODUCT" "$OBSERVED" >&2
		return 1
	fi
	if [ -n "$VERSION" ] && [ "$OBSERVED" != "$VERSION" ]; then
		printf '%s: requested %s but the Cargo build produced %s\n' "$PRODUCT" "$VERSION" "$OBSERVED" >&2
		return 1
	fi

	mkdir -p "$INSTALL_DIR" 2>/dev/null || true
	if [ ! -d "$INSTALL_DIR" ] || [ ! -w "$INSTALL_DIR" ]; then
		printf '%s: install directory is not writable: %s\n' "$PRODUCT" "$INSTALL_DIR" >&2
		printf '  re-run this installer yourself as root for a system-wide install; it never escalates privileges for you.\n' >&2
		return 1
	fi

	cp "$BUILT" "$TMP_DIR/.place" || return 1
	mv -f "$TMP_DIR/.place" "$DEST" || {
		printf '%s: could not place %s\n' "$PRODUCT" "$DEST" >&2
		return 1
	}
	chmod 755 "$DEST" 2>/dev/null || true

	INSTALL_OK=1
	return 0
}

# ---------------------------------------------------------------- version pick
if [ -n "$VERSION" ] && ! is_version "$VERSION"; then
	fail "--version must be an exact X.Y.Z release, got: $VERSION"
fi

# The version authority is the Cargo-built/staged candidate's own report; the
# published tag selects the asset, and the candidate must agree with it.
EXPECTED_VERSION="$VERSION"

if [ "$CARGO_FALLBACK_HOST" -eq 1 ]; then
	install_cargo "$EXPECTED_VERSION" || fail "installation failed."
else
	set +e
	install_binary "$EXPECTED_VERSION"
	STATUS=$?
	set -e

	case "$STATUS" in
	0) : ;;
	2)
		# The selected binary is genuinely absent. Cargo is the only
		# documented reason to fall back, and it is never used for checksum,
		# manifest, identity, or transport failures.
		info "$PRODUCT: no published binary for this release; using the Cargo source fallback."
		install_cargo "$EXPECTED_VERSION" || fail "installation failed."
		;;
	*) fail "installation failed; refusing to fall back to Cargo after a verification or transport failure." ;;
	esac
fi

FINAL=$(candidate_version "$DEST" || true)

info ""
info "$PRODUCT $FINAL installed to $DEST"

if [ "$SHOW_PATH_HINT" -eq 1 ]; then
	case ":${PATH:-}:" in
	*":$INSTALL_DIR:"*) ;;
	*)
		info ""
		info "$INSTALL_DIR is not on your PATH. Add it with:"
		info "  export PATH=\"$INSTALL_DIR:\$PATH\""
		;;
	esac
fi

exit 0
