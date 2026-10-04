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
import re
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

# An exact Rust *release* is a full `major.minor.patch`. `stable`, `beta`,
# `nightly`, and a bare `major.minor` are all floating: each resolves to a
# different compiler over time, so rebuilding one tag can produce different
# bytes. A required target that floats makes the whole release irreproducible,
# which is the gap M010D recorded and C014 closes.
EXACT_RUST = re.compile(r"\d+\.\d+\.\d+")

# Rust channel names that look like a version to a careless reader but are not
# one. Listed for the error message, not for matching.
FLOATING_EXAMPLES = ("stable", "beta", "nightly", "1.99", "1.99.0-beta.1")


def version_tuple(value: str) -> tuple[int, ...]:
    return tuple(int(part) for part in value.split("."))


def check_release_toolchains(pack: dict, workflow: str, msrv: str | None) -> list[str]:
    """Every required target must be built by one exact, recorded Rust release.

    Three separate things are checked, because any one of them can be true
    while the release is still irreproducible:

    1. each target pins a full `major.minor.patch`, not a channel;
    2. they all pin the *same* one, so a release has a single producer fact to
       record and re-verify;
    3. the rendered workflow actually uses it, so editing `pack.toml` without
       regenerating the workflow cannot leave a floating `+stable` in the build
       command.
    """
    problems: list[str] = []
    pinned: dict[str, str] = {}

    for entry in pack.get("targets", []):
        triple = str(entry.get("target", "<unnamed>"))
        support = str(entry.get("support", "required"))
        rust = (entry.get("toolchain") or {}).get("rust")
        if not isinstance(rust, str) or not rust.strip():
            problems.append(f"target {triple} declares no rust toolchain in pack.toml")
            continue
        if not EXACT_RUST.fullmatch(rust):
            problems.append(
                f"target {triple} pins rust = {rust!r}, which is not an exact "
                f"release. A rebuild of the same tag must use one recorded "
                f"compiler; use a full major.minor.patch (not "
                f"{', '.join(FLOATING_EXAMPLES[:3])} or a bare major.minor)"
            )
            continue
        if support == "required":
            pinned[triple] = rust
            if msrv and version_tuple(rust) < version_tuple(msrv):
                problems.append(
                    f"target {triple} builds with rust {rust}, which is older "
                    f"than the declared MSRV {msrv}"
                )

    distinct = sorted(set(pinned.values()))
    if len(distinct) > 1:
        problems.append(
            "required targets pin different Rust releases ("
            + ", ".join(f"{triple}={rust}" for triple, rust in sorted(pinned.items()))
            + "); one release must have one producer toolchain"
        )
    if not pinned:
        problems.append("no required target declares a verified rust toolchain")
        return problems

    release_rust = distinct[0]
    for triple, rust in sorted(pinned.items()):
        if f'toolchain: "{rust}"' not in workflow:
            problems.append(
                f"generated workflow does not set up {rust} for {triple}; the "
                f"rendered workflow has drifted from pack.toml"
            )
        if "+'stable'" in workflow or 'toolchain: "stable"' in workflow:
            problems.append(
                "generated workflow still resolves a floating `stable` toolchain; "
                "regenerate it from the pinned pack.toml"
            )
            break
    return problems


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def fail(message: str) -> None:
    errors.append(message)


def job_blocks(workflow: str) -> dict[str, str]:
    """Split a workflow into `job id -> job text` by two-space job keys."""
    jobs: dict[str, str] = {}
    current: str | None = None
    buffer: list[str] = []
    for line in workflow.splitlines():
        match = re.match(r"^  ([A-Za-z_][\w-]*):\s*$", line)
        if match:
            if current is not None:
                jobs[current] = "\n".join(buffer)
            current, buffer = match.group(1), [line]
            continue
        if current is not None:
            buffer.append(line)
    if current is not None:
        jobs[current] = "\n".join(buffer)
    return jobs


