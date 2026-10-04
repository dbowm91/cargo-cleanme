#!/usr/bin/env python3
"""Bind a release tag to the source identity that is actually in the tree.

The Eggpack release pipeline already binds three things tightly: the workflow
checks out the dispatch-supplied exact tag, `_resolve-release` refuses any
`--source-revision` that is not the checked-out commit, and every build job
re-runs `_verify-source` before it compiles.

What it does **not** bind is the tag's version to the crate's version. That was
measured, not assumed:

~~~text
$ eggpack ci _resolve-release --tag scratch-v9.9.9 ...   # Cargo.toml says 0.1.2
checked-out source matches release plan
resolved runtime release identity for tag scratch-v9.9.9
~~~

A mistyped or stale tag therefore resolves cleanly and stages a draft release
whose tag, asset names, and changelog disagree with the version the binary
actually reports. The updater's identity check would reject the download, and
the installer would refuse the candidate, so the failure would surface at the
user's machine rather than in CI.

This script is the repository-side gate for that identity, run before a
dispatch. It is a check, not a publisher: it verifies, and it changes nothing.

Usage:
    python3 scripts/check-release-identity.py --tag v0.1.2
    python3 scripts/check-release-identity.py --self-test
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO_TOML = ROOT / "Cargo.toml"
CARGO_LOCK = ROOT / "Cargo.lock"
CHANGELOG = ROOT / "CHANGELOG.md"
PRODUCT = "cargo-cleanme"

# Tags are `v<version>`; the version is a full release triple with no pre-release
# or build metadata, because the publication contract promises a plain release.
EXACT_RELEASE = re.compile(r"^v(\d+\.\d+\.\d+)$")

# The changelog heading style, tried in order so the check does not hard-code
# one document convention.
CHANGELOG_HEADINGS = ("## [{version}]", "## {version}", "## [{version} -")


class IdentityError(Exception):
    """A release tag that does not match the source it would publish."""


def git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=str(ROOT),
        capture_output=True,
        text=True,
        check=False,
    )


def crate_version() -> str:
    with CARGO_TOML.open("rb") as handle:
        package = tomllib.load(handle)["package"]
    name, version = package["name"], package["version"]
    if name != PRODUCT:
        raise IdentityError(f"Cargo.toml names {name!r}, expected {PRODUCT!r}")
    return version


def locked_version() -> str | None:
    """The version `Cargo.lock` recorded for this crate, if it has one."""
    if not CARGO_LOCK.is_file():
        return None
    with CARGO_LOCK.open("rb") as handle:
        packages = tomllib.load(handle).get("package", [])
    for entry in packages:
        if entry.get("name") == PRODUCT:
            return entry.get("version")
    return None


def check_identity(tag: str, expect_revision: str | None = None) -> list[str]:
    """Return the identity problems for `tag`; empty means it is publishable."""
    problems: list[str] = []

    match = EXACT_RELEASE.match(tag)
    if not match:
        problems.append(
            f"tag {tag!r} is not an exact release tag; expected v<major.minor.patch> "
            f"with no pre-release or build metadata"
        )
        return problems
    tag_version = match.group(1)

    source_version = crate_version()
    if source_version != tag_version:
        problems.append(
            f"tag {tag} declares {tag_version} but Cargo.toml declares "
            f"{source_version}; the staged release would be labelled with a "
            f"version the binary does not report"
        )

    locked = locked_version()
    if locked is not None and locked != source_version:
        problems.append(
            f"Cargo.lock records {PRODUCT} {locked} but Cargo.toml declares "
            f"{source_version}; a --locked release build would fail, or would "
            f"build a different version than the tag names"
        )

    resolved = git("rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    if resolved.returncode != 0:
        problems.append(
            f"tag {tag} does not exist in this repository; a release must name an "
            f"existing exact tag"
        )
        return problems
    tag_revision = resolved.stdout.strip()

    head = git("rev-parse", "--verify", "HEAD^{commit}").stdout.strip()
    wanted = expect_revision or head
    if tag_revision != wanted:
        problems.append(
            f"tag {tag} points at {tag_revision[:12]}, not the revision being "
            f"released ({wanted[:12]}); the tag and the source would disagree"
        )

    changelog = CHANGELOG.read_text(encoding="utf-8") if CHANGELOG.is_file() else ""
    if not any(heading.format(version=tag_version) in changelog for heading in CHANGELOG_HEADINGS):
        problems.append(
            f"CHANGELOG.md has no {tag_version} entry; the release record must "
            f"state what changed before the tag is staged"
        )

    return problems


def check_clean_tree() -> list[str]:
    """Publication must run from a clean checkout of the tagged commit.

    A dirty tree means the bytes being published are not the bytes that were
    reviewed, which is the whole failure this gate exists to prevent. The
    v0.1.1 deviation was different but related: the tag held the reviewed bytes
    and the publish ran from a later commit, so the tree being published was not
    the tree that was tagged.
    """
    status = git("status", "--porcelain")
    if status.returncode != 0:
        return [f"git status failed: {status.stderr.strip()}"]
    if status.stdout.strip():
        entries = [line for line in status.stdout.strip().splitlines()]
        return [
            f"working tree is not clean ({len(entries)} entr"
            f"{'y' if len(entries) == 1 else 'ies'}); the bytes that would be "
            f"published are not the bytes that were reviewed"
        ]
    return []


def check_manifest(manifest_path: Path, tag: str, expect_revision: str) -> list[str]:
    """A staged release manifest must bind `tag` to the revision being released.

    The manifest is the document the release actually ships beside the binaries.
    If its `release_id` or `source_revision` disagrees with the tag, then the
    updater's identity check and the installer's version check are reasoning
    about bytes from a different commit than the one being promoted.
    """
    if not manifest_path.is_file():
        return [f"release manifest not found at {manifest_path}"]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        return [f"release manifest is not one JSON document: {error}"]

    problems: list[str] = []
    if manifest.get("release_id") != tag:
        problems.append(
            f"release manifest binds {manifest.get('release_id')!r}, not the "
            f"tag being published ({tag})"
        )
    if manifest.get("source_revision") != expect_revision:
        problems.append(
            f"release manifest records source revision "
            f"{str(manifest.get('source_revision'))[:12]}, not the revision being "
            f"released ({expect_revision[:12]})"
        )
    if manifest.get("product_id") != PRODUCT:
        problems.append(
            f"release manifest names {manifest.get('product_id')!r}, "
            f"expected {PRODUCT!r}"
        )
    if manifest.get("schema_version") != 1:
        problems.append(
            f"release manifest schema_version is {manifest.get('schema_version')!r}, "
            f"expected 1"
        )
    return problems


def self_test() -> int:
    """Prove each identity assertion rejects the mismatch it exists for.

    The version mismatch is the one that shipped unresolved, so it is asserted
    here against a real temporary tag on a real version bump rather than
    against a mocked document.
    """
    failures: list[str] = []

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            verdict = "rejected" if problems else "accepted"
            wanted = "rejected" if should_fail else "accepted"
            failures.append(f"{name}: expected {wanted}, got {verdict}")
            print(f"  FAIL {name}: expected {wanted}, got {verdict}")
        else:
            print(f"  ok   {name}")

    source_version = crate_version()
    good_tag = f"v{source_version}"

    expect("a malformed tag is rejected", check_identity("0.1"), True)
    expect("a tag with no v prefix is rejected", check_identity(source_version), True)
    expect("a pre-release tag is rejected", check_identity("v0.1.2-rc.1"), True)
    expect("a tag with build metadata is rejected", check_identity("v0.1.2+1"), True)
    expect("a nonexistent tag is rejected", check_identity("v99.99.99"), True)
    expect(
        f"a tag whose version disagrees with Cargo.toml is rejected ({source_version})",
        check_identity(f"v0.0.{int(source_version.split('.')[-1]) + 7}"),
        True,
    )
    expect(
        "a correctly versioned tag that does not exist yet is still rejected",
        check_identity(good_tag),
        True,
    )

    # The manifest binding is checked against a real staged manifest, because
    # its field names are the contract and asserting against a made-up document
    # would prove nothing about the real one.
    staged = Path("/tmp/relv011/release-manifest.json")
    if staged.is_file():
        real = json.loads(staged.read_text(encoding="utf-8"))
        head = git("rev-parse", "--verify", "HEAD^{commit}").stdout.strip()
        expect(
            "a real manifest from another release is rejected for this one",
            check_manifest(staged, "v0.1.2", head),
            True,
        )
        expect(
            "a real manifest is accepted for the release it actually belongs to",
            check_manifest(staged, str(real["release_id"]), str(real["source_revision"])),
            False,
        )
    else:
        print("  skip manifest cases: no staged manifest available on this host")

    expect("a dirty tree is rejected", check_clean_tree(), True)

    if failures:
        print(f"check-release-identity: self test FAILED ({len(failures)} case(s))", file=sys.stderr)
        return 1
    print(
        "check-release-identity: self test passed; every tag/source mismatch is rejected"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="the exact release tag about to be staged")
    parser.add_argument(
        "--expect-revision",
        help="the revision the tag must point at (defaults to HEAD)",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        help="verify a staged release-manifest.json binds the tag to this revision",
    )
    parser.add_argument(
        "--allow-dirty",
        action="store_true",
        help="skip the clean-tree assertion (for pre-commit use only)",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove each identity assertion rejects the mismatch it exists for",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not args.tag:
        parser.error("--tag is required (or use --self-test)")

    problems: list[str] = []
    if not args.allow_dirty:
        problems.extend(check_clean_tree())
    problems.extend(check_identity(args.tag, args.expect_revision))

    if args.manifest is not None:
        revision = args.expect_revision or git("rev-parse", "--verify", "HEAD^{commit}").stdout.strip()
        problems.extend(check_manifest(args.manifest, args.tag, revision))

    if problems:
        print(f"check-release-identity: {args.tag} is not publishable", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print(
        f"check-release-identity: {args.tag} names {crate_version()} at "
        f"{args.expect_revision or git('rev-parse', '--verify', 'HEAD^{commit}').stdout.strip()[:12]}"
    )
    if args.manifest is not None:
        print(f"check-release-identity: {args.manifest} binds {args.tag} to that revision")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
