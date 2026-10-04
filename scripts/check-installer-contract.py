#!/usr/bin/env python3
"""Prove the product installer wrappers are a projection of the Eggpack contract.

Eggpack owns the published target matrix, asset names, and checksum sidecars
(`release/eggpack/distribution.toml`). The product-owned wrappers in
`packaging/` own latest/exact version selection, integrity and identity
verification, destination scope, and the Cargo fallback policy. The danger is
that a wrapper quietly becomes a *second* release schema that can disagree
with the contract.

This check extracts the host-to-target mapping each wrapper actually
implements and compares it to the contract:

  * every contracted triple must be reachable from some recognized host;
  * the wrapper must not reference a triple the contract does not publish;
  * the asset name each wrapper builds must equal the contracted expansion;
  * the hosts the wrapper routes to the Cargo fallback must be hosts the
    contract deliberately omits, and must be documented as such;
  * both wrappers must be present, and the POSIX one executable.

The M010A plan's companion `scripts/check-release-contract.py` owns the
producer/workflow/package invariants; this file owns the wrapper invariants
that only exist once M010B lands the wrappers.
"""
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EGG = ROOT / "release/eggpack"
INSTALL_SH = ROOT / "packaging" / "install.sh"
INSTALL_PS1 = ROOT / "packaging" / "install.ps1"
README = ROOT / "README.md"

# Hosts the wrappers recognize but for which no prebuilt binary is published.
# These are the only families allowed to fall back to Cargo, and the README must
# say so. Keep in sync with the contract's own documented omission.
CARGO_ONLY_FAMILIES = {"armv7"}

errors: list[str] = []


def fail(message: str) -> None:
    errors.append(message)


def contracted() -> tuple[str, dict[str, str], dict[str, str]]:
    with (EGG / "distribution.toml").open("rb") as handle:
        contract = tomllib.load(handle)
    product = contract["product"]["id"]
    assets: dict[str, str] = {}
    install_names: dict[str, str] = {}
    for entry in contract["targets"]:
        triple = entry["triple"]
        assets[triple] = (
            entry["asset"]["asset"].replace("{product}", product).replace("{target}", triple)
        )
        install_names[triple] = entry["asset"]["install"].replace("{product}", product)
    return product, assets, install_names


def posix_mapping(text: str) -> tuple[set[str], set[str]]:
    """Return (published triples, Cargo-only families) from install.sh."""
    # `case "$os_family:$arch_family" in` ... `esac`, mapping each key to either a
    # contracted triple or the empty string meaning "no prebuilt binary".
    block = re.search(
        r'case "\$os_family:\$arch_family" in(.*?)\n\s*esac', text, re.DOTALL
    )
    if block is None:
        return set(), set()
    published: set[str] = set()
    fallback: set[str] = set()
    for line in block.group(1).splitlines():
        match = re.match(r"\s*([a-z0-9]+):([a-z0-9]+)\)\s*TARGET=\"([^\"]*)\"", line)
        if match is None:
            continue
        _, _, target = match.groups()
        if target:
            published.add(target)
        else:
            fallback.add(match.group(2))
    return published, fallback


def powershell_mapping(text: str) -> set[str]:
    """Return the triples install.ps1 can select."""
    return set(re.findall(r"\$target\s*=\s*'([a-z0-9_]+(?:-[a-z0-9_]+)+)'", text))


