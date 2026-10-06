#!/usr/bin/env python3
"""Resolve an exact smoke transition and wait boundedly for crates.io (M011C).

The published-release smoke answers one question: does the binary that shipped
for version N actually update a real installation of version N-1 to version N?
Until M011C that ran only when an operator remembered, which is why it found
C016 and C017 while every fixture suite was green.

Making it automatic creates two hazards that this module exists to remove, and
both are the same hazard in different clothes: **resolving a transition from
whatever happens to be available** instead of from a proven exact pair.

1. `"latest"` is not a version. `post-release-smoke.sh` must exercise the
   *released previous-version updater*, so the source binary has to be pinned to
   an exact published release. Asking the updater to fetch "latest" and comparing
   afterwards would let a newer release race the test into passing.

2. GitHub publication is not crates.io publication. The release process publishes
   the GitHub release first and `cargo publish` second, so a `release: published`
   event can arrive before the version the updater depends on exists. Waiting
   unboundedly is not the alternative -- that converts an infrastructure lag into
   a red build that is still red an hour later. **A wait that runs out of time is
   a failure, never a skip**, because "the smoke did not run" and "the smoke
   passed" must never look alike in a closure record.

Version comparison is numeric, not lexical: `0.1.9 < 0.1.10` is a requirement
here precisely because string comparison gets it backwards and would select a
release that does not exist as the "previous" one.

Network behaviour is not configurable in a way that lets a caller skip the wait.
The timeout is a hard deadline, retries have backoff, every request is HTTPS
with a descriptive User-Agent and a finite body cap, and a yanked or prerelease
version never satisfies the wait.

Usage:
    python3 scripts/resolve-smoke-transition.py --to v0.1.7
    python3 scripts/resolve-smoke-transition.py --to v0.1.7 --previous v0.1.6
    python3 scripts/resolve-smoke-transition.py --wait-crates-io --to v0.1.7
    python3 scripts/resolve-smoke-transition.py --self-test
"""
from __future__ import annotations

import argparse
import json
import re
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONTRACT = ROOT / "release/eggpack/distribution.toml"

REPO = "dbowm91/cargo-cleanme"
USER_AGENT = "cargo-cleanme-release-smoke/1 (+https://github.com/dbowm91/cargo-cleanme)"
CRATES_IO_API = "https://crates.io/api/v1/crates/cargo-cleanme"

STABLE_TAG = re.compile(r"^v(\d+)\.(\d+)\.(\d+)$")

# A crates.io response for one crate version is a few kilobytes. Reading
# unbounded is how a verification step becomes an out-of-memory step, so the
# body is capped rather than trusted.
MAX_RESPONSE_BYTES = 1 << 20
REQUEST_TIMEOUT_SECONDS = 20
DEFAULT_DEADLINE_SECONDS = 900
DEFAULT_INITIAL_BACKOFF_SECONDS = 5
MAX_BACKOFF_SECONDS = 60


class TransitionError(Exception):
    """A classified failure. Every one of these is a failed smoke, not a skip."""


def parse_stable_tag(tag: str) -> tuple[int, int, int] | None:
    """`(major, minor, patch)` for a plain stable tag, else `None`.

    A leading `v` is required and prerelease/build suffixes are refused, matching
    the publication contract. `v0.1.7-rc.1` is a pre-release and is not a smoke
    target; accepting it would let a pre-release move the "previous" pointer.
    """
    match = STABLE_TAG.match(tag or "")
    if not match:
        return None
    return tuple(int(part) for part in match.groups())  # type: ignore[return-value]


def format_version(version: tuple[int, int, int]) -> str:
    return f"v{version[0]}.{version[1]}.{version[2]}"


def contracted_asset(triple: str) -> str:
    import tomllib

    with CONTRACT.open("rb") as handle:
        contract = tomllib.load(handle)
    product = contract["product"]["id"]
    for entry in contract["targets"]:
        if entry["triple"] == triple:
            return (
                entry["asset"]["asset"]
                .replace("{product}", product)
                .replace("{target}", triple)
            )
    raise TransitionError(
        f"triple {triple!r} is not a contracted release target; the smoke matrix is tied to the "
        "five targets in distribution.toml and inventing a sixth would prove nothing"
    )


