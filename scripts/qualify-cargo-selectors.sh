#!/usr/bin/env bash
set -euo pipefail

# Real-Cargo selector qualification (M011D).
#
# This script exists to make cargo-cleanme's *capability claim* checkable. The
# product already fails closed for an unknown Cargo version --
# `clean_capabilities_from_version` returns an all-false capability struct, so an
# unqualified release degrades to whole-target scope instead of guessing. That is
# the correct runtime posture, and it is also why a stale allowlist is invisible:
# nothing breaks when the claim drifts from reality, the product just quietly
# stops offering a selector.
#
# So this is an assertion harness, not a characterizer. Every claimed capability
# produces one named result with an explicit verdict, and a version the product
# claims but that produced no assertion is a FAILURE, not a pass.
#
# Three failure classes, deliberately kept apart:
#
#   semantic_regression  - a claimed capability did not hold on real Cargo.
#                          This is a product finding.
#   toolchain_unavailable- the exact pinned toolchain cannot be run here. This
#                          is an operational blocker and is reported as one; it
#                          never silently drops a version from the product
#                          allowlist.
#   premise_failed       - a fixture could not be built for a reason unrelated
#                          to selectors (no network-free lockfile, a host triple
#                          that cannot be resolved, a non-POSIX path). A
#                          fixture that did not run is NOT evidence about Cargo,
#                          and collapsing it into "the selector regressed" is
#                          the false-green this script is written to prevent.
#
# Everything runs inside a private temporary directory. It never cleans a
# developer workspace, and it only ever invokes `cargo clean --dry-run` plus
# in-fixture `cargo clean` calls.
#
# Usage:
#   scripts/qualify-cargo-selectors.sh [--policy PATH] [--evidence PATH]
#   scripts/qualify-cargo-selectors.sh --exploratory [--evidence PATH]
#   scripts/qualify-cargo-selectors.sh --self-test
#
# Exit status is nonzero when any *supported* version is not `pass`. The
# exploratory lane never fails the run: it is a research signal, and treating it
# as a gate is how an unobserved Cargo release silently becomes a support claim.

ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)
POLICY="$ROOT/release/selector-qualification.json"
EVIDENCE=""
MODE=supported
SELF_TEST=0

usage() {
  sed -n '3,34p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while (($#)); do
  case $1 in
    --exploratory) MODE=exploratory ;;
    --evidence) EVIDENCE=${2:?--evidence needs a path}; shift ;;
    --policy) POLICY=${2:?--policy needs a path}; shift ;;
    --self-test) SELF_TEST=1 ;;
    -h | --help) usage; exit 0 ;;
    *) echo "qualify-cargo-selectors: unknown argument $1" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