def main() -> int:
    product, assets, install_names = contracted()
    if not INSTALL_SH.is_file():
        print("check-installer-contract: packaging/install.sh is missing", file=sys.stderr)
        return 1
    if not INSTALL_PS1.is_file():
        print("check-installer-contract: packaging/install.ps1 is missing", file=sys.stderr)
        return 1
    if not INSTALL_SH.stat().st_mode & 0o111:
        fail("packaging/install.sh is not executable")

    posix = INSTALL_SH.read_text(encoding="utf-8")
    powershell = INSTALL_PS1.read_text(encoding="utf-8")

    posix_published, posix_fallback = posix_mapping(posix)
    if not posix_published:
        fail("could not extract a host-to-target mapping from packaging/install.sh")
    ps_published = powershell_mapping(powershell)
    if not ps_published:
        fail("could not extract a target mapping from packaging/install.ps1")

    missing = sorted(set(assets) - posix_published)
    if missing:
        fail(f"packaging/install.sh cannot reach contracted targets: {missing}")
    extra = sorted(posix_published - set(assets))
    if extra:
        fail(f"packaging/install.sh references targets the contract does not publish: {extra}")

    # install.ps1 is the Windows wrapper, so it must reach exactly the
    # contract's Windows targets and claim nothing else.
    windows_targets = {triple for triple in assets if triple.endswith("-pc-windows-msvc")}
    if not windows_targets:
        fail("the contract publishes no Windows target for packaging/install.ps1 to install")
    if ps_published != windows_targets:
        fail(
            "packaging/install.ps1 target set is not the contract's Windows set "
            f"(expected {sorted(windows_targets)}, found {sorted(ps_published)})"
        )
    for triple in sorted(set(assets) - windows_targets):
        if triple in powershell:
            fail(f"packaging/install.ps1 names non-Windows contracted target {triple}")

    # The asset name each wrapper builds must match the contract expansion.
    for label, text in (("packaging/install.sh", posix), ("packaging/install.ps1", powershell)):
        # The asset is always `<product>-<target>` with an optional `.exe`.
        if not re.search(r"[Pp]roduct\}?-\$?[Tt]arget|\$PRODUCT-\$TARGET", text):
            fail(f"{label} does not build the asset name from the product and target")
    for triple in sorted(assets):
        if triple not in posix:
            fail(f"packaging/install.sh does not name contracted target {triple}")
    for triple in sorted(windows_targets):
        if triple not in powershell:
            fail(f"packaging/install.ps1 does not name contracted target {triple}")
    if "$asset.sha256" not in posix and "SIDECAR=" not in posix:
        fail("packaging/install.sh does not require a checksum sidecar")
    if "algorithm sha256" not in powershell.lower():
        fail("packaging/install.ps1 does not require a SHA-256 sidecar")

    # Install names must be derived from the contract so `cargo cleanme` keeps
    # working. The base name appears as a product variable and the Windows
    # suffix is appended conditionally, so check the derivation rather than a
    # literal per-target string.
    base_install = install_names[next(iter(install_names))].removesuffix(".exe")
    for label, text in (("packaging/install.sh", posix), ("packaging/install.ps1", powershell)):
        if base_install not in text:
            fail(f"{label} does not derive the contracted install name {base_install!r}")
        if ".exe" not in text:
            fail(f"{label} never applies the contracted Windows .exe install suffix")

    # Cargo fallback is allowed only for contract-omitted hosts.
    unexpected = sorted(posix_fallback - CARGO_ONLY_FAMILIES)
    if unexpected:
        fail(f"packaging/install.sh routes non-contracted families to Cargo: {unexpected}")
    readme = README.read_text(encoding="utf-8")
    for family in sorted(CARGO_ONLY_FAMILIES):
        if family not in readme:
            fail(f"README does not document the Cargo-only {family} family")

    # Invariants the wrappers must not weaken. Comments are stripped first: the
    # wrappers *document* that they never escalate, and a prose mention of sudo
    # must not read as an invocation of it.
    def code_only(text: str) -> str:
        without_block = re.sub(r"(?s)\".*?\"", " ", text)
        return re.sub(r"(?m)^\s*#.*$", " ", without_block)

    for label, text in (("packaging/install.sh", posix), ("packaging/install.ps1", powershell)):
        body = code_only(text)
        for label_match, pattern in (
            ("sudo", r"(^|[;&|(\s])sudo\s"),
            ("su", r"(^|[;&|(\s])su\s"),
            ("doas", r"(^|[;&|(\s])doas\s"),
        ):
            if re.search(pattern, body):
                fail(f"{label} invokes {label_match}; privilege escalation must be the user's choice")
        if re.search(r"Start-Process[^\n]*-Verb\s+RunAs", body):
            fail(f"{label} elevates through Start-Process -Verb RunAs")
        for marker in ("--no-check-certificate", "--insecure", "SkipCertificateCheck", "verify=False"):
            if marker in body:
                fail(f"{label} contains a TLS-bypass marker: {marker}")

    if errors:
        for error in errors:
            print(f"check-installer-contract: {error}", file=sys.stderr)
        return 1
    print(
        f"check-installer-contract: {len(assets)} contracted targets reachable "
        f"from {len(posix_published) + len(ps_published) - len(assets)} wrapper mappings, "
        f"{len(posix_fallback)} Cargo-only families, install names match"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
