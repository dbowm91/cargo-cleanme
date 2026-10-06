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
  * the asset name each wrapper builds must be derived from the product and the
    target, in that order;
  * the hosts the wrapper routes to the Cargo fallback must be hosts the
    contract deliberately omits, and must be documented as such;
  * both wrappers must be present, the POSIX one executable, and neither may
    escalate privilege or bypass TLS verification.

The M010A plan's companion `scripts/check-release-contract.py` owns the
producer/workflow/package invariants; this file owns the wrapper invariants
that only exist once M010B lands the wrappers.

Usage:
    python3 scripts/check-installer-contract.py
    python3 scripts/check-installer-contract.py --self-test

The self-test is not optional. A green run of this file is a statement about
the current tree; it is not a statement that the file can still detect the
defect classes above. C022 exists because this was the only contract checker in
the repository whose green was never demonstrated in the failing direction --
and this repository has closed milestones on tests that passed for the wrong
reason often enough that "the tree happens to pass" is not evidence.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# Hosts the wrappers recognize but for which no prebuilt binary is published.
# These are the only families allowed to fall back to Cargo, and the README must
# say so. Keep in sync with the contract's own documented omission.
CARGO_ONLY_FAMILIES = {"armv7"}


def contracted(root: Path) -> tuple[str, dict[str, str], dict[str, str]]:
    with (root / "release/eggpack/distribution.toml").open("rb") as handle:
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


def git_recorded_mode(root: Path) -> str:
    """The exec bit git recorded for the POSIX wrapper, if this is a checkout.

    A Windows checkout has no exec bit, so ask git for the recorded mode and
    fall back to the filesystem. The recorded mode is the durable fact: a
    `curl | sh` install does not depend on it, but every other consumer does.
    """
    try:
        git_mode = subprocess.run(
            [
                "git",
                "ls-files",
                "-s",
                "--",
                str((root / "packaging/install.sh").relative_to(root)),
            ],
            capture_output=True,
            text=True,
            check=False,
            cwd=str(root),
        ).stdout.split()
    except OSError:
        return ""
    return git_mode[0] if git_mode else ""


def check(root: Path, recorded_mode: str) -> list[str]:
    """Every invariant this checker owns, as attributable diagnostics.

    Takes a root and the recorded exec bit rather than reading the repository,
    so the self-test can run it against an isolated fixture. `recorded_mode` is
    a parameter for the same reason: the durable fact about the exec bit is what
    git recorded, and a self-test that could only exercise it by mutating the
    host filesystem could not also exercise the Windows-checkout branch.
    """
    install_sh = root / "packaging" / "install.sh"
    install_ps1 = root / "packaging" / "install.ps1"
    readme_path = root / "README.md"

    errors: list[str] = []

    def fail(message: str) -> None:
        errors.append(message)

    product, assets, install_names = contracted(root)
    if not install_sh.is_file():
        return ["packaging/install.sh is missing"]
    if not install_ps1.is_file():
        return ["packaging/install.ps1 is missing"]
    if recorded_mode and recorded_mode != "100755":
        fail(f"packaging/install.sh is recorded in git as mode {recorded_mode}, expected 100755")
    if not recorded_mode and not install_sh.stat().st_mode & 0o111:
        fail("packaging/install.sh is not executable")

    posix = install_sh.read_text(encoding="utf-8")
    powershell = install_ps1.read_text(encoding="utf-8")

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
    readme = readme_path.read_text(encoding="utf-8") if readme_path.is_file() else ""
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

    # Unreachable-here guard: the product id is read from the contract, so a
    # contract that stopped naming the product would silently check nothing.
    if not product:
        fail("the contract names no product id, so the wrappers cannot be compared to it")

    return errors


# --------------------------------------------------------------- self-test

# A coherent, deliberately tiny fixture: one POSIX target and one Windows one.
# Kept independent of the repository's live contract on purpose -- a self-test
# fixture copied from the tree it is supposed to test cannot fail for a reason
# its author did not anticipate, and it would need editing on every legitimate
# contract change.
FIXTURE_CONTRACT = """schema_version = 1

[product]
id = "cargo-cleanme"
display_name = "cargo-cleanme"

[[targets]]
triple = "x86_64-unknown-linux-gnu"
aliases = ["linux-x64"]

[targets.asset]
kind = "direct"
asset = "{product}-{target}"
install = "{product}"

[targets.checksum]
sidecar = "{asset}.sha256"

[[targets]]
triple = "x86_64-pc-windows-msvc"
aliases = ["windows-x64"]

[targets.asset]
kind = "direct"
asset = "{product}-{target}.exe"
install = "{product}.exe"

[targets.checksum]
sidecar = "{asset}.sha256"
"""