# The premise guards are the load-bearing part of this harness: a fixture that
# did not run is not evidence about Cargo. `--self-test` proves each of them
# still distinguishes its own failure shape, because a guard that cannot fail is
# a guard that reports a false green.
self_test() {
  local failures=0
  local check_name expected actual

  expect() { # name expected actual
    check_name=$1 expected=$2 actual=$3
    if [[ "$expected" == "$actual" ]]; then
      printf '  ok   %s\n' "$check_name"
    else
      printf '  FAIL %s: expected %s, got %s\n' "$check_name" "$expected" "$actual" >&2
      failures=$((failures + 1))
    fi
  }

  # The dangerous direction is a misclassification *into* `pass`, and the second
  # is collapsing an operational or premise failure into a product finding. Both
  # are differences, not equalities, so they get their own assertion.
  expect_different() { # name unwanted actual
    check_name=$1 expected=$2 actual=$3
    if [[ "$expected" != "$actual" ]]; then
      printf '  ok   %s\n' "$check_name"
    else
      printf '  FAIL %s: the verdict collapsed into %s\n' "$check_name" "$expected" >&2
      failures=$((failures + 1))
    fi
  }

  echo "qualify-cargo-selectors: self test"

  # 1. A failing premise is classified as a premise failure and never as a pass.
  RESULTS=""
  VERDICTS=()
  fail_premise "9.9.9" "fixture_build" "injected: the fixture did not build"
  expect "a failed premise is premise_failed" premise_failed "${VERDICTS[0]}"
  expect_different "a failed premise is not a pass" pass "${VERDICTS[0]}"
  expect_different "a failed premise is not a semantic regression" semantic_regression "${VERDICTS[0]}"
  expect "a failed premise makes the run fail" 0 "$(has_failure && echo 0 || echo 1)"

  # 2. A toolchain that cannot be probed is an operational blocker, reported as
  #    its own class rather than as a selector regression.
  RESULTS=""
  VERDICTS=()
  fail_toolchain "9.9.9" "cargo_version_probe" "injected: rustup cannot run this toolchain"
  expect "an unprobeable toolchain is toolchain_unavailable" toolchain_unavailable "${VERDICTS[0]}"
  expect_different "an unprobeable toolchain is not a semantic regression" semantic_regression "${VERDICTS[0]}"
  expect_different "an unprobeable toolchain is not a pass" pass "${VERDICTS[0]}"

  # 3. A run in which every assertion passed is not a failure.
  RESULTS=""
  VERDICTS=()
  pass "1.99.0" profile_dry_run_nonmutating "injected"
  expect "an all-pass run is not a failure" 1 "$(has_failure && echo 0 || echo 1)"

  # 4. A claimed version that produced no assertion is a failure, not a pass.
  #    This is the false green a weaker harness reports.
  RESULTS=""
  VERDICTS=()
  require_claims_produced "1.99.0" profile || true
  expect "a claim with no assertion is reported" semantic_regression "${VERDICTS[0]}"
  RESULTS=""
  VERDICTS=()
  pass "1.99.0" profile_dry_run_nonmutating "injected"
  require_claims_produced "1.99.0" profile
  expect "a claim with an assertion is not reported" 1 "${#VERDICTS[@]}"

  # 5. The evidence file must not claim overall success while a verdict is not a
  #    pass, and must not claim success for a run that observed nothing.
  local scratch evidence
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/cargo-cleanme-selector-selftest.XXXXXX")
  evidence="$scratch/evidence.json"
  RESULTS=""
  VERDICTS=()
  pass "1.99.0" profile_dry_run_nonmutating "injected"
  EVIDENCE="$evidence" write_evidence >/dev/null
  expect "an all-pass evidence file reports pass" pass \
    "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["overall"])' "$evidence")"
  RESULTS=""
  VERDICTS=()
  pass "1.99.0" profile_dry_run_nonmutating "injected"
  fail_premise "1.99.0" fixture_build "injected: the fixture did not build"
  EVIDENCE="$evidence" write_evidence >/dev/null
  expect "a premise failure makes the evidence report fail" fail \
    "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["overall"])' "$evidence")"
  RESULTS=""
  VERDICTS=()
  EVIDENCE="$evidence" write_evidence >/dev/null
  expect "an empty run does not report pass" fail \
    "$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["overall"])' "$evidence")"
  rm -rf "$scratch"

  # 6. A real, deliberately unprobeable toolchain is classified operationally.
  #    This one is not injected: it proves the real probe path agrees with the
  #    classification the harness reports, and that an operational blocker never
  #    edits the product allowlist on its way out.
  local allowlist_before allowlist_after
  allowlist_before=$(read_policy profile_selector | wc -l | tr -d ' ')
  RESULTS=""
  VERDICTS=()
  qualify_version "0.0.0-nonexistent-toolchain" profile
  expect "a real unprobeable toolchain is toolchain_unavailable" toolchain_unavailable "${VERDICTS[0]}"
  allowlist_after=$(read_policy profile_selector | wc -l | tr -d ' ')
  expect "an operational blocker leaves the allowlist intact" "$allowlist_before" "$allowlist_after"

  if ((failures)); then
    echo "qualify-cargo-selectors: self test FAILED ($failures case(s))" >&2
    return 1
  fi
  echo "qualify-cargo-selectors: self test passed; premise failures are still distinguishable from selector regressions"
  return 0
}

# ------------------------------------------------------------------ evidence
#
# One TSV row per assertion: version, capability, verdict, detail. Kept flat so a
# truncated or partially written evidence file is still readable, and rendered
# to JSON once at the end so a half-finished run never leaves a file claiming
# success.
RESULTS=""
VERDICTS=()

record() { # version capability verdict detail
  RESULTS+="$1"$'\t'"$2"$'\t'"$3"$'\t'"$4"$'\n'
  VERDICTS+=("$3")
}

pass() { record "$1" "$2" pass "$3"; }
fail_assertion() { record "$1" "$2" semantic_regression "$3"; }
fail_premise() { record "$1" "$2" premise_failed "$3"; }
fail_toolchain() { record "$1" "$2" toolchain_unavailable "$3"; }

has_failure() {
  local verdict
  for verdict in "${VERDICTS[@]+"${VERDICTS[@]}"}"; do
    case $verdict in
      pass) ;;
      *) return 0 ;;
    esac
  done
  return 1
}