def top_level_permissions(workflow: str) -> str:
    """The workflow-wide `permissions:` block, which is the default for every job."""
    match = re.search(r"^permissions:\n((?:  .*\n)+)", workflow, re.MULTILINE)
    return match.group(1) if match else ""


def check_workflow_cannot_publish(workflow: str) -> list[str]:
    """The candidate-build workflow may stage a draft and must not publish.

    Staging a GitHub **draft** is a write, so the staging job legitimately holds
    `contents: write`; requiring the whole workflow to be read-only would be
    wrong, and the rule below is narrower than "no write scope" on purpose.
    What must be impossible is publishing: promoting the draft to a public
    release. So:

    1. the workflow-wide default stays read-only, so no job gets write scope by
       accident;
    2. only a job that actually stages a draft may request write scope, and it
       must be a staging job;
    3. nothing anywhere invokes a publish capability.

    Together these mean the workflow can create a draft and cannot make it
    public.
    """
    problems: list[str] = []

    top = top_level_permissions(workflow)
    if "contents: write" in top:
        problems.append(
            "workflow-wide permissions grant `contents: write`; the default for "
            "every job must be read-only so write scope is only ever requested "
            "deliberately by the staging job"
        )
    elif top and "contents: read" not in top:
        problems.append(
            "workflow-wide permissions do not state `contents: read`; the "
            "default token scope is then implicit"
        )

    for job, body in job_blocks(workflow).items():
        if "contents: write" not in body:
            continue
        if "_stage-github-draft" not in body:
            problems.append(
                f"job {job} requests `contents: write` but does not stage a "
                f"GitHub draft; write scope must never be held by a job that "
                f"does not need it to stage the draft"
            )
        elif job != "stage":
            problems.append(
                f"job {job} stages a draft; draft staging is the `stage` job's "
                f"contracted role, so an unexpected writer is a drift"
            )

    for pattern, description in (
        (r"softprops/action-gh-release", "a GitHub release publishing action"),
        (r"gh\s+release\s+(create|edit|upload)", "the `gh release` client"),
        (r"--draft[= ]*false", "an explicit publish (non-draft) release request"),
        (r"^\s*(?:-\s*)?(?:run|name):.*\bpublish\b", "a step named or invoking publish"),
    ):
        if re.search(pattern, workflow, re.MULTILINE | re.IGNORECASE):
            problems.append(f"generated workflow contains {description}")
    if "_stage-github-draft" not in workflow:
        problems.append(
            "generated workflow does not stage a GitHub draft; staging the "
            "draft is the workflow's only release-side effect"
        )
    return problems


