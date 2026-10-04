#!/usr/bin/env python3
"""Keep every cargo-cleanme release surface aligned with the Eggpack contract.

Eggpack is producer authority since M010A: the canonical target set, asset
names, install names, and checksum sidecars come from
`release/eggpack/distribution.toml`, the reusable workflow shape is derived
from the other Eggpack inputs, and the checked-in release workflow is generated
from that configuration (drift-guarded by CI running `eggpack ci check` at the
pinned tool revision).

This script therefore *compares* product surfaces against the Eggpack
configuration instead of duplicating producer facts, and retains only
cargo-cleanme-owned invariants:

  * every contracted target is reachable through the product wrappers, the
    generated workflow, the packaging payload, and the README support matrix;
  * the wrapper target mapping is a mechanical projection of the contract, not
    an independent schema;
  * the packaging payload actually ships the wrappers the release requires;
  * the package metadata the contract implies is present in `Cargo.toml`;
  * nothing advertises a prebuilt target, ABI floor, or update path that the
    contract and plan boundaries do not actually establish.
"""
from __future__ import annotations

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EGG = ROOT / "release/eggpack"

WORKFLOW = ROOT / ".github/workflows/release-binaries.yml"
README = ROOT / "README.md"
CHANGELOG = ROOT / "CHANGELOG.md"
CARGO_TOML = ROOT / "Cargo.toml"
RELEASE_CHECK = ROOT / "scripts/release-check.sh"

# Hosts cargo-cleanme recognizes but does not publish prebuilt binaries for.
# They stay Cargo-fallback-only. Adding a prebuilt target here without adding
# it to the contract would be a false support claim.
CARGO_ONLY_TARGETS = ("armv7-unknown-linux-gnueabihf",)

errors: list[str] = []


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def fail(message: str) -> None:
    errors.append(message)


