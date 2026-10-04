#!/usr/bin/env bash
# Single local gate for everything that must hold before a cargo-cleanme
# release may be staged. It performs no publication: the last two steps are a
# package build and a publish *dry run*.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT"

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

echo "release-check: package"
cargo package --locked

echo "release-check: publish dry run"
cargo publish --locked --dry-run

echo "release-check: passed; no publication was performed"
