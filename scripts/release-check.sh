#!/usr/bin/env bash
# Single local gate for everything that must hold before a cargo-cleanme
# release may be staged. It performs no publication: the last two steps are a
# package build and a publish *dry run*.
#
# Pass the exact release tag to also prove the tag matches the source identity
# in the tree:
#
#     bash scripts/release-check.sh v0.1.2
#
# Without an argument the identity check runs in self-test mode only, so the
# gate is still meaningful for ordinary work.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

RELEASE_TAG=${1:-}

if [[ -n "$(git status --porcelain)" ]]; then
  echo "release-check: working tree is not clean" >&2
  exit 1
fi

if ! command -v eggpack >/dev/null 2>&1; then
  echo "release-check: eggpack is required; install it at the revision pinned in" >&2
  echo "  release/eggpack/github-policy.json with:" >&2
  echo "  cargo install --git https://github.com/eggstack/eggpack --rev <revision> --locked eggpack-cli" >&2
  exit 1
fi

echo "release-check: formatting"
cargo fmt --all -- --check

echo "release-check: clippy"
cargo clippy --all-targets --all-features -- -D warnings

echo "release-check: tests"
cargo test --all-targets --all-features

echo "release-check: doc tests"
cargo test --doc

echo "release-check: MSRV 1.89"
if cargo +1.89 --version >/dev/null 2>&1; then
  cargo +1.89 check --locked --all-targets
  cargo +1.89 test --locked --all-targets
else
  echo "release-check: toolchain 1.89 is not installed; run 'rustup toolchain install 1.89'" >&2
  exit 1
fi

echo "release-check: semantic counters match the recorded baseline"
cargo build --release
python3 scripts/release-benchmark.py --check

echo "release-check: completions and manpage match the clap model"
cargo run --quiet --features dev-tools --bin generate-docs -- --check

echo "release-check: Eggpack workflow shape is derived"
python3 scripts/gen-release-workflow-shape.py --check

echo "release-check: generated release workflow has no drift"
eggpack ci check \
  --workflow-shape release/eggpack/workflow-shape.json \
  --contract release/eggpack/distribution.toml \
  --github-policy release/eggpack/github-policy.json \
  --workflow .github/workflows/release-binaries.yml

echo "release-check: release contract is consistent"
python3 scripts/check-release-contract.py
python3 scripts/check-release-contract.py --self-test

echo "release-check: release identity"
if [[ -n "$RELEASE_TAG" ]]; then
  python3 scripts/check-release-identity.py --tag "$RELEASE_TAG"
else
  echo "no release tag given; proving the identity gate rejects mismatches only"
  python3 scripts/check-release-identity.py --self-test
fi

echo "release-check: installer wrappers match the contract"
# C022: the failing direction first. A release gate that trusts a green wrapper
# check is only as strong as that check's ability to reject a drifted wrapper,
# and until this self-test existed nothing demonstrated it.
python3 scripts/check-installer-contract.py --self-test
python3 scripts/check-installer-contract.py

echo "release-check: post-release smoke matrix is bound to the release contract"
python3 scripts/check-post-release-smoke-contract.py --self-test
python3 scripts/check-post-release-smoke-contract.py

# M011C: the transition resolver is the thing that keeps an automated smoke from
# testing a floating "latest", and the crates.io wait is what keeps it from
# running before the version it updates to exists. Both are premise-tested.
echo "release-check: published-release smoke transition resolver and crates.io wait"
python3 scripts/resolve-smoke-transition.py --self-test

# M011B: staged validation is only an enforcement point if it cannot be made to
# run on the wrong input. Both directions run here, because the quietest failure
# in that workflow is a green validation of a default-branch checkout.
echo "release-check: staged-release validation workflow premises"
python3 scripts/check-staged-validation-contract.py --self-test
python3 scripts/check-staged-validation-contract.py

# M011D: the Cargo selector support claim is a checked-in policy, so it is gated
# the same way. Both directions run because the product fails closed on an
# unknown Cargo version, which is exactly why a stale allowlist is invisible.
echo "release-check: Cargo selector qualification matrix is bound to the policy"
python3 scripts/check-selector-qualification.py --self-test
python3 scripts/check-selector-qualification.py
bash scripts/qualify-cargo-selectors.sh --self-test

# M011A: the attestation verifier is a guard, so its classifications are proved
# rather than trusted. The live `--settings-only` query is what tells the
# operator whether the repository's immutability policy is still on, which is a
# live external fact and not something a self test can establish.
echo "release-check: release attestation verification helper"
python3 scripts/verify-release-attestation.py --self-test
python3 scripts/verify-release-attestation.py --settings-only

echo "release-check: installer fixture qualification"
python3 packaging/tests/test_installers.py

echo "release-check: installer premise guards reject a broken setup"
python3 packaging/tests/test_installers.py --self-test

echo "release-check: fixture portability guard"
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py

echo "release-check: architecture citations resolve"
python3 scripts/check-doc-citations.py --self-test
python3 scripts/check-doc-citations.py

echo "release-check: release candidate smoke validator"
python3 scripts/smoke-release-candidate.py target/debug/cargo-cleanme

echo "release-check: package"
cargo package --locked

echo "release-check: publish dry run"
cargo publish --locked --dry-run

echo "release-check: passed; no publication was performed"