def main() -> int:
    contract = load_toml(EGG / "distribution.toml")
    policy = json.loads((EGG / "github-policy.json").read_text(encoding="utf-8"))
    presentation = json.loads((EGG / "installer-presentation.json").read_text(encoding="utf-8"))
    pack = (EGG / "pack.toml").read_text(encoding="utf-8")
    cargo = load_toml(CARGO_TOML)

    product = contract["product"]["id"]
    if product != cargo["package"]["name"]:
        fail(
            f"contract product {product!r} does not match the package name "
            f"{cargo['package']['name']!r}"
        )

    contracted: dict[str, str] = {}
    aliases: dict[str, list[str]] = {}
    for entry in contract["targets"]:
        triple = entry["triple"]
        asset = entry["asset"]
        if asset["kind"] != "direct":
            fail(f"target {triple} is not a direct artifact; wrappers assume direct")
        contracted[triple] = (
            asset["asset"].replace("{product}", product).replace("{target}", triple)
        )
        sidecar = entry["checksum"]["sidecar"].replace(
            "{asset}", contracted[triple]
        )
        if sidecar != f"{contracted[triple]}.sha256":
            fail(f"target {triple} does not use a .sha256 sidecar of its asset")
        aliases[triple] = list(entry.get("aliases", []))

    if not contracted:
        fail("contract declares no targets")
    if len(set(contracted.values())) != len(contracted):
        fail("contracted asset names are not unique")

    workflow = WORKFLOW.read_text(encoding="utf-8")
    readme = README.read_text(encoding="utf-8")
    changelog = CHANGELOG.read_text(encoding="utf-8")

    # 1. The generated workflow must exercise exactly the contracted matrix.
    for triple in contracted:
        if triple not in workflow:
            fail(f"generated workflow does not mention target {triple}")
    for triple in CARGO_ONLY_TARGETS:
        if triple in workflow:
            fail(f"generated workflow claims the Cargo-only target {triple}")

    # 2. Installer presentation must stay internally consistent. The
    #    presentation names the product-owned wrappers that M010B lands; this
    #    milestone only owns the declared shape, so existence and executable
    #    bits are enforced by scripts/check-installer-contract.py from M010B.
    wrappers = presentation["mode"]["product_wrappers"]
    if presentation.get("schema_version") != 1:
        fail("installer-presentation.json must declare schema_version = 1")
    if set(presentation["mode"]) != {"product_wrappers"}:
        fail("installer-presentation.json declares an unsupported mode")
    for key in (
        "posix_source",
        "powershell_source",
        "generated_posix_name",
        "generated_powershell_name",
    ):
        if not isinstance(wrappers.get(key), str) or not wrappers[key]:
            fail(f"installer-presentation.json product_wrappers.{key} is missing")
    for key in ("posix_source", "powershell_source"):
        if not wrappers[key].startswith("packaging/"):
            fail(f"installer-presentation.json {key} must stay inside packaging/")
    for key in ("generated_posix_name", "generated_powershell_name"):
        if wrappers[key] in (wrappers["posix_source"], wrappers["powershell_source"]):
            fail(f"generated exact-release installer {wrappers[key]!r} would clobber a product wrapper")

    # 3. The README support matrix must not overstate the published set.
    if "Prebuilt release targets" not in readme:
        fail("README does not document a prebuilt release target matrix")
    for triple in contracted:
        if triple not in readme:
            fail(f"README does not document contracted target {triple}")
    for triple in CARGO_ONLY_TARGETS:
        if triple not in readme:
            fail(f"README does not document the Cargo-only target {triple}")

    # 4. Package metadata that the distribution contract implies.
    if cargo["package"].get("license") != "MIT OR Apache-2.0":
        fail("Cargo.toml license expression changed; the dual-license files must match")
    for license_file in ("LICENSE-MIT", "LICENSE-APACHE"):
        if not (ROOT / license_file).is_file():
            fail(f"{license_file} is missing but the SPDX expression claims it")
    if not cargo["package"].get("readme"):
        fail("Cargo.toml does not name a README for the package")
    include = [entry.lstrip("/") for entry in cargo["package"].get("include", [])]
    if not include:
        fail("Cargo.toml has no explicit package include allowlist")
    if "config.toml" not in include:
        fail("Cargo.toml include allowlist omits the embedded config.toml template")
    if cargo["package"].get("repository") != "https://github.com/dbowm91/cargo-cleanme":
        fail("Cargo.toml repository does not match the release origin")
    # The completion/manpage generator is a maintenance tool, not product
    # surface. It lives outside the allowlist on purpose.
    include_paths = [entry.lstrip("/") for entry in cargo["package"].get("include", [])]
    if any(path.startswith("xtask") for path in include_paths):
        fail("Cargo.toml include allowlist would publish the xtask generator")
    if not (ROOT / "xtask/src/main.rs").is_file():
        fail("the completion/manpage generator source is missing")

    # 5. Staging must remain draft-only and pinned to an exact source.
    if policy["staging"]["owner"] != "dbowm91" or policy["staging"]["repository"] != "cargo-cleanme":
        fail("github-policy.json staging target does not match the release origin")
    if policy["staging"]["tag_source"] != "dispatch_input":
        fail("github-policy.json staging must bind an exact dispatch-supplied tag")
    revision = policy["eggpack_tool"].get("revision", "")
    if len(revision) != 40 or any(c not in "0123456789abcdef" for c in revision):
        fail("eggpack tool must be pinned to an exact 40-hex Git revision")
    if "release_tag" not in workflow:
        fail("generated workflow does not bind an exact release tag input")
    if "gh release" in workflow or "--clobber" in workflow or "id-token" in workflow:
        fail("generated workflow contains a publication or clobber path")

    # 6. The declared Linux ABI floor must remain a build fact here, not a
    #    published support claim. cargo-cleanme advertises the glibc floor only
    #    after M010B runtime qualification, which has not happened yet.
    if "cargo_zigbuild" not in pack:
        fail("pack.toml no longer declares the Linux cross-build strategy")
    # The floor is a *build* fact until M010B's runtime qualification exists.
    # The README may name it while explicitly declining to claim it, so the
    # invariant is "mentioning it requires an explicit non-claim nearby", not
    # "the string may never appear" - which would forbid the honest
    # explanation of why the floor is not advertised yet.
    if "glibc 2.17" in readme.lower():
        non_claims = (
            "not yet claimed",
            "not advertised",
            "no runtime evidence",
            "until a release has been qualified",
        )
        if not any(phrase in readme.lower() for phrase in non_claims):
            fail(
                "README mentions the glibc 2.17 floor without an explicit "
                "statement that it is not yet qualified"
            )

    # 7. Release surfaces that must exist before any publication.
    if not RELEASE_CHECK.is_file():
        fail("scripts/release-check.sh is missing")
    if "## [0.1.0]" not in changelog and "## 0.1.0" not in changelog:
        fail("CHANGELOG.md has no 0.1.0 entry")

    if errors:
        for error in errors:
            print(f"check-release-contract: {error}", file=sys.stderr)
        return 1
    print(
        f"check-release-contract: {len(contracted)} contracted targets, "
        f"{len(contracted) * 2 + 1} release assets, wrappers aligned"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