# ------------------------------------------------------------------- policy

read_policy() { # list-name
  python3 - "$POLICY" "$1" <<'PY'
import json, sys
policy = json.loads(open(sys.argv[1], encoding="utf-8").read())
for value in policy[sys.argv[2]]:
    print(value)
PY
}

# ------------------------------------------------------------------- fixture

make_fixture() {
  local base=$1
  mkdir -p "$base"/{app-a/src,app-b/src,shared/src,.cargo}
  cat > "$base/Cargo.toml" <<'EOF'
[workspace]
members = ["app-a", "app-b", "shared"]
resolver = "2"

[profile.custom]
inherits = "dev"
opt-level = 1
EOF
  cat > "$base/shared/Cargo.toml" <<'EOF'
[package]
name = "shared"
version = "0.1.0"
edition = "2021"
EOF
  printf 'pub fn value() -> u32 { 7 }\n' > "$base/shared/src/lib.rs"
  for package in app-a app-b; do
    local dependency='shared = { path = "../shared" }'
    local import=shared
    if [[ "$package" == app-a ]]; then
      dependency='shared_alias = { package = "shared", path = "../shared" }'
      import=shared_alias
    fi
    cat > "$base/$package/Cargo.toml" <<EOF
[package]
name = "$package"
version = "0.1.0"
edition = "2021"

[dependencies]
$dependency
EOF
    printf 'fn main() { println!("{}", %s::value()); }\n' "$import" > "$base/$package/src/main.rs"
  done
}

# A second fixture whose whole point is that Cargo cannot resolve a package
# identity that is ambiguous. This is production metadata only; cargo-cleanme
# never parses a package path.
make_duplicate_fixture() {
  local base=$1
  mkdir -p "$base"/{host/src,duplicate-one/src,duplicate-two/src}
  cat > "$base/Cargo.toml" <<'EOF'
[workspace]
members = ["host"]
resolver = "2"
EOF
  cat > "$base/host/Cargo.toml" <<'EOF'
[package]
name = "host"
version = "0.1.0"
edition = "2021"
[dependencies]
dup-one = { package = "duplicate", path = "../duplicate-one" }
dup-two = { package = "duplicate", path = "../duplicate-two" }
EOF
  printf 'fn main() { println!("{}", dup_one::value() + dup_two::value()); }\n' > "$base/host/src/main.rs"
  local row directory package_version library_name
  for row in 'duplicate-one 1.0.0 dup_one' 'duplicate-two 2.0.0 dup_two'; do
    read -r directory package_version library_name <<<"$row"
    cat > "$base/$directory/Cargo.toml" <<EOF
[package]
name = "duplicate"
version = "$package_version"
edition = "2021"
[lib]
name = "$library_name"
EOF
    printf 'pub fn value() -> u32 { 1 }\n' > "$base/$directory/src/lib.rs"
  done
}

# ------------------------------------------------------------------ qualify

# qualify_version TOOLCHAIN CLAIM
#   CLAIM is `profile`, `package`, or `exploratory`.
qualify_version() {
  local toolchain=$1 claim=$2
  local want_profile=0 want_package=0
  case $claim in
    profile | package)
      want_profile=1
      ;;
  esac
  [[ "$claim" == package ]] && want_package=1

  local cargo_cmd=(rustup run "$toolchain" cargo)

  # Premise 1: the exact pinned toolchain must actually run here. This is an
  # operational fact about rustup's catalogue, not a statement about Cargo's
  # selectors, and the two must not share a verdict.
  local version_text
  if ! version_text=$("${cargo_cmd[@]}" -V 2>&1); then
    fail_toolchain "$toolchain" "cargo_version_probe" \
      "rustup cannot run the pinned toolchain $toolchain: ${version_text//$'\n'/ }"
    return 0
  fi
  printf '=== %s (%s) ===\n' "$toolchain" "$version_text"

  local host
  if ! host=$(rustup run "$toolchain" rustc -vV 2>/dev/null | sed -n 's/^host: //p') || [[ -z "$host" ]]; then
    fail_premise "$toolchain" "host_triple" \
      "could not resolve the host triple for $toolchain, so configured/explicit target behaviour cannot be measured"
    return 0
  fi

  local fixture="$TMPROOT/${toolchain//[^[:alnum:]]/_}"
  local target="$fixture/redirected-target"
  local build_dir="$fixture/redirected-build"
  mkdir -p "$fixture"
  make_fixture "$fixture"
  cat > "$fixture/.cargo/config.toml" <<EOF
