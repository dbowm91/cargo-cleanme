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
#   install.sh                        # latest stable release
#   install.sh --version 0.1.0        # exact release
#   install.sh --dir <path>           # override the install directory
#   install.sh --no-shell-profile     # do not edit a shell startup file
#   install.sh --no-path              # do not touch PATH at all
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
INSTALL_DIR_EXPLICIT=0
SHOW_PATH_HINT=1
PERSIST_SHELL_PROFILE=1
FORCE=0
IS_ROOT=0

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
  install.sh [--version X.Y.Z] [--dir PATH] [--force]
             [--no-shell-profile] [--no-path] [--help]

Options:
  --version X.Y.Z    Install an exact release instead of the latest stable one.
  --dir PATH         Install into PATH instead of the default location.
  --force            Replace an existing cargo-cleanme in the install directory.
  --no-shell-profile Install the binary but do not add the directory to a shell
                     startup file. PATH guidance is still printed.
  --no-path          Do not print PATH guidance and do not modify any shell
                     startup file. This is the broad opt-out.
  --help             Show this message.

Default install directory:
  non-root   \$HOME/.local/bin
  root       /usr/local/bin

Shell PATH integration (POSIX only):
  A normal, non-root install into the default \$HOME/.local/bin adds a small
  guarded block to your zsh or bash startup file so future shells can run
  $PRODUCT. The block is idempotent, is appended only after the verified binary
  is in place, and is never added for a --dir destination, a root/system
  install, an unsupported shell, or an unsafe profile target. The installer
  cannot change the PATH of the shell that invoked it, so it also prints the
  export line for the current shell.
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
		INSTALL_DIR_EXPLICIT=1
		shift 2
		;;
	--dir=*)
		INSTALL_DIR="${1#--dir=}"
		INSTALL_DIR_EXPLICIT=1
		shift
		;;
	--force)
		FORCE=1
		shift
		;;
	--no-shell-profile)
		PERSIST_SHELL_PROFILE=0
		shift
		;;
	--no-path)
		# The broad opt-out: no PATH output *and* no startup-file mutation, so
		# an existing scripted caller that already opted out never acquires a
		# new side effect after upgrading.
		SHOW_PATH_HINT=0
		PERSIST_SHELL_PROFILE=0
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
if [ "$(id -u 2>/dev/null || echo 1000)" = "0" ]; then
	IS_ROOT=1
fi

if [ -z "$INSTALL_DIR" ]; then
	if [ "$IS_ROOT" -eq 1 ]; then
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

# ------------------------------------------------------------ PATH handling
# The installer cannot change the PATH of the shell that invoked it: a piped
# child process cannot mutate its already-running parent. So the export line is
# always printed for the current shell, and separately a small guarded block is
# appended to one supported startup file so *future* shells work without the
# user editing anything by hand.
#
# Every step below is deliberately conservative. The profile is only touched for
# the canonical non-root user-local destination, the file is never sourced,
# evaluated, or command-substituted, a symlink is never followed, and a failure
# to write the profile is reported rather than rolled back into the binary.
#
# Two environment overrides exist purely so a deterministic fixture on one host
# can exercise the *other* platform's PATH-integration policy:
#
#   CARGO_CLEANME_INSTALL_PATH_PROFILE_OS   linux|macos -- which startup file
#                                           bash integration targets.
#   CARGO_CLEANME_INSTALL_SYSTEM_SCOPE      1 -- refuse profile persistence, as
#                                           a root/system install must.
#
# Both are PATH-integration policy only. Neither can influence transport, the
# digest check, candidate identity, or placement: those run before any of this
# and read no override.

CANONICAL_USER_DIR="${HOME:-}/.local/bin"
PATH_PROFILE_OS="${CARGO_CLEANME_INSTALL_PATH_PROFILE_OS:-$os_family}"
SYSTEM_SCOPE="$IS_ROOT"
if [ "${CARGO_CLEANME_INSTALL_SYSTEM_SCOPE:-0}" = "1" ]; then
	SYSTEM_SCOPE=1
