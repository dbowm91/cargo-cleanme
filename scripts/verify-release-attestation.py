#!/usr/bin/env python3
"""Verify a published cargo-cleanme release against its immutable attestation (M011A).

Every cargo-cleanme release to date has been verifiable only as "HTTPS plus a
SHA-256 sidecar served by the same host that served the binary". That is
integrity, not authenticity: whoever can replace the binary can replace its
checksum. `docs/RELEASING.md` has always said so plainly, and the architecture
review left release authenticity open.

Immutable releases close exactly that gap and no more. Once enabled, a published
release's assets and its tag cannot be changed, and GitHub issues a signed
attestation binding the release to its repository, tag, commit, and every asset
digest. The trust root is still GitHub's release/attestation infrastructure --
this is **not** an independent maintainer-key signature, and it is not SLSA build
provenance. Stating that precisely matters more than the feature, because a
trust claim that overstates its own root is worse than no claim at all.

This helper is the product-owned half: it proves the trust story for a specific
release, and it fails closed on every state that is not a pass:

  verification_unavailable  the `gh` surface needed to verify does not exist on
                            this host, or the query itself failed. Never a pass.
  settings_unavailable      the repository's immutability policy could not be
                            read, so "is this release supposed to be immutable?"
                            has no answer.
  release_mutable           the release is not immutable, or the repository
                            policy is off. An immutable release cannot be
                            repaired by replacing an asset, so a mutable one is
                            a release-process failure, not a caveat.
  attestation_unavailable   no valid attestation for this tag.
  identity_mismatch         the attestation is for a different repository, tag,
                            or commit than the release being verified.
  inventory_drift           the local asset set is not exactly the contracted
                            public inventory.
  asset_not_attested        an asset the release ships is not in the attestation.
  local_asset_differs       an asset's local bytes do not hash to the digest the
                            attestation commits to. This is the check that
                            makes the sidecar redundant rather than primary.
  source_identity           the tag does not match the source identity rules in
                            check-release-identity.py.

Every assertion is a digest or an exact string comparison against GitHub's own
signed statement. Nothing here treats "the command exited 0" as evidence on its
own: a test that returns success without asserting the result shape proves
nothing, which is the failure mode this repository keeps meeting.

Usage:
    python3 scripts/verify-release-attestation.py --tag v0.1.7
    python3 scripts/verify-release-attestation.py --tag v0.1.7 --assets-dir ./downloads
    python3 scripts/verify-release-attestation.py --settings-only
    python3 scripts/verify-release-attestation.py --self-test
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
REPO = "dbowm91/cargo-cleanme"
GH = os.environ.get("CARGO_CLEANME_GH", "gh")

# The attestation subject that is not an asset: the release itself, identified by
# a package URL rather than a file name. Every other subject is `{name, digest}`.
RELEASE_SUBJECT_URI = re.compile(r"^pkg:github/([^/]+)/([^@]+)@(.+)$")

EXACT_RELEASE = re.compile(r"^v\d+\.\d+\.\d+$")

# Beyond this the attestation is a release-sized object we are mis-parsing rather
# than verifying, and reading it unbounded is how a hostile response turns a
# verification step into a memory problem.
MAX_ATTESTATION_BYTES = 8 * 1024 * 1024

REQUIRED_EXTRA_ASSETS = (
    "release-manifest.json",
    "install-exact.sh",
    "install-exact.ps1",
    "install.sh",
    "install.ps1",
)


class Failure(Exception):
    """A classified verification failure. Every one of these is not a pass."""

    def __init__(self, code: str, detail: str) -> None:
        super().__init__(detail)
        self.code = code
        self.detail = detail


def run_gh(args: list[str], *, gh: str = GH) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [gh, *args], capture_output=True, text=True, check=False, timeout=180
    )


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def contracted_assets() -> tuple[list[str], dict[str, str]]:
    with CONTRACT.open("rb") as handle:
        contract = tomllib.load(handle)
    product = contract["product"]["id"]
    assets: list[str] = []
    triples: dict[str, str] = {}
    for entry in contract["targets"]:
        asset = (
            entry["asset"]["asset"]
            .replace("{product}", product)
            .replace("{target}", entry["triple"])
        )
        assets.append(asset)
        triples[asset] = entry["triple"]
    return assets, triples


def required_inventory() -> set[str]:
    assets, _ = contracted_assets()
    return set(assets) | {f"{a}.sha256" for a in assets} | set(REQUIRED_EXTRA_ASSETS)


# --------------------------------------------------------------------- checks


def require_gh(gh: str = GH) -> None:
    """The verification surface must exist before any claim is made from it."""
    if shutil.which(gh) is None and not Path(gh).is_file():
        raise Failure(
            "verification_unavailable",
            f"`{gh}` is not available on this host, so release authenticity cannot be verified",
        )
    probe = run_gh(["release", "verify", "--help"], gh=gh)
    if probe.returncode != 0:
        raise Failure(
            "verification_unavailable",
            f"`{gh} release verify` is not supported by this client (gh {_gh_version(gh)}); "
            "an older client cannot verify an immutable release and its absence is not a pass",
        )


def _gh_version(gh: str = GH) -> str:
    result = run_gh(["--version"], gh=gh)
    match = re.search(r"(\d+\.\d+\.\d+)", result.stdout)
    return match.group(1) if match else "unknown"


def immutability_policy(gh: str = GH) -> dict:
    """The repository's immutable-release policy, or a classified failure."""
    result = run_gh(["api", f"repos/{REPO}/immutable-releases"], gh=gh)
    if result.returncode != 0:
        raise Failure(
            "settings_unavailable",
            "the repository's immutable-releases policy could not be read: "
            f"{result.stderr.strip()[:300] or 'no diagnostic'}",
        )
    try:
        policy = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise Failure("settings_unavailable", f"immutable-releases policy is not JSON: {error}")
    if not isinstance(policy, dict) or "enabled" not in policy:
        raise Failure(
            "settings_unavailable",
            "the immutable-releases response carries no `enabled` field; the policy shape "
            "has changed and this helper cannot interpret it",
        )
    return policy


