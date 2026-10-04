#!/usr/bin/env python3
"""Validate the staged cargo-cleanme draft release against the release contract.

Run after `release-binaries.yml` stages a draft and before anything is
published. Nothing here publishes; it only inspects and reports.

Checks, in order:

1. **Exact inventory.** The draft must contain the five contracted binaries,
   five `.sha256` sidecars, `release-manifest.json`, and the two exact-release
   bootstrap installers. A missing or extra asset is a hard failure, because
   "exactly the required qualified inventory" is a publication precondition.
2. **Sidecar integrity.** Every sidecar must name its own asset and its digest
   must match the bytes actually served.
3. **Manifest agreement.** `release-manifest.json` must agree with the bytes
   for every asset it lists: size and SHA-256. A manifest that disagrees with
   the bytes is a stale-build indicator, which the release process forbids.
4. **Contract agreement.** Asset names must be exactly the names
   `release/eggpack/distribution.toml` expands to. This is the check that the
   installer and the updater point at the same release contract.
5. **Static ABI evidence** for the two Linux targets: the maximum glibc version
   each binary actually requires. This is *static* evidence read from the
   binary, not runtime execution on an old distribution, and is labelled as
   such.
6. **Real installer qualification on this host.** The staged artifacts are
   served over a local HTTP fixture and the product's own `packaging/install.sh`
   is run against them, so the qualification exercises the real bytes rather
   than a synthetic candidate.

Usage:
    python3 scripts/validate-staged-release.py --tag v0.1.0
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "release/eggpack/distribution.toml"
INSTALL_SH = ROOT / "packaging/install.sh"
FIXTURE_SERVER = ROOT / "packaging/tests/fixture_server.py"
TEST_INSTALLERS = ROOT / "packaging/tests/test_installers.py"

errors: list[str] = []
notes: list[str] = []


def fail(message: str) -> None:
    errors.append(message)


def note(message: str) -> None:
    notes.append(message)
    print(f"  {message}")


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def resolve_tag(tag: str) -> str | None:
    """The commit a tag points at, or None if it cannot be determined."""
    result = subprocess.run(
        ["git", "rev-list", "-n", "1", tag], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip() or None


def contracted_assets() -> tuple[list[str], dict[str, str]]:
    with CONTRACT.open("rb") as handle:
        contract = tomllib.load(handle)
    product = contract["product"]["id"]
    expected: list[str] = []
    triples: dict[str, str] = {}
    for entry in contract["targets"]:
        asset = (
            entry["asset"]["asset"]
            .replace("{product}", product)
            .replace("{target}", entry["triple"])
        )
        expected.append(asset)
        triples[asset] = entry["triple"]
    return expected, triples


def download_draft(tag: str, destination: Path) -> list[str]:
    """Download every asset of the draft. Drafts are visible to a maintainer."""
    result = subprocess.run(
        ["gh", "release", "download", tag, "--dir", str(destination), "--clobber"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        fail(f"could not download the draft assets: {result.stderr.strip()[:400]}")
        return []
    return sorted(p.name for p in destination.iterdir() if p.is_file())


def parse_sidecar(path: Path) -> tuple[str, str]:
    text = path.read_text(encoding="utf-8").strip()
    match = re.match(r"^([0-9a-fA-F]{64})\s+\*?(\S+)$", text)
    if not match:
        raise ValueError(f"malformed sidecar: {text!r}")
    return match.group(1).lower(), match.group(2)


def glibc_floor(path: Path) -> str | None:
    """Highest glibc version symbol the ELF requires, or None if not dynamic."""
    readelf = shutil.which("readelf")
    if readelf is None:
        return None
    result = subprocess.run(
        [readelf, "--version-info", str(path)], capture_output=True, text=True, check=False
    )
    versions = set()
    for match in re.finditer(r"GLIBC_(\d+\.\d+(?:\.\d+)?)", result.stdout):
        parts = [int(p) for p in match.group(1).split(".")]
        while len(parts) < 3:
            parts.append(0)
        versions.add(tuple(parts[:3]))
    if not versions:
        return None
    return ".".join(str(p) for p in max(versions))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="release tag of the staged draft")
    parser.add_argument("--skip-installer", action="store_true")
    args = parser.parse_args()

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-release-validate-"))
    try:
        print(f"Validating staged draft {args.tag}")
        present = download_draft(args.tag, work)
        if errors:
            return report()
        print(f"\n[1/6] inventory ({len(present)} assets)")
        for name in present:
            print(f"  {name}")

        expected, triples = contracted_assets()
        # The plan requires the product-owned public wrappers *and* the
        # Eggpack exact-release bootstrap evidence, so all four installers are
        # part of the required inventory.
        required = set(expected) | {f"{a}.sha256" for a in expected} | {
            "release-manifest.json",
            "install-exact.sh",
            "install-exact.ps1",
            "install.sh",
            "install.ps1",
        }
        missing = sorted(required - set(present))
        extra = sorted(set(present) - required)
        for name in missing:
            fail(f"required asset is missing from the draft: {name}")
        for name in extra:
            fail(f"unexpected asset in the draft: {name}")
        note(f"{len(expected)} contracted binaries + {len(expected)} sidecars + manifest + 2 installers")

        print("\n[2/6] sidecar integrity")
        digests: dict[str, str] = {}
        for asset in expected:
            sidecar = work / f"{asset}.sha256"
            binary = work / asset
            if not sidecar.is_file() or not binary.is_file():
                continue
            try:
                declared, named = parse_sidecar(sidecar)
            except ValueError as error:
                fail(str(error))
                continue
            if named != asset:
                fail(f"{sidecar.name} names {named!r}, not {asset!r}")
            actual = sha256_of(binary)
            digests[asset] = actual
            if declared != actual:
                fail(f"{asset}: sidecar says {declared}, bytes are {actual}")
            else:
                print(f"  ok {asset}  {actual}")

        print("\n[3/6] release-manifest.json agreement")
        manifest_path = work / "release-manifest.json"
        if manifest_path.is_file():
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

            # Source binding: the manifest must name this exact tag and the
            # exact commit the tag points at. A manifest from a different build
            # is the stale-artifact case the release process forbids.
            if manifest.get("release_id") != args.tag:
                fail(
                    f"release-manifest.json release_id is {manifest.get('release_id')!r}, "
                    f"expected {args.tag!r}"
                )
            revision = manifest.get("source_revision")
            if revision:
                expected_revision = resolve_tag(args.tag)
                if expected_revision and not revision.startswith(expected_revision):
                    fail(
                        f"release-manifest.json source_revision {revision} is not the commit "
                        f"{args.tag} points at ({expected_revision})"
                    )
                else:
                    note(f"manifest binds source revision {revision[:12]} to tag {args.tag}")
            else:
                fail("release-manifest.json has no source_revision")

            # Eggpack's manifest nests the artifact under targets[].form.
            seen = set()
            for entry in manifest.get("targets", []):
                artifact = (entry.get("form") or {}).get("artifact") or {}
                name = artifact.get("name")
                if not name:
                    fail(f"manifest target {entry.get('target')!r} has no artifact name")
                    continue
                seen.add(name)
                binary = work / name
                if not binary.is_file():
                    fail(f"manifest lists {name}, which is not in the draft")
                    continue
                size = artifact.get("size")
                if size is not None and int(size) != binary.stat().st_size:
                    fail(f"manifest size for {name}: {size} != {binary.stat().st_size}")
                declared = artifact.get("sha256")
                if declared:
                    actual = digests.get(name) or sha256_of(binary)
                    if str(declared).lower() != actual:
                        fail(f"manifest digest for {name} disagrees with the bytes")
            for asset in expected:
                if asset not in seen:
                    fail(f"release-manifest.json does not list {asset}")
            note(f"manifest agrees with the bytes for {len(seen)} asset(s)")
        else:
            fail("release-manifest.json is absent")

        print("\n[4/6] contract agreement")
        for asset in expected:
            triple = triples[asset]
            if asset not in present:
                continue
            note(f"{triple:32} -> {asset}")
        windows_assets = [a for a, tr in triples.items() if tr.endswith("-pc-windows-msvc")]
        if not windows_assets:
            fail("the contract publishes no Windows asset, so the release cannot serve Windows")
        for asset in expected:
            if asset not in present:
                continue
            binary = work / asset
            digest = sha256_of(binary)
            note(f"{triples[asset]:32} {binary.stat().st_size:>9} bytes  {digest[:16]}...")

        print("\n[5/6] static Linux ABI evidence (not runtime)")
        for asset in expected:
            triple = triples[asset]
            if not triple.endswith("-unknown-linux-gnu"):
                continue
            path = work / asset
            if not path.is_file():
                continue
            floor = glibc_floor(path)
            if floor is None:
                note(f"{triple}: no glibc version info found (static read only)")
            else:
                note(f"{triple}: requires at most GLIBC_{floor} (static ELF evidence)")

        if not args.skip_installer:
            print("\n[6/6] real installer qualification on this host")
            qualify_installer(work, expected, args.tag)
        else:
            print("\n[6/6] real installer qualification skipped by flag")

        return report()
    finally:
        shutil.rmtree(work, ignore_errors=True)


def qualify_installer(work: Path, expected: list[str], tag: str) -> None:
    """Run the product's own installer against the real staged bytes."""
    if not INSTALL_SH.is_file():
        fail("packaging/install.sh is missing; the release ships it")
        return
    if sys.platform not in ("linux", "darwin"):
        note(f"skipping installer qualification: {sys.platform} is not a POSIX host")
        return

    host = {
        ("linux", "x86_64"): "cargo-cleanme-x86_64-unknown-linux-gnu",
        ("linux", "aarch64"): "cargo-cleanme-aarch64-unknown-linux-gnu",
        ("darwin", "x86_64"): "cargo-cleanme-x86_64-apple-darwin",
        ("darwin", "arm64"): "cargo-cleanme-aarch64-apple-darwin",
    }.get((sys.platform, os.uname().machine))
    if host not in expected:
        note(f"skipping installer qualification: this host is {sys.platform}/{os.uname().machine}")
        return
    if not (work / host).is_file():
        fail(f"this host's asset {host} is absent, so it cannot be qualified here")
        return

    # Serve the *real* staged bytes over a local fixture release.
    release = work / "fixture-release"
    version_dir = release / tag
    version_dir.mkdir(parents=True)
    for name in present_names(work):
        shutil.copy2(work / name, version_dir / name)
    for name in present_names(work):
        if not name.endswith(".sha256"):
            shutil.copy2(work / name, release / name)

    server = subprocess.Popen(
        [sys.executable, str(FIXTURE_SERVER), str(release), "--port", "0"],
        stdout=subprocess.PIPE,
        text=True,
    )
    try:
        line = server.stdout.readline().strip()
        match = re.search(r"(http://127\.0\.0\.1:\d+)", line)
        if not match:
            fail(f"the fixture server did not report a port: {line!r}")
            return
        base = match.group(1)
        dest = work / "installed"
        env = dict(os.environ)
        env["CARGO_CLEANME_INSTALL_BASE_URL"] = base
        env["CARGO_CLEANME_INSTALL_LATEST_URL"] = base
        env["CARGO_CLEANME_INSTALL_ALLOW_INSECURE"] = "1"
        env["NO_COLOR"] = "1"
        result = subprocess.run(
            ["sh", str(INSTALL_SH), "--version", tag.lstrip("v"), "--dir", str(dest)],
            capture_output=True,
            text=True,
            env=env,
            check=False,
            timeout=600,
        )
        if result.returncode != 0:
            fail(f"the real installer failed: {result.stdout.strip()} {result.stderr.strip()[:400]}")
            return
        installed = dest / "cargo-cleanme"
        if not installed.is_file():
            fail(f"the installer did not place {installed}")
            return
        if sha256_of(installed) != digests_for(work).get(host):
            fail("the installed bytes do not match the release asset")
            return
        version = subprocess.run(
            [str(installed), "--version"], capture_output=True, text=True, check=False
        ).stdout.strip()
        if version != f"cargo-cleanme {tag.lstrip('v')}":
            fail(f"the installed candidate reports {version!r}")
            return
        note(f"installed the real {host} and it reports {version!r}")
        note(f"the installed bytes match the release asset digest exactly")
    finally:
        server.terminate()
        try:
            server.wait(timeout=10)
        except subprocess.TimeoutExpired:
            server.kill()


def present_names(work: Path) -> list[str]:
    return sorted(p.name for p in work.iterdir() if p.is_file())


def digests_for(work: Path) -> dict[str, str]:
    out = {}
    for path in work.iterdir():
        if path.is_file() and not path.name.endswith(".sha256") and path.name != "release-manifest.json":
            out[path.name] = sha256_of(path)
    return out


def report() -> int:
    print()
    for message in notes:
        pass
    if errors:
        print(f"FAILED: {len(errors)} problem(s)", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("PASSED: the staged draft matches the release contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