def _fetch_json(url: str, *, opener=urllib.request.urlopen) -> object:
    if not url.startswith("https://"):
        # Not a defensive nicety: the version authority must not be readable or
        # substitutable over a channel this project has not committed to.
        raise TransitionError(f"refusing a non-HTTPS version authority: {url}")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with opener(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
            body = response.read(MAX_RESPONSE_BYTES + 1)
    except urllib.error.URLError as error:
        raise TransitionError(f"the version authority at {url} is unreachable: {error.reason}")
    except TimeoutError as error:
        raise TransitionError(f"the version authority at {url} timed out after {REQUEST_TIMEOUT_SECONDS}s")
    if len(body) > MAX_RESPONSE_BYTES:
        raise TransitionError(
            f"the version authority at {url} returned more than {MAX_RESPONSE_BYTES} bytes; "
            "refusing to read an unbounded body as a version list"
        )
    try:
        return json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise TransitionError(f"the version authority at {url} did not return JSON: {error}")


def enumerate_releases(*, opener=urllib.request.urlopen) -> list[dict]:
    """Public, non-draft, non-prerelease releases, newest first.

    Drafts are excluded because they are not published; prereleases are excluded
    because a pre-release is not what an operator installing `latest` would have
    received, so a transition through one would not describe a real update.
    """
    url = f"https://api.github.com/repos/{REPO}/releases?per_page=100"
    payload = _fetch_json(url, opener=opener)
    if not isinstance(payload, list):
        raise TransitionError("the GitHub releases API did not return a list")
    releases: list[dict] = []
    for entry in payload:
        if not isinstance(entry, dict):
            continue
        if entry.get("draft") or entry.get("prerelease"):
            continue
        tag = entry.get("tag_name")
        version = parse_stable_tag(tag or "")
        if version is None:
            continue
        assets = {
            a.get("name")
            for a in entry.get("assets", [])
            if isinstance(a, dict) and isinstance(a.get("name"), str)
        }
        releases.append({"tag": tag, "version": version, "assets": assets})
    releases.sort(key=lambda r: r["version"], reverse=True)
    return releases


def select_predecessor(
    target: str, triple: str, *, opener=urllib.request.urlopen, releases=None
) -> str:
    """The greatest stable release strictly below `target` that ships the asset."""
    target_version = parse_stable_tag(target)
    if target_version is None:
        raise TransitionError(
            f"{target!r} is not a plain stable vX.Y.Z tag; the smoke target is fixed to the "
            "publication contract, and accepting a pre-release or a floating name would make "
            "the transition unreproducible"
        )
    asset = contracted_asset(triple)

    if releases is None:
        releases = enumerate_releases(opener=opener)

    candidates = [
        release
        for release in releases
        if release["version"] < target_version and asset in release["assets"]
    ]
    if not candidates:
        # A broken predecessor is exactly what a transition test should expose,
        # so it is never skipped. The only tolerated state is a genuine first
        # release, which the caller handles explicitly.
        raise TransitionError(
            f"no stable cargo-cleanme release below {target} ships {asset!r} for {triple}; a "
            "transition cannot be tested without a real previous-version binary, and skipping "
            "a known-bad predecessor would hide the very defect this smoke exists to find"
        )
    # Sort here rather than trusting the caller's order. `enumerate_releases`
    # already sorts, but a caller supplying releases directly -- a test, a
    # cached list, a future caller reading from a different source -- would
    # otherwise get whatever happened to be first, and `0.1.10` sorts below
    # `0.1.9` as a string.
    candidates.sort(key=lambda release: release["version"], reverse=True)
    return candidates[0]["tag"]


def wait_for_crates_io(
    target: str,
    *,
    deadline_seconds: int = DEFAULT_DEADLINE_SECONDS,
    initial_backoff: int = DEFAULT_INITIAL_BACKOFF_SECONDS,
    sleep=time.sleep,
    clock=time.monotonic,
    opener=urllib.request.urlopen,
) -> dict:
    """Poll the public crates.io version authority until `target` is advertised.

    The updater resolves its download from crates.io, so a smoke that ran before
    the target version appeared would be testing a transition that cannot
    actually happen. Exhausting the deadline is a failure: a smoke that never ran
    must not be reportable as one that passed.
    """
    target_version = parse_stable_tag(target)
    if target_version is None:
        raise TransitionError(f"{target!r} is not a plain stable vX.Y.Z tag")

    # The crates.io per-version endpoint takes a **version number**, not a tag:
    # `/api/v1/crates/<name>/<version>`. Passing the tag asked for
    # `.../cargo-cleanme/v0.2.0` and crates.io answered **HTTP 400**, so every
    # automatic smoke polled a URL that can never succeed until its deadline and
    # then failed. The path is built from the validated parse rather than by
    # stripping the first character, so a future tag-shape change cannot
    # reintroduce it silently.
    major, minor, patch = target_version
    version_path = f"{major}.{minor}.{patch}"

    started = clock()
    deadline = started + deadline_seconds
    backoff = initial_backoff
    attempts = 0
    last: str = "the target version has not been observed yet"

    while True:
        attempts += 1
        try:
            payload = _fetch_json(f"{CRATES_IO_API}/{version_path}", opener=opener)
        except TransitionError as error:
            # A transient 404 or outage is the normal case here, because
            # publication is ordered GitHub-first. It is recorded and retried
            # until the deadline, never converted into a pass.
            last = str(error)
        else:
            version = payload.get("version") if isinstance(payload, dict) else None
            if isinstance(payload, dict) and isinstance(version, dict):
                number = version.get("num")
                yanked = bool(version.get("yanked"))
                prerelease = "yanked" in (version.get("channel") or "") or bool(
                    re.search(r"[-+]", str(number or ""))
                )
                if yanked:
                    last = f"crates.io advertises {target} but it is yanked"
                elif prerelease:
                    last = f"crates.io advertises {target} as a non-stable channel"
                elif number != version_path:
                    last = (
                        f"crates.io answered for {target!r} with version {number!r}; the "
                        "authority must echo the exact release tag"
                    )
                else:
                    return {
                        "target": target,
                        "attempts": attempts,
                        "waited_seconds": round(clock() - started, 3),
                        "source": f"{CRATES_IO_API}/{version_path}",
                    }
            else:
                last = "crates.io returned a response without a version object"

        if clock() >= deadline:
            raise TransitionError(
                f"crates.io did not advertise {target} within {deadline_seconds}s "
                f"({attempts} attempt(s)). Last observation: {last}. This is a failed smoke, not "
                "a skipped one: the release is not closed until the version is actually available"
            )
        sleep(min(backoff, max(0, deadline - clock())))
        backoff = min(backoff * 2, MAX_BACKOFF_SECONDS)


def resolve(*, target: str, triple: str, previous: str | None, opener=urllib.request.urlopen) -> dict:
    """The exact `from`/`to` pair the smoke must use, recorded explicitly."""
    if parse_stable_tag(target) is None:
        raise TransitionError(
            f"{target!r} is not a plain stable vX.Y.Z tag; a floating or pre-release target makes "
            "the whole transition unreproducible"
        )
    if previous is not None:
        if parse_stable_tag(previous) is None:
            raise TransitionError(f"{previous!r} is not a plain stable vX.Y.Z tag")
        if parse_stable_tag(previous) >= parse_stable_tag(target):
            raise TransitionError(
                f"the transition source {previous} is not below the target {target}; an update "
                "that does not move the version forward is not a transition"
            )
        return {"from": previous, "to": target, "source": "manual"}
    return {
        "from": select_predecessor(target, triple, opener=opener),
        "to": target,
        "source": "resolved",
    }


# ------------------------------------------------------------------ self-test


def self_test() -> int:
    failures: list[str] = []

    def expect(name: str, problems: list[str], should_fail: bool) -> None:
        if bool(problems) != should_fail:
            failures.append(name)
            print(
                f"  FAIL {name}: expected {'rejected' if should_fail else 'accepted'}, got "
                f"{'rejected' if problems else 'accepted'}"
            )
        else:
            print(f"  ok   {name}")

    def expect_error(name: str, call, fragment: str) -> None:
        try:
            call()
        except TransitionError as error:
            if fragment in str(error):
                print(f"  ok   {name}")
                return
            failures.append(name)
            print(f"  FAIL {name}: expected a failure mentioning {fragment!r}, got {error}")
            return
        failures.append(name)
        print(f"  FAIL {name}: expected a failure mentioning {fragment!r}, but the call succeeded")

    def fake_opener(pages: dict[str, object]):
        """A urllib stand-in serving fixed pages, so the wait is exercised without a network."""
        def open_url(request, timeout=None):
            url = request.full_url if hasattr(request, "full_url") else str(request)
            for pattern, payload in pages.items():
                if re.fullmatch(pattern, url):
                    body = json.dumps(payload).encode("utf-8")
                    return _Response(body)
            raise urllib.error.HTTPError(url, 404, "not found", None, None)

        return open_url

    class _Response:
        def __init__(self, body: bytes) -> None:
            self._body = body

        def read(self, amount: int = -1) -> bytes:
            return self._body if amount < 0 else self._body[:amount]

        def __enter__(self) -> "_Response":
            return self

        def __exit__(self, *_: object) -> None:
            return None

    print("resolve-smoke-transition: self test")

    # 1. Version ordering. `0.1.9 < 0.1.10` is the case string comparison gets
    #    backwards, and getting it backwards selects a release that does not
    #    exist as the previous one.
    expect(
        "0.1.9 sorts below 0.1.10",
        [] if parse_stable_tag("v0.1.9") < parse_stable_tag("v0.1.10") else ["ordering is wrong"],
        False,
    )
    expect(
        "0.1.10 sorts above 0.1.9",
        [] if parse_stable_tag("v0.1.10") > parse_stable_tag("v0.1.9") else ["ordering is wrong"],
        False,
    )
    for bad in ("v0.1.7-rc.1", "0.1.7", "v0.1", "latest", "", "v0.1.7+build", "V0.1.7"):
        if parse_stable_tag(bad) is not None:
            failures.append(f"{bad!r} is refused as a stable tag")
            print(f"  FAIL {bad!r} is refused as a stable tag")
    print("  ok   pre-release, partial, and floating tags are refused")

    # 2. Predecessor selection over a realistic release list.
    triple = "x86_64-unknown-linux-gnu"
    asset = contracted_asset(triple)
    releases = [
        {"tag": "v0.1.10", "version": parse_stable_tag("v0.1.10"), "assets": {asset, "x"}},
        {"tag": "v0.1.9", "version": parse_stable_tag("v0.1.9"), "assets": {asset, "x"}},
        {"tag": "v0.2.0", "version": parse_stable_tag("v0.2.0"), "assets": {asset, "x"}},
    ]
    expect(
        "the greatest stable release below the target is selected",
        [] if select_predecessor("v0.1.10", triple, releases=releases) == "v0.1.9" else ["wrong predecessor"],
        False,
    )
    expect(
        "a target above every release selects the newest below it",
        [] if select_predecessor("v0.3.0", triple, releases=releases) == "v0.2.0" else ["wrong predecessor"],
        False,
    )
    # 0.1.9 < 0.1.10 numerically. Under string ordering "0.1.10" < "0.1.9", so a
    # lexical implementation would move the previous-version pointer backwards
    # and then try to download a binary from the wrong release.
    ten_nine = [
        {"tag": "v0.1.10", "version": parse_stable_tag("v0.1.10"), "assets": {asset}},
        {"tag": "v0.1.9", "version": parse_stable_tag("v0.1.9"), "assets": {asset}},
        {"tag": "v0.1.8", "version": parse_stable_tag("v0.1.8"), "assets": {asset}},
    ]
    expect(
        "0.1.9 is the predecessor of 0.1.10, not 0.1.8",
        [] if select_predecessor("v0.1.10", triple, releases=ten_nine) == "v0.1.9" else ["wrong predecessor"],
        False,
    )
    expect(
        "0.1.8 is the predecessor of 0.1.9",
        [] if select_predecessor("v0.1.9", triple, releases=ten_nine) == "v0.1.8" else ["wrong predecessor"],
        False,
    )
    expect(
        "the target release itself is never selected as its own predecessor",
        [] if select_predecessor("v0.2.0", triple, releases=releases) == "v0.1.10" else ["wrong predecessor"],
        False,
    )
    expect_error(
        "a predecessor missing the contracted asset fails rather than being skipped",
        lambda: select_predecessor("v0.1.8", triple, releases=releases),
        "no stable cargo-cleanme release below",
    )
    expect_error(
        "a target with no predecessor at all fails",
        lambda: select_predecessor("v0.0.1", triple, releases=[]),
        "no stable cargo-cleanme release below",
    )
    expect_error(
        "a pre-release target is refused",
        lambda: select_predecessor("v0.1.7-rc.1", triple, releases=releases),
        "plain stable",
    )
    expect_error(
        "an uncontracted triple is refused",
        lambda: contracted_asset("aarch64-unknown-plan9"),
        "not a contracted release target",
    )

    # Drafts and prereleases must never enter the candidate set.
    enumerated = [
        {"tag_name": "v0.1.8", "draft": False, "prerelease": False, "assets": [{"name": asset}]},
        {"tag_name": "v0.1.7-rc.1", "draft": False, "prerelease": True, "assets": [{"name": asset}]},
        {"tag_name": "v0.1.9", "draft": True, "prerelease": False, "assets": [{"name": asset}]},
    ]
    filtered = enumerate_releases(opener=fake_opener({r".*releases.*": enumerated}))
    tags = [r["tag"] for r in filtered]
    if "v0.1.7-rc.1" in tags or "v0.1.9" in tags:
        failures.append("drafts and prereleases are excluded")
        print("  FAIL drafts and prereleases are excluded")
    else:
        print("  ok   drafts and prereleases are excluded")
    if tags != ["v0.1.8"]:
        failures.append("only the stable release survives filtering")
        print(f"  FAIL only the stable release survives filtering: {tags}")
    else:
        print("  ok   only the stable release survives filtering")

    # Newest-first ordering, with a list where input order and version order
    # disagree -- the GitHub API returns releases by creation date, not by
    # semver, so "sort by what the API gave us" is not an ordering.
    unordered = [
        {"tag_name": "v0.1.8", "draft": False, "prerelease": False, "assets": [{"name": asset}]},
        {"tag_name": "v0.1.10", "draft": False, "prerelease": False, "assets": [{"name": asset}]},
        {"tag_name": "v0.1.9", "draft": False, "prerelease": False, "assets": [{"name": asset}]},
    ]
    ordered = [r["tag"] for r in enumerate_releases(opener=fake_opener({r".*releases.*": unordered}))]
    if ordered == ["v0.1.10", "v0.1.9", "v0.1.8"]:
        print("  ok   releases are ordered by version, not by API order")
    else:
        failures.append("releases are ordered by version, not by API order")
        print(f"  FAIL releases are ordered by version, not by API order: {ordered}")

    # 3. The crates.io wait reaches the target, and a fake clock makes that
    #    testable without spending the real deadline.
    clock_state = {"now": 0.0}
    sleeps: list[float] = []

    def fake_clock() -> float:
        return clock_state["now"]

    def fake_sleep(seconds: float) -> None:
        sleeps.append(seconds)
        clock_state["now"] += seconds

    attempts = {"n": 0}

    def eventually_ready(request, timeout=None):
        attempts["n"] += 1
        url = request.full_url if hasattr(request, "full_url") else str(request)
        if attempts["n"] < 3:
            raise urllib.error.HTTPError(url, 404, "not found", None, None)
        return _Response(
            json.dumps(
                {
                    "version": {
                        "num": "0.1.7",
                        "yanked": False,
                        "channel": "stable",
                    }
                }
            ).encode()
        )

    # Every other case here injects an opener that ignores the URL it is handed,
    # which is precisely why a wrong URL could ship green: all of them would
    # have passed against `/cargo-cleanme/v0.1.7` just as happily. This one
    # records what was actually requested and checks the path. crates.io's
    # per-version endpoint takes a version number, not a tag -- a tag answers
    # HTTP 400 -- so this case is the one that can fail for the reason that
    # matters.
    requested: list[str] = []

    def records_url(request, timeout=None):
        requested.append(request.full_url if hasattr(request, "full_url") else str(request))
        return _Response(
            json.dumps({"version": {"num": "0.1.7", "yanked": False, "channel": "stable"}}).encode()
        )

    wait_for_crates_io(
        "v0.1.7",
        deadline_seconds=5,
        sleep=lambda _s: None,
        clock=lambda: 0.0,
        opener=records_url,
    )
    polled = requested[0] if requested else ""
    if polled == f"{CRATES_IO_API}/0.1.7" and not polled.endswith("/v0.1.7"):
        print("  ok   the crates.io wait requests a version number, not a tag")
    else:
        failures.append("the crates.io wait requests a version number, not a tag")
        print(f"  FAIL the crates.io wait requests a version number, not a tag: {polled!r}")

    result = wait_for_crates_io(
        "v0.1.7",
        deadline_seconds=120,
        sleep=fake_sleep,
        clock=fake_clock,
        opener=eventually_ready,
    )
    if result["attempts"] == 3 and clock_state["now"] > 0:
        print("  ok   the crates.io wait retries until the target is advertised")
    else:
        failures.append("the crates.io wait retries until the target is advertised")
        print("  FAIL the crates.io wait retries until the target is advertised")

    if sleeps == sorted(sleeps) and all(s > 0 for s in sleeps) and max(sleeps) <= MAX_BACKOFF_SECONDS:
        print("  ok   retry backoff grows and stays bounded")
    else:
        failures.append("retry backoff grows and stays bounded")
        print("  FAIL retry backoff grows and stays bounded")

    # 4. Every non-arrival is a failure, not a skip.
    def always_missing(request, timeout=None):
        url = request.full_url if hasattr(request, "full_url") else str(request)
        raise urllib.error.HTTPError(url, 404, "not found", None, None)

    expect_error(
        "an authority that never advertises the target fails at the deadline",
        lambda: wait_for_crates_io(
            "v0.1.7",
            deadline_seconds=30,
            initial_backoff=5,
            sleep=fake_sleep,
            clock=fake_clock,
            opener=always_missing,
        ),
        "did not advertise",
    )

    def yanked(request, timeout=None):
        return _Response(
            json.dumps({"version": {"num": "0.1.7", "yanked": True, "channel": "stable"}}).encode()
        )

    expect_error(
        "a yanked target does not satisfy the wait",
        lambda: wait_for_crates_io(
            "v0.1.7", deadline_seconds=30, sleep=fake_sleep, clock=fake_clock, opener=yanked
        ),
        "yanked",
    )

    def wrong_version(request, timeout=None):
        return _Response(
            json.dumps({"version": {"num": "0.1.6", "yanked": False, "channel": "stable"}}).encode()
        )

    expect_error(
        "a wrong version does not satisfy the wait",
        lambda: wait_for_crates_io(
            "v0.1.7", deadline_seconds=30, sleep=fake_sleep, clock=fake_clock, opener=wrong_version
        ),
        "must echo the exact release tag",
    )

    def prereleased(request, timeout=None):
        return _Response(
            json.dumps({"version": {"num": "0.1.7-rc.1", "yanked": False, "channel": "beta"}}).encode()
        )

    expect_error(
        "a pre-release target does not satisfy the wait",
        lambda: wait_for_crates_io(
            "v0.1.7", deadline_seconds=30, sleep=fake_sleep, clock=fake_clock, opener=prereleased
        ),
        "non-stable channel",
    )

    def garbage(request, timeout=None):
        return _Response(b"<html>maintenance</html>")

    expect_error(
        "a non-JSON authority response is a failure, not an arrival",
        lambda: wait_for_crates_io(
            "v0.1.7", deadline_seconds=30, sleep=fake_sleep, clock=fake_clock, opener=garbage
        ),
        "did not return JSON",
    )

    expect_error(
        "a non-HTTPS version authority is refused outright",
        lambda: _fetch_json("http://crates.io/api/v1/crates/cargo-cleanme"),
        "refusing a non-HTTPS",
    )

    def oversized(request, timeout=None):
        return _Response(b"{" + b"x" * (MAX_RESPONSE_BYTES + 10))

    expect_error(
        "an oversized response is refused",
        lambda: _fetch_json("https://crates.io/api/v1/crates/cargo-cleanme", opener=oversized),
        "refusing to read an unbounded body",
    )

    # 5. Manual overrides are still validated.
    expect(
        "an explicit predecessor is accepted and recorded",
        [] if resolve(target="v0.1.7", triple=triple, previous="v0.1.6").get("from") == "v0.1.6" else ["wrong"],
        False,
    )
    expect_error(
        "a manual predecessor at or above the target is refused",
        lambda: resolve(target="v0.1.7", triple=triple, previous="v0.1.7"),
        "not below the target",
    )
    expect_error(
        "a manual predecessor that is not stable is refused",
        lambda: resolve(target="v0.1.7", triple=triple, previous="latest"),
        "plain stable",
    )
    expect_error(
        "a floating target is refused",
        lambda: resolve(target="latest", triple=triple, previous=None),
        "plain stable",
    )

    if failures:
        print(f"resolve-smoke-transition: self test FAILED ({len(failures)} case(s))", file=sys.stderr)
        return 1
    print(
        "resolve-smoke-transition: self test passed; transitions are exact, waits are bounded, "
        "and every non-arrival is a failure"
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--to", help="exact target release tag, e.g. v0.1.7")
    parser.add_argument(
        "--previous", help="exact source release tag for a manual/rehearsal transition"
    )
    parser.add_argument(
        "--target-triple",
        default="x86_64-unknown-linux-gnu",
        help="the release target whose transition is being resolved",
    )
    parser.add_argument(
        "--wait-crates-io",
        action="store_true",
        help="also wait boundedly for crates.io to advertise the target version",
    )
    parser.add_argument(
        "--deadline-seconds",
        type=int,
        default=DEFAULT_DEADLINE_SECONDS,
        help=f"total wait budget for the crates.io authority (default {DEFAULT_DEADLINE_SECONDS})",
    )
    parser.add_argument(
        "--emit",
        choices=("github", "env"),
        default="github",
        help="how to emit the resolved pair for the workflow (default github)",
    )
    parser.add_argument("--self-test", action="store_true", help="prove each classification")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if not args.to:
        parser.error("--to is required (or use --self-test)")

    try:
        transition = resolve(
            target=args.to, triple=args.target_triple, previous=args.previous
        )
        if args.wait_crates_io:
            transition["crates_io"] = wait_for_crates_io(
                args.to, deadline_seconds=args.deadline_seconds
            )
    except TransitionError as error:
        print(f"resolve-smoke-transition: FAILED: {error}", file=sys.stderr)
        return 1

    if args.emit == "github":
        print(f"from_version={transition['from']}")
        print(f"to_version={transition['to']}")
    else:
        print(f"from_version={transition['from']}")
        print(f"to_version={transition['to']}")
    if "crates_io" in transition:
        print(
            f"crates_io_attempts={transition['crates_io']['attempts']} "
            f"crates_io_waited_seconds={transition['crates_io']['waited_seconds']}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