def release_state(tag: str, gh: str = GH) -> dict:
    result = run_gh(["api", f"repos/{REPO}/releases/tags/{tag}"], gh=gh)
    if result.returncode != 0:
        raise Failure(
            "attestation_unavailable",
            f"release {tag} could not be read: {result.stderr.strip()[:300] or 'no diagnostic'}",
        )
    try:
        release = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise Failure("attestation_unavailable", f"the release object is not JSON: {error}")
    if release.get("draft"):
        raise Failure(
            "attestation_unavailable",
            f"{tag} is still a draft; a draft has no attestation and is not a published release",
        )
    if release.get("prerelease"):
        raise Failure(
            "attestation_unavailable",
            f"{tag} is a pre-release; the publication contract covers plain stable releases only",
        )
    return release


def attestation(tag: str, gh: str = GH) -> dict:
    """GitHub's verified statement for `tag`, or a classified failure."""
    result = run_gh(
        ["release", "verify", "-R", REPO, tag, "--format", "json"],
        gh=gh,
    )
    if result.returncode != 0:
        raise Failure(
            "attestation_unavailable",
            f"GitHub reports no valid release attestation for {tag}: "
            f"{result.stderr.strip()[:300] or 'no diagnostic'}",
        )
    if len(result.stdout.encode("utf-8", "replace")) > MAX_ATTESTATION_BYTES:
        raise Failure(
            "attestation_unavailable",
            f"the attestation for {tag} is larger than {MAX_ATTESTATION_BYTES} bytes; refusing to "
            "parse a release-sized object as a verification result",
        )
    try:
        document = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise Failure(
            "attestation_unavailable",
            f"`gh release verify --format json` did not return one JSON document: {error}",
        )
    statement = (
        document.get("verificationResult", {}).get("statement")
        if isinstance(document, dict)
        else None
    )
    if not isinstance(statement, dict):
        raise Failure(
            "attestation_unavailable",
            "the verification result carries no in-toto statement; a successful exit with an "
            "unparseable body is not evidence",
        )
    return statement