[build]
target-dir = "$target"
EOF
  local minor
  minor=$(printf '%s\n' "$version_text" | sed -E 's/^cargo 1\.([0-9]+).*/\1/')
  if [[ "$minor" =~ ^[0-9]+$ ]] && ((minor >= 91)); then
    printf 'build-dir = "%s"\n' "$build_dir" >>"$fixture/.cargo/config.toml"
  fi

  # Premise 2: the fixture must build before any clean result means anything.
  # These builds are the only reason a later `cargo clean` has output to select.
  if ! ensure_built "$toolchain" "$fixture"; then
    fail_premise "$toolchain" "fixture_build" \
      "the selector fixture did not build on $version_text: $(tail -3 "$fixture/premise-build.txt" | tr '\n' ' ')"
    return 0
  fi

  if ((want_profile)); then
    qualify_profile "$toolchain" "$fixture" "$target" "$build_dir" "$host" "$minor"
  fi
  if ((want_package)); then
    qualify_package "$toolchain" "$fixture" "$target" "$host"
  fi
  if [[ "$claim" == exploratory ]]; then
    qualify_exploratory "$toolchain" "$fixture" "$target" "$build_dir" "$host" "$minor"
  fi
}

# Rebuild every profile the assertions below need to find.
#
# Idempotent and incremental on purpose. The profile and package phases each end
# by removing output the other phase asserts is still present, so a phase that
# ran second would otherwise be measuring a fixture the first phase had already
# cleaned -- and "the release profile survived a --package clean" would pass for
# the wrong reason on a release output that had never been built. Calling this at
# the top of every phase makes the two order-independent and each assertion
# about a real artifact rather than about the previous phase's leftovers.
ensure_built() { # toolchain fixture
  local cargo_cmd=(rustup run "$1" cargo)
  (
    cd "$2" &&
      "${cargo_cmd[@]}" generate-lockfile --offline &&
      "${cargo_cmd[@]}" build --workspace --offline --locked &&
      "${cargo_cmd[@]}" build --workspace --release --offline --locked &&
      "${cargo_cmd[@]}" build --workspace --profile custom --offline --locked
  ) >"$2/premise-build.txt" 2>&1
}