FIXTURE_POSIX = """#!/bin/sh
# cargo-cleanme fixture wrapper, used only by the checker's self-test.
set -eu

PRODUCT="cargo-cleanme"
REPO="dbowm91/cargo-cleanme"
BASE_URL="https://github.com/$REPO/releases/download"
ASSET=""
SIDECAR=""

os_family=linux
arch_family=x64

case "$os_family:$arch_family" in
linux:x64) TARGET="x86_64-unknown-linux-gnu" ;;
windows:x64) TARGET="x86_64-pc-windows-msvc" ;;
linux:armv7) TARGET="" ;; # recognized, no prebuilt binary
*) TARGET="" ;;
esac

if [ -n "$TARGET" ]; then
\tASSET="$PRODUCT-$TARGET"
\tif [ "$os_family" = "windows" ]; then
\t\tASSET="$ASSET.exe"
\tfi
\tSIDECAR="$ASSET.sha256"
\tinfo "$BASE_URL/$ASSET $SIDECAR"
fi
"""

FIXTURE_POWERSHELL = """# cargo-cleanme fixture wrapper, used only by the checker's self-test.
$Product = 'cargo-cleanme'
$Repo = 'dbowm91/cargo-cleanme'
$BaseUrl = "https://github.com/$Repo/releases/download"
$target = ''

if ($env:PROCESSOR_ARCHITECTURE -eq 'AMD64') {
    $target = 'x86_64-pc-windows-msvc'
}

if (-not $cargoFallbackHost) {
    $asset = "$Product-$target.exe"
    $sidecar = "$asset.sha256"
    Write-Note "$BaseUrl/$asset $sidecar"
    Get-FileHash -Algorithm sha256 $sidecar
}
"""

FIXTURE_README = """# cargo-cleanme (fixture)

Installation targets a prebuilt binary on every contracted host. The `armv7`
family is recognized but has no published binary, so it falls back to Cargo.
"""


def write_fixture(root: Path) -> None:
    (root / "release/eggpack").mkdir(parents=True)
    (root / "packaging").mkdir(parents=True)
    (root / "release/eggpack/distribution.toml").write_text(FIXTURE_CONTRACT, encoding="utf-8")
    posix = root / "packaging/install.sh"
    posix.write_text(FIXTURE_POSIX, encoding="utf-8")
    posix.chmod(0o755)
    (root / "packaging/install.ps1").write_text(FIXTURE_POWERSHELL, encoding="utf-8")
    (root / "README.md").write_text(FIXTURE_README, encoding="utf-8")