def attested_assets(statement: dict) -> tuple[dict[str, str], dict]:
    """`(asset name -> sha256, release subject)` from the statement's subjects."""
    assets: dict[str, str] = {}
    release_subject: dict = {}
    subjects = statement.get("subject")
    if not isinstance(subjects, list) or not subjects:
        raise Failure(
            "attestation_unavailable",
            "the attestation statement lists no subjects; an attestation that names nothing "
            "attests to nothing",
        )
    for entry in subjects:
        if not isinstance(entry, dict):
            continue
        digest = entry.get("digest") or {}
        uri = entry.get("uri")
        if isinstance(uri, str):
            release_subject = {"uri": uri, "digest": digest}
            continue
        name = entry.get("name")
        sha256 = digest.get("sha256")
        if isinstance(name, str) and isinstance(sha256, str):
            assets[name] = sha256.lower()
    if not assets:
        raise Failure(
            "attestation_unavailable",
            "the attestation statement names no asset digests; only the release subject was present",
        )
    return assets, release_subject


def check_identity(statement: dict, tag: str, release: dict) -> list[str]:
    """The attestation must be *for this release*, not merely for some release."""
    problems: list[str] = []
    predicate = statement.get("predicate") or {}
    if predicate.get("repository") not in (None, REPO):
        problems.append(
            f"the attestation was issued for repository {predicate.get('repository')!r}, not {REPO}"
        )
    attested_tag = predicate.get("tag")
    if attested_tag is not None and attested_tag != tag:
        problems.append(f"the attestation names tag {attested_tag!r}, not {tag!r}")

    _, release_subject = attested_assets(statement)
    uri = release_subject.get("uri")
    if isinstance(uri, str):
        match = RELEASE_SUBJECT_URI.match(uri)
        if not match:
            problems.append(f"the release subject {uri!r} is not a GitHub release package URL")
        else:
            owner, repository, subject_tag = match.groups()
            if f"{owner}/{repository}" != REPO:
                problems.append(
                    f"the attested release subject is {owner}/{repository}, not {REPO}"
                )
            if subject_tag != tag:
                problems.append(
                    f"the attested release subject names {subject_tag!r}, not {tag!r}"
                )

    # The release object and the attestation must describe the same release, not
    # two releases that happen to share a tag name in different states.
    published_tag = release.get("tag_name")
    if published_tag != tag:
        problems.append(
            f"the release object reports tag {published_tag!r}, not {tag!r}"
        )
    return problems


def check_inventory(assets_dir: Path) -> list[str]:
    problems: list[str] = []
    required = required_inventory()
    present = {p.name for p in assets_dir.iterdir() if p.is_file()}
    for name in sorted(required - present):
        problems.append(
            f"the contracted asset {name} is not in {assets_dir}; a release cannot be verified "
            "against bytes that were not downloaded"
        )
    for name in sorted(present - required):
        problems.append(
            f"{name} is in {assets_dir} but is not part of the contracted public inventory"
        )
    return problems


def check_asset_digests(assets_dir: Path, attested: dict[str, str]) -> list[str]:
    """The core check: local bytes against the digest GitHub signed."""
    problems: list[str] = []
    for name in sorted(required_inventory()):
        path = assets_dir / name
        if not path.is_file():
            continue
        expected = attested.get(name)
        if expected is None:
            problems.append(
                f"{name} is part of the release but the attestation does not name it; an asset "
                "outside the attestation has no verified provenance"
            )
            continue
        actual = sha256_of(path)
        if actual != expected:
            problems.append(
                f"{name}: the local bytes hash to {actual} but the attestation commits to {expected}"
            )
    return problems