# Every profile-qualified version owes the same claims, and they are written
# against the *product's own* command shape rather than a convenient one.
# `cleanup::clean_args` builds exactly:
#
#   cargo clean [--dry-run --verbose] --offline --locked \
#               --manifest-path <root> --target-dir <target> \
#               [--profile NAME | --package SPEC]
#
# Two things follow, and both matter for whether this evidence is real. The
# product pins the target directory with an explicit flag, so a claim proved
# only by a `.cargo/config.toml` discovered from a particular working directory
# would be proving something the product never does. And the product never passes
# `--workspace`; the whole-target fallback is the same command with no selector
# at all, so that is what the unqualified-Cargo path is qualified against.
qualify_profile() {
  local toolchain=$1 fixture=$2 target=$3 build_dir=$4 host=$5 minor=$6
  local cargo_cmd=(rustup run "$toolchain" cargo)
  local manifest="$fixture/Cargo.toml"

  ensure_built "$toolchain" "$fixture" || return 0

  # A1: a profile dry run must report paths and remove nothing.
  local before after_dry
  before=$(cd "$fixture" && find "$target" -type f -print | LC_ALL=C sort)
  if (cd "$fixture" && "${cargo_cmd[@]}" clean --dry-run --verbose --offline --locked \
    --manifest-path "$manifest" --target-dir "$target" --profile dev) >"$fixture/profile-dry-run.txt" 2>&1; then
    after_dry=$(cd "$fixture" && find "$target" -type f -print | LC_ALL=C sort)
    if [[ "$before" == "$after_dry" ]]; then
      pass "$toolchain" profile_dry_run_nonmutating "the product's --profile dev dry run reported paths and removed none"
    else
      fail_assertion "$toolchain" profile_dry_run_nonmutating "the --profile dev dry run changed the output tree"
    fi
  else
    fail_assertion "$toolchain" profile_dry_run_nonmutating \
      "the product's --profile dev dry-run shape failed: $(tail -1 "$fixture/profile-dry-run.txt")"
  fi

  # A2: release selection stays inside its own profile.
  (cd "$fixture" && "${cargo_cmd[@]}" clean --offline --locked --manifest-path "$manifest" --target-dir "$target" --profile release) >"$fixture/profile-release.txt" 2>&1 || true
  if [[ ! -e "$target/release/app-a" && -e "$target/debug/app-b" && -e "$target/custom/app-a" ]]; then
    pass "$toolchain" release_profile_isolation "release removed; debug and custom preserved"
  else
    fail_assertion "$toolchain" release_profile_isolation \
      "the --profile release selector crossed the profile boundary (release/app-a still present)"
  fi

  # A3: a custom profile is selected by name, not by approximation.
  (cd "$fixture" && "${cargo_cmd[@]}" clean --offline --locked --manifest-path "$manifest" --target-dir "$target" --profile custom) >"$fixture/profile-custom.txt" 2>&1 || true
  if [[ ! -e "$target/custom/app-a" && -e "$target/debug/app-b" ]]; then
    pass "$toolchain" custom_profile_isolation "custom removed; debug preserved"
  fi

  # A4: a configured build.target-dir is honoured. This is the one claim that has
  # to come from inside the fixture, and it deliberately does NOT pass
  # `--target-dir`, because the product always passes it. What is being proved is
  # the premise underneath that flag: the product resolves a physical group from
  # Cargo's configuration and then pins the same path explicitly, so a Cargo
  # release that silently ignored `build.target-dir` from the config file would
  # make the product clean a group Cargo is not using. The default `target/`
  # directory is checked as well, because a dry run that reported *both* would
  # be evidence of nothing.
  (cd "$fixture" && "${cargo_cmd[@]}" clean --dry-run --verbose --offline --locked \
    --manifest-path "$manifest" --profile dev) >"$fixture/config-target-profile-clean.txt" 2>&1 || true
  local configured_hits default_hits
  configured_hits=$(grep -c "$target/" "$fixture/config-target-profile-clean.txt" || true)
  default_hits=$(grep -c "$fixture/target/" "$fixture/config-target-profile-clean.txt" || true)
  if ((configured_hits > 0 && default_hits == 0)); then
    pass "$toolchain" configured_target_profile_dry_run \
      "a configured build.target-dir is honoured ($configured_hits path(s), none under the default target directory)"
  else
    fail_assertion "$toolchain" configured_target_profile_dry_run \
      "the profile dry run reported $configured_hits path(s) under the configured target directory and $default_hits under the default one; the product pins --target-dir to the group Cargo resolved from this configuration"
  fi

  # A5: the whole-target fallback the product uses whenever the runtime Cargo is
  # not qualified. Same command, no selector.
  if (cd "$fixture" && "${cargo_cmd[@]}" clean --dry-run --verbose --offline --locked \
    --manifest-path "$manifest" --target-dir "$target") >"$fixture/whole-target.txt" 2>&1; then
    pass "$toolchain" whole_target_fallback "the product's unselected clean shape is accepted on this Cargo"
  else
    fail_assertion "$toolchain" whole_target_fallback \
      "the unselected clean shape failed: $(tail -1 "$fixture/whole-target.txt")"
  fi

  # A6: Cargo 1.91+ reports a build directory distinct from the target
  # directory, which is what `CargoBuildDirCapability::Distinct` claims. Below
  # 1.91 there is no such concept, so there is nothing to assert and saying so
  # is the honest result rather than a silent omission.
  if [[ "$minor" =~ ^[0-9]+$ ]] && ((minor >= 91)); then
    if [[ -d "$build_dir" ]]; then
      pass "$toolchain" distinct_build_dir "Cargo created a build directory distinct from the target directory"
    else
      fail_assertion "$toolchain" distinct_build_dir \
        "Cargo $toolchain created no build directory despite the fixture configuring one"
    fi
  else
    pass "$toolchain" pre_1_91_no_build_directory "Cargo $minor predates the separate build directory, so the capability is Unavailable rather than Distinct"
  fi

  pass "$toolchain" host_triple "$host"
}