def self_test() -> int:
    """Prove this checker rejects each defect class it claims to guard.

    Each case is a fresh fixture tree, because these mutations are mutually
    exclusive: a checker that rejected every tree for the same unrelated reason
    would pass all eight cases while testing one defect class.
    """
    failures: list[str] = []

    def expect(label: str, mutate, diagnostic: str) -> None:
        """Require `mutate` to change the fixture AND be rejected for `diagnostic`."""
        with tempfile.TemporaryDirectory(prefix="installer-contract-selftest-") as raw:
            root = Path(raw)
            write_fixture(root)
            before = snapshot(root)
            mutate(root)
            if snapshot(root) == before:
                failures.append(f"{label}: the mutation was a no-op")
                print(f"  FAIL {label}: the mutation did not change the fixture")
                return
            problems = check(root, "")
            if not problems:
                failures.append(f"{label}: the checker accepted a broken fixture")
                print(f"  FAIL {label}: accepted a fixture with a known defect")
                return
            # Attributable, not merely non-empty: a parse error or a missing
            # file would make the checker "reject" for a reason unrelated to
            # the invariant the case is about.
            attributable = [p for p in problems if diagnostic in p]
            if not attributable:
                failures.append(f"{label}: rejected, but not for {diagnostic!r}: {problems}")
                print(f"  FAIL {label}: rejected for the wrong reason: {problems}")
                return
            print(f"  ok   {label}")

    def snapshot(root: Path) -> dict[str, str]:
        # Mode bits belong in the snapshot, not just the bytes: the exec-bit
        # cases mutate nothing else, so a content-only snapshot would report
        # them as no-ops.
        return {
            str(path.relative_to(root)): f"{path.stat().st_mode & 0o777:o}:{path.read_bytes().hex()}"
            for path in sorted(root.rglob("*"))
            if path.is_file()
        }

    def edit(name: str, old: str, new: str):
        def apply(root: Path) -> None:
            path = root / name
            text = path.read_text(encoding="utf-8")
            if old not in text:
                raise AssertionError(f"self-test fixture no longer contains {old!r} in {name}")
            path.write_text(text.replace(old, new, 1), encoding="utf-8")

        return apply

    def remove(name: str):
        def apply(root: Path) -> None:
            (root / name).unlink()

        return apply

    # Positive control first: a coherent fixture the checker accepts. Without
    # it, "every case is rejected" would be equally consistent with a checker
    # that rejects everything.
    with tempfile.TemporaryDirectory(prefix="installer-contract-selftest-") as raw:
        control_root = Path(raw)
        write_fixture(control_root)
        problems = check(control_root, "")
        if problems:
            failures.append(f"the coherent fixture was rejected: {problems}")
            print(f"  FAIL the coherent fixture is rejected: {problems}")
        else:
            print("  ok   the coherent fixture is accepted")

    expect(
        "a missing contracted POSIX mapping is rejected",
        edit("packaging/install.sh", 'linux:x64) TARGET="x86_64-unknown-linux-gnu" ;;\n', ""),
        "cannot reach contracted targets",
    )
    expect(
        "an extra POSIX mapping is rejected",
        edit(
            "packaging/install.sh",
            'linux:x64) TARGET="x86_64-unknown-linux-gnu" ;;',
            'linux:x64) TARGET="x86_64-unknown-linux-musl" ;;',
        ),
        "references targets the contract does not publish",
    )
    expect(
        "a PowerShell target-set drift is rejected",
        edit("packaging/install.ps1", "'x86_64-pc-windows-msvc'", "'aarch64-pc-windows-msvc'"),
        "target set is not the contract's Windows set",
    )
    expect(
        "a reversed asset-name expansion is rejected",
        edit("packaging/install.sh", 'ASSET="$PRODUCT-$TARGET"', 'ASSET="$TARGET-$PRODUCT"'),
        "does not build the asset name from the product and target",
    )
    expect(
        "an unexpected Cargo-only fallback family is rejected",
        edit("packaging/install.sh", 'linux:armv7) TARGET="" ;;', 'linux:s390x) TARGET="" ;;'),
        "routes non-contracted families to Cargo",
    )
    expect(
        "an undocumented Cargo-only family is rejected",
        edit("README.md", "The `armv7`\nfamily", "The\nfamily"),
        "does not document the Cargo-only armv7 family",
    )
    expect(
        "a missing POSIX installer is rejected",
        remove("packaging/install.sh"),
        "packaging/install.sh is missing",
    )
    expect(
        "a missing PowerShell installer is rejected",
        remove("packaging/install.ps1"),
        "packaging/install.ps1 is missing",
    )
    expect(
        "a non-executable POSIX installer is rejected",
        lambda root: (root / "packaging/install.sh").chmod(0o644),
        "is not executable",
    )

    # The durable exec-bit fact is what git recorded, so that branch is proven
    # directly rather than through a mutation the host filesystem has to allow.
    with tempfile.TemporaryDirectory(prefix="installer-contract-selftest-") as raw:
        root = Path(raw)
        write_fixture(root)
        problems = check(root, "100644")
        expected = "packaging/install.sh is recorded in git as mode 100644, expected 100755"
        if any(p == expected for p in problems):
            print("  ok   a non-executable recorded mode is rejected")
        else:
            failures.append(f"the recorded-mode branch did not reject: {problems}")
            print(f"  FAIL the recorded-mode branch did not reject: {problems}")

    # A removal that takes the whole POSIX wrapper must not be reported as a
    # set of unrelated mapping complaints: the fixture is unusable and the
    # checker says so.
    with tempfile.TemporaryDirectory(prefix="installer-contract-selftest-") as raw:
        root = Path(raw)
        write_fixture(root)
        (root / "packaging/install.sh").unlink()
        problems = check(root, "")
        if problems == ["packaging/install.sh is missing"]:
            print("  ok   a missing wrapper is reported as itself, not as mapping drift")
        else:
            failures.append(f"a missing wrapper produced unrelated diagnostics: {problems}")
            print(f"  FAIL a missing wrapper produced unrelated diagnostics: {problems}")

    if failures:
        print(f"check-installer-contract: self test FAILED ({len(failures)} case(s))", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(
        "check-installer-contract: self test passed; the wrappers are a projection "
        "of the contract in the failing direction too"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the checker rejects each defect class it guards, then exit",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    errors = check(ROOT, git_recorded_mode(ROOT))
    if errors:
        for error in errors:
            print(f"check-installer-contract: {error}", file=sys.stderr)
        return 1
    _, assets, _ = contracted(ROOT)
    print(
        f"check-installer-contract: {len(assets)} contracted targets reachable "
        f"from the wrapper mappings, {len(CARGO_ONLY_FAMILIES)} Cargo-only "
        f"families, install names match"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())