fi
PROFILE_BEGIN="# >>> cargo-cleanme installer PATH >>>"
PROFILE_END="# <<< cargo-cleanme installer PATH <<<"

# The literal directory a guarded profile entry should compare against. The
# canonical spelling is the literal path, not `$HOME`, so the entry stays valid
# and readable when a later shell sources it.
canonical_dir_literal() {
	printf '%s' "$CANONICAL_USER_DIR"
}

path_has_dir() {
	case ":${PATH:-}:" in
	*":$1:"*) return 0 ;;
	esac
	return 1
}

# Which startup file this invocation may use, or nothing at all.
#
# Only zsh and bash are supported. Anything else -- fish, nushell, elvish, an
# unset SHELL, or a path with no recognisable basename -- yields no target, and
# the caller falls back to manual guidance rather than guessing syntax or
# startup-file precedence we have not tested.
shell_profile_target() {
	_shell_name=$(basename "${SHELL:-}" 2>/dev/null || printf '')
	case "$_shell_name" in
	zsh)
		# A zsh user may have relocated their dotfiles. A valid ZDOTDIR is an
		# absolute path with no newline in it; anything else falls back to HOME.
		case "${ZDOTDIR:-}" in
		/*)
			case "$ZDOTDIR" in
			*"
"*) ;;
			*) printf '%s/.zshrc' "$ZDOTDIR"; return 0 ;;
			esac
			;;
		esac
		printf '%s/.zshrc' "$HOME"
		return 0
		;;
	bash)
		# bash reads .bashrc for interactive non-login shells everywhere, but a
		# login shell reads the first of .bash_profile, .bash_login, .profile.
		# On macOS `Terminal` starts login shells, so following that precedence
		# is what makes the entry actually take effect; elsewhere .bashrc is the
		# file a terminal shell reads.
		if [ "$PATH_PROFILE_OS" = "macos" ]; then
			for _candidate in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
				if [ -f "$_candidate" ]; then
					printf '%s' "$_candidate"
					return 0
				fi
			done
			printf '%s/.bash_profile' "$HOME"
			return 0
		fi
		printf '%s/.bashrc' "$HOME"
		return 0
		;;
	esac
	return 1
}

# Is `$2` an acceptable profile file to append to?
#
# Rules, all of them about not touching something that is not ours to touch:
#   * the file must not be a symlink -- following one could redirect the append
#     into an arbitrary location, and a dotfile symlink is common enough that
#     silently obeying it would be a surprise;
#   * an existing target must be a regular file we can write;
#   * a missing target may be created only in an existing, writable directory
#     that we expect to be the user's own.
profile_target_is_safe() {
	_target="$1"
	_parent=$(dirname "$_target" 2>/dev/null || printf '')

	if [ -L "$_target" ]; then
		return 1
	fi
	if [ -e "$_target" ]; then
		[ -f "$_target" ] || return 1
		[ -w "$_target" ] || return 1
		return 0
	fi

	# Creating a file: only inside a directory that already exists, is a real
	# directory, and is writable. `unignore` and friends are irrelevant here;
	# the point is that the installer never creates a directory tree to drop a
	# dotfile into.
	[ -n "$_parent" ] || return 1
	[ -d "$_parent" ] || return 1
	[ ! -L "$_parent" ] || return 1
	[ -w "$_parent" ] || return 1
	return 0
}

# Does the profile already integrate the canonical directory for real?
#
# "For real" is the whole point. A commented-out line, an `echo`, a mention in
# prose, or an unrelated variable that happens to contain `.local/bin` is not an
# active integration, and treating one as such would silently skip the append
# and leave the user with a broken PATH and no warning. Only an uncommented
# `PATH=` assignment or `PATH` mutation naming the directory counts, plus the
# installer's own previously-written managed block.
profile_has_active_entry() {
	_target="$1"
	_dir="$2"
	[ -f "$_target" ] || return 1

	# Our own managed block is an unambiguous prior integration.
	if grep -qF "$PROFILE_BEGIN" "$_target" 2>/dev/null; then
		return 0
	fi

	# An active line is one that is not a comment and that assigns or extends
	# PATH with the canonical directory. `#` anywhere earlier in the line means
	# commented out, which is exactly the case that must not suppress us.
	grep -v '^[[:space:]]*#' "$_target" 2>/dev/null |
		grep -E "^[[:space:]]*(export[[:space:]]+)?PATH[[:space:]]*(\+?=)" |
		grep -qF "$_dir"
}

# Append the guarded entry. Idempotent: a repeat install finds the managed block
# and writes nothing.
#
# A write failure here must leave the already-verified binary alone, so the
# signal POSIX raises for the file-size-limit case is ignored for the duration
# of the write and the resulting write error is returned as an ordinary status
# for the caller to report.
append_profile_entry() {
	_target="$1"
	_dir="$2"

	trap '' XFSZ 2>/dev/null || true
	{
		printf '\n%s\n' "$PROFILE_BEGIN"
		printf '# added by the cargo-cleanme installer; safe to delete\n'
		printf 'case ":$PATH:" in\n'
		printf '  *":%s:"*) ;;\n' "$_dir"
		printf '  *) export PATH="%s:$PATH" ;;\n' "$_dir"
		printf 'esac\n'
		printf '%s\n' "$PROFILE_END"
	} >>"$_target" 2>/dev/null
	_status=$?
	trap - XFSZ 2>/dev/null || trap - SIGXFSZ 2>/dev/null || true
	return "$_status"
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

# ---------------------------------------------------------- PATH integration
# Everything below runs only after the verified binary is in place, and nothing
# here can turn a successful installation into a failure.

print_path_guidance() {
	info ""
	info "$INSTALL_DIR is not on your PATH. Add it with:"
	info "  export PATH=\"$INSTALL_DIR:\$PATH\""
}

# Profile persistence is permitted only for the canonical user-local destination
# of a non-root install with the default directory. A `--dir` value is the
# user's explicit choice and is never written into a startup file; neither is a
# root/system destination, which has no user profile to edit.
profile_persistence_permitted() {
	[ "$PERSIST_SHELL_PROFILE" -eq 1 ] || return 1
	[ "$SHOW_PATH_HINT" -eq 1 ] || return 1
	[ "$INSTALL_DIR_EXPLICIT" -eq 0 ] || return 1
	[ "$SYSTEM_SCOPE" -eq 0 ] || return 1
	[ -n "$HOME" ] || return 1
	[ "$INSTALL_DIR" = "$CANONICAL_USER_DIR" ] || return 1
	return 0
}

integrate_shell_profile() {
	profile_persistence_permitted || return 0

	if path_has_dir "$INSTALL_DIR"; then
		# Already on PATH in this process, so nothing to persist for the next
		# one either: the user's environment already does it.
		return 0
	fi

	_dir=$(canonical_dir_literal)
	_target=$(shell_profile_target) || return 0

	if ! profile_target_is_safe "$_target"; then
		info ""
		info "  could not add $_dir to $_target (it is not a writable regular file)."
		info "  add it yourself if you want it on PATH in future shells."
		return 0
	fi

	if profile_has_active_entry "$_target" "$_dir"; then
		info ""
		info "  $_target already adds $_dir to PATH; leaving it unchanged."
		return 0
	fi

	if append_profile_entry "$_target" "$_dir" 2>/dev/null; then
		info ""
		info "  added $_dir to $_target for future shells."
		return 0
	fi

	# Non-fatal by design: the binary is installed and verified, and undoing
	# that because a dotfile could not be written would be a worse outcome than
	# a PATH the user configures by hand.
	info ""
	info "  could not write $_target; $_dir is not on PATH for future shells."
	info "  add it yourself, or re-run with --dir to choose a directory already on PATH."
	return 0
}

# One gate for "the caller wants PATH handling at all", and one for "this
# invocation may persist a profile". They are deliberately not the same
# condition: --no-path opts out of both, --no-shell-profile out of only the
# second.
if [ "$SHOW_PATH_HINT" -eq 1 ] && ! path_has_dir "$INSTALL_DIR"; then
	integrate_shell_profile
	print_path_guidance
fi

exit 0