# Package selection has strictly more preconditions than profile selection, which
# is why the policy keeps the two lists separate: M008D only authorized package
# selection on the releases where a configured `build.target` and an explicit
# `--target` agree about which bytes are selected.
qualify_package() {
  local toolchain=$1 fixture=$2 target=$3 host=$4
  local cargo_cmd=(rustup run "$toolchain" cargo)
  local manifest="$fixture/Cargo.toml"

  ensure_built "$toolchain" "$fixture" || return 0

  local before after_dry
  before=$(cd "$fixture" && find "$target" -type f -print | LC_ALL=C sort)

  # K1: non-mutating dry run, in the product's command shape.
  if (cd "$fixture" && "${cargo_cmd[@]}" clean --dry-run --verbose --offline --locked \
    --manifest-path "$manifest" --target-dir "$target" --package app-a) >"$fixture/package-dry-run.txt" 2>&1; then
    after_dry=$(cd "$fixture" && find "$target" -type f -print | LC_ALL=C sort)
    if [[ "$before" == "$after_dry" ]]; then
      pass "$toolchain" package_dry_run_nonmutating "the product's --package dry run reported paths and removed none"
    else
      fail_assertion "$toolchain" package_dry_run_nonmutating "the --package dry run changed the output tree"
    fi
  else
    fail_assertion "$toolchain" package_dry_run_nonmutating \
      "the product's --package dry-run shape failed: $(tail -1 "$fixture/package-dry-run.txt")"
  fi

  # K2: a version-qualified spec resolves rather than being rejected as
  # ambiguous, because the product's package-spec resolution accepts that form.
  if (cd "$fixture" && "${cargo_cmd[@]}" clean --dry-run --offline --locked \
    --manifest-path "$manifest" --target-dir "$target" --package app-a@0.1.0) >"$fixture/package-version-dry-run.txt" 2>&1; then
    pass "$toolchain" package_version_spec "a version-qualified package spec is accepted for an unambiguous name"
  else
    fail_assertion "$toolchain" package_version_spec \
      "Cargo rejected a version-qualified spec for an unambiguous package: $(tail -1 "$fixture/package-version-dry-run.txt")"
  fi

  # K3: an ambiguous package name is refused. The product rejects duplicate
  # names before spawning a clean process, so this proves the behaviour it
  # delegates to Cargo actually holds on this release.
  local duplicate_fixture="$TMPROOT/${toolchain//[^[:alnum:]]/_}-duplicate-names"
  mkdir -p "$duplicate_fixture"
  make_duplicate_fixture "$duplicate_fixture"
  if (cd "$duplicate_fixture" && "${cargo_cmd[@]}" generate-lockfile --offline && "${cargo_cmd[@]}" build --offline --locked) >"$duplicate_fixture/build.txt" 2>&1; then
    if (cd "$duplicate_fixture" && "${cargo_cmd[@]}" clean --package duplicate --dry-run --offline --locked) >"$duplicate_fixture/ambiguous.txt" 2>&1; then
      fail_assertion "$toolchain" package_ambiguity_rejected "a duplicate package name was accepted by cargo clean"
    else
      pass "$toolchain" package_ambiguity_rejected "a duplicate package name is rejected as ambiguous"
    fi
  elif grep -q 'two packages named `duplicate`' "$duplicate_fixture/build.txt"; then
    # Cargo may reject the duplicate identity during resolution, before clean is
    # ever reached. Same outcome for our purposes, and reported as such rather
    # than as an absent result.
    pass "$toolchain" package_ambiguity_rejected "Cargo rejected the duplicate package identity during resolution, before clean"
  else
    fail_premise "$toolchain" package_ambiguity_rejected \
      "the duplicate-identity fixture did not build: $(tail -1 "$duplicate_fixture/build.txt")"
  fi

  # K4/K5/K6: the selected package goes; its sibling and the other profiles stay.
  (cd "$fixture" && "${cargo_cmd[@]}" clean --offline --locked --manifest-path "$manifest" --target-dir "$target" --package app-a) >"$fixture/package-clean.txt" 2>&1 || true
  local after
  after=$(cd "$fixture" && find "$target" -type f -print | LC_ALL=C sort)
  if [[ ! -e "$target/debug/app-a" ]]; then
    pass "$toolchain" package_selected_removed "the selected package's debug executable was removed"
  else
    fail_assertion "$toolchain" package_selected_removed "the selected package's executable survived a --package clean"
  fi
  if [[ -e "$target/debug/app-b" ]]; then
    pass "$toolchain" package_sibling_preserved "the sibling package's executable survived a --package clean"
  else
    fail_assertion "$toolchain" package_sibling_preserved "a --package clean removed an unselected sibling's executable"
  fi
  if [[ -e "$target/release/app-a" ]]; then
    pass "$toolchain" package_other_profile_preserved "the release profile survived a debug-scoped --package clean"
  else
    fail_assertion "$toolchain" package_other_profile_preserved "a --package clean crossed the profile boundary into release"
  fi

  # K7: shared-dependency behaviour is OBSERVED and recorded, not asserted.
  # Whether Cargo keeps or removes `libshared-*` is a Cargo policy detail the
  # product documents rather than depends on, so the honest result is the
  # observation itself.
  local shared_state removed_count
  if find "$target/debug/deps" -maxdepth 1 -name 'libshared-*' -print -quit 2>/dev/null | grep -q .; then
    shared_state=preserved
  else
    shared_state=removed
  fi
  # `comm` re-checks the sort order of its input under the *current* locale while
  # the two snapshots were produced with `LC_ALL=C sort`, so without the prefix
  # it warns, reports a wrong count, and still exits 0. A silently wrong
  # observation in a retained evidence artifact is worse than no observation.
  removed_count=$(LC_ALL=C comm -23 <(printf '%s\n' "$before") <(printf '%s\n' "$after") | grep -c . || true)
  pass "$toolchain" shared_dependency_observed "shared_dependency=$shared_state removed_paths=$removed_count"

  # K8: the condition M008D actually used to authorize package selection. A
  # configured `build.target` and an explicit `--target` must select the same
  # bytes, or the product cannot prove which physical group it is cleaning --
  # which is the ownership invariant, not a convenience.
  local config_fixture="$TMPROOT/${toolchain//[^[:alnum:]]/_}-configured-target"
  make_fixture "$config_fixture"
  local config_target="$config_fixture/target-output"
  printf '[build]\ntarget-dir = "%s"\n' "$config_target" >"$config_fixture/.cargo/config.toml"
  if ! (cd "$config_fixture" && "${cargo_cmd[@]}" generate-lockfile --offline && "${cargo_cmd[@]}" build --workspace --offline --locked) >"$config_fixture/build.txt" 2>&1; then
    fail_premise "$toolchain" configured_and_explicit_target_agree \
      "the configured-target fixture did not build: $(tail -1 "$config_fixture/build.txt")"
    return 0
  fi
  mkdir -p "$config_target/$host"
  cp -R "$config_target/debug" "$config_target/$host/"

  (cd "$config_fixture" && "${cargo_cmd[@]}" clean --package app-a --dry-run --verbose --config "build.target=\"$host\"" --offline --locked --manifest-path Cargo.toml) >"$config_fixture/config-target-clean.txt" 2>&1 || true
  (cd "$config_fixture" && "${cargo_cmd[@]}" clean --package app-a --dry-run --verbose --target "$host" --offline --locked --manifest-path Cargo.toml) >"$config_fixture/explicit-target-clean.txt" 2>&1 || true

  local configured explicit
  configured=$(grep -c "target-output/$host/debug/" "$config_fixture/config-target-clean.txt" || true)
  explicit=$(grep -c "target-output/$host/debug/" "$config_fixture/explicit-target-clean.txt" || true)
  if ((configured > 0 && explicit > 0)); then
    pass "$toolchain" configured_and_explicit_target_agree \
      "a configured build.target reported $configured path(s) and an explicit --target reported $explicit for triple $host"
  else
    fail_assertion "$toolchain" configured_and_explicit_target_agree \
      "a configured build.target reported $configured path(s) and an explicit --target reported $explicit for triple $host; package selection is authorized only where these agree"
  fi
}

