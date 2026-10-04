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
python3 scripts/check-installer-contract.py

echo "release-check: installer fixture qualification"
python3 packaging/tests/test_installers.py

echo "release-check: installer premise guards reject a broken setup"
python3 packaging/tests/test_installers.py --self-test

echo "release-check: fixture portability guard"
python3 scripts/check-fixture-portability.py --self-test
python3 scripts/check-fixture-portability.py

echo "release-check: release candidate smoke validator"
python3 scripts/smoke-release-candidate.py target/debug/cargo-cleanme

echo "release-check: package"
cargo package --locked

echo "release-check: publish dry run"
cargo publish --locked --dry-run

echo "release-check: passed; no publication was performed"
