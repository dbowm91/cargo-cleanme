#!/usr/bin/env bash
set -euo pipefail

# Real Cargo selector characterization. Runs only in a private temp directory.
# Usage: scripts/qualify-cargo-selectors.sh [rustup-toolchain ...]
if (($#)); then versions=("$@"); else versions=(1.89 1.91 1.92 stable); fi
tmp=$(mktemp -d "${TMPDIR:-/tmp}/cargo-cleanme-selector.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

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
    cat > "$base/$package/Cargo.toml" <<EOF
[package]
name = "$package"
version = "0.1.0"
edition = "2021"

[dependencies]
shared = { path = "../shared" }
EOF
    printf 'fn main() { println!("{}", shared::value()); }\n' > "$base/$package/src/main.rs"
  done
}

for version in ${versions[@]}; do
  cargo_cmd=(rustup run "$version" cargo)
  version_text=$("${cargo_cmd[@]}" -V)
  fixture="$tmp/${version//[^[:alnum:]]/_}"
  target="$fixture/redirected-target"
  build_dir="$fixture/redirected-build"
  mkdir -p "$fixture"
  make_fixture "$fixture"
  cat > "$fixture/.cargo/config.toml" <<EOF
[build]
target-dir = "$target"
EOF
  minor=$(printf '%s\n' "$version_text" | sed -E 's/^cargo 1\.([0-9]+).*/\1/')
  if [[ "$minor" =~ ^[0-9]+$ ]] && ((minor >= 91)); then
    printf 'build-dir = "%s"\n' "$build_dir" >> "$fixture/.cargo/config.toml"
  fi
  pushd "$fixture" >/dev/null
  echo "=== $version_text ==="

  "${cargo_cmd[@]}" generate-lockfile --offline --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" build --workspace --offline --locked --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" build --workspace --release --offline --locked --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" build --workspace --profile custom --offline --locked --manifest-path "$fixture/Cargo.toml"
  before=$(find "$target" -type f -print | LC_ALL=C sort)
  "${cargo_cmd[@]}" clean --package app-a --dry-run --verbose --offline --locked --manifest-path "$fixture/Cargo.toml" > "$fixture/package-dry-run.txt" 2>&1
  after_dry_run=$(find "$target" -type f -print | LC_ALL=C sort)
  [[ "$before" == "$after_dry_run" ]] || { echo 'FAIL: package dry-run mutated output'; exit 1; }
  "${cargo_cmd[@]}" clean --package app-a --offline --locked --manifest-path "$fixture/Cargo.toml"
  after=$(find "$target" -type f -print | LC_ALL=C sort)
  [[ ! -e "$target/debug/app-a" ]] || { echo 'FAIL: selected package executable survived'; exit 1; }
  [[ -e "$target/debug/app-b" ]] || { echo 'FAIL: sibling package executable was removed'; exit 1; }
  [[ -e "$target/release/app-a" ]] || { echo 'FAIL: package selector removed release profile'; exit 1; }
  removed_count=$(comm -23 <(printf '%s\n' "$before") <(printf '%s\n' "$after") | wc -l | tr -d ' ')
  if find "$target/debug/deps" -maxdepth 1 -name 'libshared-*' -print -quit | grep -q .; then shared_state=preserved; else shared_state=removed; fi
  printf 'package selector: app-a removed; app-b/release preserved; shared_dependency=%s; removed_paths=%s; dry-run_nonmutating=yes\n' "$shared_state" "$removed_count"
  printf 'package dry-run summary: %s\n' "$(tail -1 "$fixture/package-dry-run.txt")"

  "${cargo_cmd[@]}" build --workspace --offline --locked --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" build --workspace --release --offline --locked --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" clean --profile release --offline --locked --manifest-path "$fixture/Cargo.toml"
  [[ ! -e "$target/release/app-a" && -e "$target/debug/app-b" && -e "$target/custom/app-a" ]] || { echo 'FAIL: release selector crossed profile boundary'; exit 1; }
  echo 'release selector: release removed; debug/custom preserved'

  "${cargo_cmd[@]}" build --workspace --profile custom --offline --locked --manifest-path "$fixture/Cargo.toml"
  "${cargo_cmd[@]}" clean --profile custom --offline --locked --manifest-path "$fixture/Cargo.toml"
  [[ ! -e "$target/custom/app-a" && -e "$target/debug/app-b" ]] || { echo 'FAIL: custom selector crossed profile boundary'; exit 1; }
  echo 'custom selector: custom removed; debug preserved'

  if "${cargo_cmd[@]}" clean --workspace --dry-run --offline --locked --manifest-path "$fixture/Cargo.toml" > "$fixture/workspace-selector.txt" 2>&1; then
    echo 'workspace selector: supported'
  else
    echo 'workspace selector: unsupported'
  fi

  if [[ -d "$build_dir" ]]; then echo 'distinct build-dir: created'; else echo 'distinct build-dir: not created'; fi
  host=$(${cargo_cmd[0]} run "$version" rustc -vV | sed -n 's/^host: //p')
  config_fixture="$tmp/${version//[^[:alnum:]]/_}-configured-target"
  make_fixture "$config_fixture"
  config_target="$config_fixture/target-output"
  printf '[build]\ntarget-dir = "%s"\n' "$config_target" > "$config_fixture/.cargo/config.toml"
  (cd "$config_fixture" && "${cargo_cmd[@]}" generate-lockfile --offline --manifest-path Cargo.toml && "${cargo_cmd[@]}" build --workspace --offline --locked --manifest-path Cargo.toml)
  mkdir -p "$config_target/$host"
  cp -R "$config_target/debug" "$config_target/$host/"
  (cd "$config_fixture" && "${cargo_cmd[@]}" clean --package app-a --dry-run --verbose --config "build.target=\"$host\"" --offline --locked --manifest-path Cargo.toml) > "$config_fixture/config-target-clean.txt" 2>&1
  (cd "$config_fixture" && "${cargo_cmd[@]}" clean --package app-a --dry-run --verbose --target "$host" --offline --locked --manifest-path Cargo.toml) > "$config_fixture/explicit-target-clean.txt" 2>&1
  config_files=$(grep -c "target-output/$host/debug/" "$config_fixture/config-target-clean.txt" || true)
  explicit_files=$(grep -c "target-output/$host/debug/" "$config_fixture/explicit-target-clean.txt" || true)
  printf 'configured build.target package dry-run paths=%s; explicit --target paths=%s; triple=%s\n' "$config_files" "$explicit_files" "$host"
  popd >/dev/null
done