def self_test() -> int:
    """Prove the exact-toolchain rule rejects each shape it exists to reject.

    A rule that has only ever been observed to pass has not been shown to work.
    These are the regressions the rule was written for, including the one that
    actually shipped: `rust = "stable"` on every required target.
    """
    def pack_with(rust: str, second: str | None = None) -> dict:
        targets = [{"target": "a", "support": "required", "toolchain": {"rust": rust}}]
        if second is not None:
            targets.append(
                {"target": "b", "support": "required", "toolchain": {"rust": second}}
            )
        return {"targets": targets}

    def workflow_with(rust: str) -> str:
        return f'          toolchain: "{rust}"\n'

    cases: list[tuple[str, dict, str, bool]] = [
        ("floating stable is rejected", pack_with("stable"), workflow_with("1.99.0"), True),
        ("a bare major.minor is rejected", pack_with("1.99"), workflow_with("1.99.0"), True),
        ("nightly is rejected", pack_with("nightly"), workflow_with("1.99.0"), True),
        (
            "a prerelease is rejected",
            pack_with("1.99.0-beta.1"),
            workflow_with("1.99.0"),
            True,
        ),
        (
            "mismatched builders are rejected",
            pack_with("1.99.0", "1.98.1"),
            workflow_with("1.99.0"),
            True,
        ),
        (
            "a workflow still on stable is rejected even when pack.toml is pinned",
            pack_with("1.99.0"),
            workflow_with("stable"),
            True,
        ),
        (
            "a builder older than the MSRV is rejected",
            pack_with("1.80.0"),
            workflow_with("1.80.0"),
            True,
        ),
        (
            "one exact builder everywhere is accepted",
            pack_with("1.99.0", "1.99.0"),
            workflow_with("1.99.0"),
            False,
        ),
    ]

    failures: list[str] = []
    for name, pack, workflow, expect_problem in cases:
        problems = check_release_toolchains(pack, workflow, "1.89")
        if bool(problems) != expect_problem:
            verdict = "rejected" if problems else "accepted"
            wanted = "rejected" if expect_problem else "accepted"
            failures.append(f"{name}: expected {wanted}, got {verdict} ({problems or 'no problem'})")
            print(f"  FAIL {name}: expected {wanted}, got {verdict}")
        else:
            print(f"  ok   {name}")

    # The publish-capability rule gets the same treatment: the real shape (a
    # read-only default with one draft-staging job holding write scope) must be
    # accepted, and each way of actually publishing must be rejected.
    draft_only = (
        "permissions:\n  contents: read\n"
        "jobs:\n"
        "  build:\n"
        "    runs-on: ubuntu-latest\n"
        "    permissions:\n      contents: read\n"
        "  stage:\n"
        "    runs-on: ubuntu-latest\n"
        "    permissions:\n      contents: write\n"
        '    steps:\n      - run: "eggpack ci _stage-github-draft"\n'
    )
    publish_cases: list[tuple[str, str, bool]] = [
        ("a read-only default with one draft-staging writer is accepted", draft_only, False),
        (
            "a write-scope default is rejected",
            draft_only.replace("permissions:\n  contents: read", "permissions:\n  contents: write", 1),
            True,
        ),
        (
            "a write-scope build job that stages nothing is rejected",
            draft_only.replace(
                "  build:\n    runs-on: ubuntu-latest\n    permissions:\n      contents: read\n",
                "  build:\n    runs-on: ubuntu-latest\n    permissions:\n      contents: write\n",
            ),
            True,
        ),
        (
            "a release publishing action is rejected",
            draft_only + "      - uses: softprops/action-gh-release@v2\n",
            True,
        ),
        (
            "the gh release client is rejected",
            draft_only + "      - run: gh release create v0.1.2\n",
            True,
        ),
        (
            "an explicit non-draft publish is rejected",
            draft_only + "      - run: eggpack ci _stage-github-draft --draft=false\n",
            True,
        ),
        (
            "a workflow that never stages a draft is rejected",
            "permissions:\n  contents: read\njobs:\n  build:\n    runs-on: ubuntu-latest\n",
            True,
        ),
    ]
    for name, workflow, expect_problem in publish_cases:
        problems = check_workflow_cannot_publish(workflow)
        if bool(problems) != expect_problem:
            verdict = "rejected" if problems else "accepted"
            wanted = "rejected" if expect_problem else "accepted"
            failures.append(f"{name}: expected {wanted}, got {verdict} ({problems or 'no problem'})")
            print(f"  FAIL {name}: expected {wanted}, got {verdict}")
        else:
            print(f"  ok   {name}")

    if failures:
        print(
            f"check-release-contract: self test FAILED ({len(failures)} case(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "check-release-contract: self test passed; the toolchain rule "
        "rejects every floating shape and the publish rule rejects every "
        "publishing capability"
    )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()

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

    # 8. The release builder must be one exact, recorded Rust release. This is
    #    a *producer* fact and is deliberately not the MSRV: MSRV stays the
    #    `rust-version` in Cargo.toml, and the exact builder is whatever
    #    qualified the shipped bytes.
    for problem in check_release_toolchains(
        load_toml(EGG / "pack.toml"), workflow, cargo["package"].get("rust-version")
    ):
        fail(problem)

    # 9. The candidate build stages a draft and never publishes.
    for problem in check_workflow_cannot_publish(workflow):
        fail(problem)

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