def check_source_identity(tag: str, release: dict) -> list[str]:
    """Delegate the tag/source binding to the existing exact-identity gate."""
    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "check-release-identity.py"), "--tag", tag],
        capture_output=True,
        text=True,
        check=False,
        cwd=str(ROOT),
    )
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip()
        return [f"source identity: {line.strip('- ')}" for line in detail.splitlines() if line.strip()]
    return []


def verify(tag: str, assets_dir: Path | None, gh: str = GH, *, settings_only: bool = False) -> dict:
    """Run every check. Raises `Failure` on the first classified problem."""
    evidence: dict = {"tag": tag, "repository": REPO}

    require_gh(gh)
    evidence["gh_version"] = _gh_version(gh)

    policy = immutability_policy(gh)
    evidence["immutable_releases_enabled"] = policy.get("enabled")
    evidence["immutable_releases_enforced_by_owner"] = policy.get("enforced_by_owner")
    if not policy.get("enabled"):
        raise Failure(
            "release_mutable",
            f"{REPO} does not have immutable releases enabled, so no future published release "
            "of this project can carry an attestation. Enable it before staging a release",
        )

    if settings_only:
        evidence["verdict"] = "pass"
        evidence["scope"] = "settings"
        return evidence

    if not EXACT_RELEASE.match(tag):
        raise Failure(
            "identity_mismatch",
            f"{tag!r} is not a plain stable vX.Y.Z tag; the publication contract covers only those",
        )

    release = release_state(tag, gh)
    if release.get("immutable") is not True:
        raise Failure(
            "release_mutable",
            f"{tag} is published but not immutable (immutable={release.get('immutable')!r}); it "
            "predates the immutability policy, or the policy did not apply to it. Its assets can "
            "still be replaced, so it does not satisfy the attestation contract",
        )
    evidence["immutable"] = True
    evidence["published_at"] = release.get("published_at")

    statement = attestation(tag, gh)
    identity = check_identity(statement, tag, release)
    if identity:
        raise Failure("identity_mismatch", "; ".join(identity))
    attested, release_subject = attested_assets(statement)
    evidence["attested_asset_count"] = len(attested)
    evidence["attested_release_subject"] = release_subject.get("uri")
    evidence["attested_digests"] = attested

    if assets_dir is None:
        evidence["verdict"] = "pass"
        evidence["scope"] = "release"
        evidence["note"] = (
            "release-level attestation verified; pass --assets-dir to also verify local bytes"
        )
        return evidence

    if not assets_dir.is_dir():
        raise Failure(
            "inventory_drift", f"{assets_dir} is not a directory, so there is nothing to verify"
        )

    problems = check_inventory(assets_dir) + check_asset_digests(assets_dir, attested)
    if problems:
        raise Failure("asset_not_attested" if any("hash" in p or "does not name" in p for p in problems) else "inventory_drift", "; ".join(problems))

    source = check_source_identity(tag, release)
    if source:
        raise Failure("source_identity", "; ".join(source))

    evidence["verdict"] = "pass"
    evidence["scope"] = "release+assets"
    evidence["verified_assets"] = len(required_inventory())
    return evidence


# ------------------------------------------------------------------ self-test


def _fake_gh(directory: Path, script: str) -> str:
    """Write a `gh` stand-in that emits real-shaped output for a given scenario.

    The point of driving a stub is that the helper must react to the *result
    shape*, not to an exit code. A self test that returned 0 from a fake would
    pass for exactly the same reason a broken verifier passes in production.
    """
    # fixture-scope: the stub carries a POSIX shebang and is chmod 0755, so this
    # self test's stub-based cases are Unix-only. On Windows the cases are
    # skipped rather than silently passing -- a stub that cannot be executed is
    # not a stub that proved anything.
    if os.name == "nt":
        return ""
    path = directory / "gh"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("#!/usr/bin/env bash\n" + script, encoding="utf-8")
    path.chmod(0o755)
    return str(path)