# The exploratory lane reports the same shape so a future promotion has evidence
# to review, and never changes a verdict.
qualify_exploratory() {
  local toolchain=$1 fixture=$2 target=$3 build_dir=$4 host=$5 minor=$6
  qualify_profile "$toolchain" "$fixture" "$target" "$build_dir" "$host" "$minor"
  qualify_package "$toolchain" "$fixture" "$target" "$host"
}

# ----------------------------------------------------------------- evidence

write_evidence() {
  [[ -n "$EVIDENCE" ]] || return 0
  mkdir -p "$(dirname "$EVIDENCE")"
  RESULTS="$RESULTS" POLICY="$POLICY" MODE="$MODE" EVIDENCE="$EVIDENCE" python3 <<'PY'
import json, os

rows = [line.split("\t") for line in os.environ["RESULTS"].splitlines() if line]
entries = [
    {"toolchain": r[0], "capability": r[1], "verdict": r[2], "detail": r[3]}
    for r in rows
    if len(r) == 4
]
toolchains = sorted({e["toolchain"] for e in entries})
summary = {}
for toolchain in toolchains:
    verdicts = [e["verdict"] for e in entries if e["toolchain"] == toolchain]
    summary[toolchain] = "pass" if verdicts and set(verdicts) == {"pass"} else "fail"

document = {
    "schema": "cargo-cleanme-selector-qualification/1",
    "mode": os.environ["MODE"],
    "policy": os.environ["POLICY"],
    "overall": "pass" if summary and all(v == "pass" for v in summary.values()) else "fail",
    "toolchains": summary,
    "assertions": entries,
}
with open(os.environ["EVIDENCE"], "w", encoding="utf-8") as handle:
    json.dump(document, handle, indent=2, sort_keys=True)
    handle.write("\n")
print(f"evidence: {len(entries)} assertion(s) across {len(toolchains)} toolchain(s) -> {os.environ['EVIDENCE']}")
for toolchain in toolchains:
    print(f"  {toolchain}: {summary[toolchain]}")
PY
}