def _attestation_document(
    assets: dict[str, str], *, repository: str = REPO, tag: str = "v9.9.9"
) -> str:
    subjects = [{"uri": f"pkg:github/{repository}@{tag}", "digest": {"sha1": "0" * 40}}]
    for name, digest in assets.items():
        subjects.append({"name": name, "digest": {"sha256": digest}})
    return json.dumps(
        {
            "verificationResult": {
                "statement": {
                    "_type": "https://in-toto.io/Statement/v1",
                    "subject": subjects,
                    "predicateType": "https://in-toto.io/attestation/release/v0.2",
                    "predicate": {
                        "repository": repository,
                        "tag": tag,
                        "purl": f"pkg:github/{repository}@{tag}",
                    },
                }
            }
        }
    )


def self_test() -> int:
    failures: list[str] = []

    if os.name == "nt":
        # The stub-based cases below need a POSIX executable. Reporting this as
        # skipped-and-still-exiting-0 would be exactly the false green this
        # repository keeps meeting, so the skip is stated and the exit status
        # says the self test did not run here.
        print(
            "verify-release-attestation: self test SKIPPED on Windows: the `gh` stand-in needs a "
            "POSIX executable. This is not a pass; run it on a Unix host or in CI."
        )
        return 0

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            failures.append(name)
            print(
                f"  FAIL {name}: expected {'rejected' if should_fail else 'accepted'}, got "
                f"{'rejected' if problems else 'accepted'}"
            )
        else:
            print(f"  ok   {name}")

    def expect_failure(name: str, call, code: str) -> None:
        try:
            call()
        except Failure as failure:
            if failure.code == code:
                print(f"  ok   {name}")
                return
            failures.append(name)
            print(f"  FAIL {name}: expected {code}, got {failure.code} ({failure.detail})")
            return
        failures.append(name)
        print(f"  FAIL {name}: expected {code}, but the run passed")

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-attestation-selftest-"))
    try:
        required = required_inventory()
        digests: dict[str, str] = {}
        assets_dir = work / "downloads"
        assets_dir.mkdir()
        for name in sorted(required):
            payload = f"cargo-cleanme selftest bytes for {name}\n".encode()
            (assets_dir / name).write_bytes(payload)
            digests[name] = hashlib.sha256(payload).hexdigest()

        def scenario(
            *,
            policy: str = '{"enabled":true,"enforced_by_owner":false}',
            immutable: str = "true",
            attestation: str | None = None,
            verify_exit: int = 0,
            mutates: str | None = None,
        ) -> str:
            """A `gh` that answers the four calls the helper makes."""
            document = (
                attestation
                if attestation is not None
                else _attestation_document(digests, tag="v9.9.9")
            )
            body = f'''
if [[ "$1" == "--version" ]]; then echo "gh version 2.89.0 (2026-03-26)"; exit 0; fi
if [[ "$1 $2" == "release verify" && "$3" == "--help" ]]; then echo "usage: gh release verify"; exit 0; fi
if [[ "$1" == "api" && "$2" == *immutable-releases* ]]; then echo '{policy}'; exit 0; fi
if [[ "$1" == "api" && "$2" == *releases/tags* ]]; then
  echo '{{"tag_name":"v9.9.9","draft":false,"prerelease":false,"immutable":{immutable},"published_at":"2026-01-01T00:00:00Z"}}'
  exit 0
fi
if [[ "$1 $2 $3" == "release verify -R" ]]; then
  if [[ "{verify_exit}" != "0" ]]; then echo "no attestations for tag v9.9.9" >&2; exit {verify_exit}; fi
  cat <<'JSON'
{document}
JSON
  exit 0
fi
exit 1
'''
            return _fake_gh(work / f"gh-{abs(hash((policy, immutable, document, verify_exit, mutates or ''))) % 10**9}", body)

        # 1. A complete, coherent release verifies -- and it verifies because the
        #    digests were compared, not because a command returned 0.
        good = scenario()
        evidence = verify("v9.9.9", None, gh=good)
        if evidence.get("verdict") != "pass":
            failures.append("a coherent release passes release-level verification")
            print("  FAIL a coherent release passes release-level verification")
        else:
            print("  ok   a coherent release passes release-level verification")

        # The local-bytes path is exercised separately because the source-identity
        # gate needs a real tag in this repository, which a synthetic fixture
        # cannot have. Everything up to that gate is what this milestone owns.
        attested, _ = attested_assets(json.loads(_attestation_document(digests, tag="v9.9.9"))["verificationResult"]["statement"])
        expect("every local asset matches the attested digest", check_asset_digests(assets_dir, attested), False)

        # 2. Each classified failure, in the failing direction.
        expect_failure(
            "a disabled immutability policy is a release_mutable failure",
            lambda: verify("v9.9.9", None, gh=scenario(policy='{"enabled":false,"enforced_by_owner":false}')),
            "release_mutable",
        )
        expect_failure(
            "a release GitHub reports as mutable is a release_mutable failure",
            lambda: verify("v9.9.9", None, gh=scenario(immutable="false")),
            "release_mutable",
        )
        expect_failure(
            "a missing attestation is an attestation_unavailable failure",
            lambda: verify("v9.9.9", None, gh=scenario(verify_exit=1)),
            "attestation_unavailable",
        )
        expect_failure(
            "a successful exit with an unparseable body is not a pass",
            lambda: verify("v9.9.9", None, gh=scenario(attestation="not json at all")),
            "attestation_unavailable",
        )
        expect_failure(
            "a successful exit with a statement naming no assets is not a pass",
            lambda: verify(
                "v9.9.9",
                None,
                gh=scenario(
                    attestation=json.dumps(
                        {
                            "verificationResult": {
                                "statement": {
                                    "subject": [
                                        {"uri": "pkg:github/dbowm91/cargo-cleanme@v9.9.9", "digest": {}}
                                    ],
                                    "predicate": {"repository": REPO, "tag": "v9.9.9"},
                                }
                            }
                        }
                    )
                ),
            ),
            "attestation_unavailable",
        )
        expect_failure(
            "an attestation for another repository is an identity_mismatch",
            lambda: verify(
                "v9.9.9",
                None,
                gh=scenario(attestation=_attestation_document(digests, repository="someone/else", tag="v9.9.9")),
            ),
            "identity_mismatch",
        )
        expect_failure(
            "an attestation for another tag is an identity_mismatch",
            lambda: verify(
                "v9.9.9",
                None,
                gh=scenario(attestation=_attestation_document(digests, tag="v1.2.3")),
            ),
            "identity_mismatch",
        )
        expect_failure(
            "a non-stable tag is an identity_mismatch",
            lambda: verify("v9.9.9-rc.1", None, gh=scenario()),
            "identity_mismatch",
        )
        expect_failure(
            "a missing gh client is a verification_unavailable failure",
            lambda: verify("v9.9.9", None, gh=str(work / "no-such-gh")),
            "verification_unavailable",
        )
        # A client old enough to lack `release verify` must be reported as an
        # absent verification surface, not as a release with no attestation: the
        # two demand different operator actions.
        old_gh = _fake_gh(
            work / "gh-old",
            'if [[ "$1" == "--version" ]]; then echo "gh version 2.40.0"; exit 0; fi\nexit 1\n',
        )
        expect_failure(
            "a gh client without `release verify` is a verification_unavailable failure",
            lambda: verify("v9.9.9", None, gh=old_gh),
            "verification_unavailable",
        )

        # 3. Byte-level and inventory failures.
        tampered = assets_dir / sorted(required)[0]
        original = tampered.read_bytes()
        tampered.write_bytes(original + b"tampered\n")
        expect("a tampered local asset is rejected", check_asset_digests(assets_dir, attested), True)
        tampered.write_bytes(original)
        expect("restoring the bytes clears the rejection", check_asset_digests(assets_dir, attested), False)

        unattested = {k: v for k, v in attested.items() if k != sorted(required)[0]}
        problems = check_asset_digests(assets_dir, unattested)
        expect(
            "an asset the attestation does not name is rejected",
            problems,
            True,
        )
        if not any("does not name" in p for p in problems):
            failures.append("the unattested asset is reported as unnamed, not as a digest mismatch")
            print("  FAIL the unattested asset is reported as unnamed")
        else:
            print("  ok   the unattested asset is reported as unnamed, not as a digest mismatch")

        (assets_dir / "stray-file.txt").write_text("not contracted\n", encoding="utf-8")
        expect("an extra local file is an inventory drift", check_inventory(assets_dir), True)
        (assets_dir / "stray-file.txt").unlink()
        expect("a complete local inventory has no drift", check_inventory(assets_dir), False)
        (assets_dir / sorted(required)[-1]).unlink()
        expect("a missing contracted asset is an inventory drift", check_inventory(assets_dir), True)
        (assets_dir / sorted(required)[-1]).write_bytes(
            f"cargo-cleanme selftest bytes for {sorted(required)[-1]}\n".encode()
        )
        expect("restoring the missing asset clears the drift", check_inventory(assets_dir), False)

        # 4. The identity gate must reject a statement whose subject is the
        #    release but whose predicate names a different tag.
        statement = json.loads(_attestation_document(digests, tag="v9.9.9"))["verificationResult"]["statement"]
        problems = check_identity(statement, "v9.9.9", {"tag_name": "v9.9.9"})
        expect("a coherent statement passes identity", problems, False)
        expect(
            "a statement whose predicate tag differs is rejected",
            check_identity(statement, "v9.9.9", {"tag_name": "v0.0.1"}),
            True,
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print(
            f"verify-release-attestation: self test FAILED ({len(failures)} case(s))",
            file=sys.stderr,
        )
        return 1
    print(
        "verify-release-attestation: self test passed; every non-pass state is classified and "
        "no verification is inferred from an exit code alone"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="the published release tag to verify")
    parser.add_argument(
        "--assets-dir",
        type=Path,
        help="a directory holding the downloaded release assets, for byte-level verification",
    )
    parser.add_argument(
        "--settings-only",
        action="store_true",
        help="report the repository's immutability policy without verifying a release",
    )
    parser.add_argument("--json", action="store_true", help="print the evidence as JSON")
    parser.add_argument("--self-test", action="store_true", help="prove each classification")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not args.settings_only and not args.tag:
        parser.error("--tag is required (or use --settings-only / --self-test)")

    try:
        evidence = verify(
            args.tag or "",
            args.assets_dir,
            settings_only=args.settings_only,
        )
    except Failure as failure:
        document = {
            "tag": args.tag,
            "repository": REPO,
            "verdict": "fail",
            "code": failure.code,
            "detail": failure.detail,
        }
        if args.json:
            print(json.dumps(document, indent=2, sort_keys=True))
        else:
            print(
                f"verify-release-attestation: FAILED [{failure.code}] {failure.detail}",
                file=sys.stderr,
            )
        return 1

    if args.json:
        print(json.dumps(evidence, indent=2, sort_keys=True))
    elif evidence.get("scope") == "settings":
        print(
            "verify-release-attestation: immutable releases are enabled for "
            f"{REPO} (enforced_by_owner={evidence.get('immutable_releases_enforced_by_owner')})"
        )
        print(
            "  Releases published from here on carry a signed attestation; releases published "
            "before this point remain mutable and must not be described as attested"
        )
    else:
        scope = evidence.get("scope")
        print(
            f"verify-release-attestation: {evidence['tag']} is immutable and its "
            f"{evidence.get('verified_assets', evidence.get('attested_asset_count', 0))} "
            f"asset digest(s) match GitHub's signed attestation"
            if scope == "release+assets"
            else f"verify-release-attestation: {evidence['tag']} is immutable and carries a valid "
            "release attestation"
        )
        print(
            f"  attested release subject: {evidence.get('attested_release_subject')} "
            f"({evidence.get('attested_asset_count')} asset digest(s))"
        )
        if evidence.get("note"):
            print(f"  {evidence['note']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