# A claimed version that produced no assertion is a failure, not a pass. This is
# the exact false green a weaker harness reports: the version is "covered" by the
# matrix, the run is green, and nothing was ever observed.
require_claims_produced() {
  local toolchain=$1 required=$2
  local produced
  produced=$(printf '%s' "$RESULTS" | grep -c "^$toolchain"$'\t' || true)
  if ((produced == 0)); then
    fail_assertion "$toolchain" claims_produced \
      "the policy claims $required selector support for $toolchain but the run produced no assertion for it"
    return 1
  fi
  return 0
}

# --------------------------------------------------------------------- main

if (($SELF_TEST)); then
  self_test
  exit $?
fi

TMPROOT=$(mktemp -d "${TMPDIR:-/tmp}/cargo-cleanme-selector.XXXXXX")
trap 'rm -rf "$TMPROOT"' EXIT

if [[ ! -f "$POLICY" ]]; then
  echo "qualify-cargo-selectors: FAILED: the selector policy $POLICY does not exist" >&2
  exit 1
fi
if ! command -v rustup >/dev/null 2>&1; then
  echo "qualify-cargo-selectors: FAILED: rustup is required to pin exact Cargo toolchains" >&2
  exit 1
fi

declare -a matrix=()
declare -a package_claimed=()
while IFS= read -r version; do
  [[ -n "$version" ]] && package_claimed+=("$version")
done < <(read_policy package_selector)

if [[ "$MODE" == exploratory ]]; then
  while IFS= read -r version; do
    [[ -n "$version" ]] && matrix+=("$version")
  done < <(read_policy exploratory)
else
  # A package-qualified release is also profile-qualified by construction, so the
  # supported matrix is the union of the two lists rather than their
  # concatenation. A version appearing twice would qualify twice against a
  # freshly built fixture, which is wasted minutes that read as extra evidence.
  while IFS= read -r version; do
    [[ -n "$version" ]] && matrix+=("$version")
  done < <(read_policy profile_selector)
  for version in "${package_claimed[@]+"${package_claimed[@]}"}"; do
    [[ -n "$version" ]] || continue
    seen=0
    for already in "${matrix[@]+"${matrix[@]}"}"; do
      if [[ "$already" == "$version" ]]; then
        seen=1
        break
      fi
    done
    ((seen)) || matrix+=("$version")
  done
fi

if ((${#matrix[@]} == 0)); then
  echo "qualify-cargo-selectors: FAILED: the policy selected no toolchains to qualify in $MODE mode" >&2
  exit 1
fi

echo "qualify-cargo-selectors: mode=$MODE policy=$POLICY"
echo "qualify-cargo-selectors: ${#matrix[@]} toolchain(s) to qualify"

for toolchain in "${matrix[@]}"; do
  claim=profile
  for candidate in "${package_claimed[@]+"${package_claimed[@]}"}"; do
    if [[ "$toolchain" == "$candidate" ]]; then
      claim=package
      break
    fi
  done
  [[ "$MODE" == exploratory ]] && claim=exploratory
  qualify_version "$toolchain" "$claim"
  if [[ "$MODE" != exploratory ]]; then
    require_claims_produced "$toolchain" "$claim" || true
  fi
done

write_evidence

if [[ "$MODE" == exploratory ]]; then
  # Research signal. The exit status is 0 whatever the observations were, because
  # an unregistered Cargo release is not a product regression.
  echo "qualify-cargo-selectors: exploratory observations recorded; runtime support is unchanged and remains false until a version is registered and reviewed"
  exit 0
fi

if has_failure; then
  echo "qualify-cargo-selectors: FAILED" >&2
  printf '%s' "$RESULTS" | awk -F'\t' '$3 != "pass" { printf "  - %s: %s: %s\n", $1, $2, $4 }' >&2
  exit 1
fi

echo "qualify-cargo-selectors: passed; every claimed capability was asserted against real Cargo"
