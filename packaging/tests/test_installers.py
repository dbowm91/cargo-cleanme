#!/usr/bin/env python3
"""Deterministic fixture tests for the cargo-cleanme public installers.

Runs `packaging/install.sh` and, when PowerShell is available,
`packaging/install.ps1` against a local fixture release server. Nothing here
touches the public GitHub service, and no test requires a real release to
exist.

Coverage is organized by the failure the installer must *not* turn into a
silent success or a Cargo fallback:

  happy path            verified binary install, identity, and PATH guidance
  exact version         pinned install selects the pinned asset
  binary absent         404 -> Cargo fallback (the only absence fallback)
  checksum absent       404 sidecar with a present binary -> hard failure
  malformed digest      non-hex / wrong-length sidecar -> hard failure
  digest mismatch       tampered payload -> hard failure
  wrong candidate       wrong product or wrong version -> hard failure
  transport failure     5xx from the release host -> hard failure, no Cargo
  existing destination  refused without --force, replaced with it
  unwritable target     non-writable install directory -> hard failure
  bad version syntax    malformed --version -> rejected before any network use
  no cargo fallback     404 binary with cargo unavailable -> hard failure
  absent build result   Cargo "succeeds" but produces no binary -> hard failure
  temp cleanup          no invocation-owned state is left behind

Run: python3 packaging/tests/test_installers.py [--keep]
"""
from __future__ import annotations

import argparse
import hashlib
import os
import platform
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from fixture_server import CONTROL_DIR, MODE_404, serve  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
INSTALL_SH = ROOT / "packaging" / "install.sh"
INSTALL_PS1 = ROOT / "packaging" / "install.ps1"

PRODUCT = "cargo-cleanme"
VERSION = "0.1.0"

# The exact wrapper diagnostics the branch-level cases must observe. Both
# packaging/install.sh and packaging/install.ps1 emit these substrings, so one
# assertion states the branch on every platform instead of a per-wrapper pair.
# They are asserted rather than inferred from an exit status: see
# `require_diagnostic`.
#
# The three form a chain, which is why each case asserts a different link:
# a 404 binary must *enter* the documented fallback; that fallback must then
# refuse when cargo is genuinely absent; and a stubbed cargo that reports
# success without producing anything must trip the wrapper's own guard.
FALLBACK_ENTERED = "no published binary for this release; using the Cargo source fallback"
CARGO_ABSENT = "cargo is required for the source fallback but was not found"
CARGO_PRODUCED_NOTHING = "Cargo reported success but"


WINDOWS_TARGET = "x86_64-pc-windows-msvc"


def host_posix_target() -> str | None:
    """The contracted triple this host's *POSIX* wrapper will request.

    Mirrors install.sh's own os/arch mapping. Publishing only the Linux asset
    would make every macOS case a false 404-to-fallback instead of testing what
    it claims to test. Returns None off Linux/macOS, where install.sh has no
    contracted target at all.
    """
    machine = platform.machine().lower()
    arch = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    system = platform.system()
    if system == "Linux" and arch == "x64":
        return "x86_64-unknown-linux-gnu"
    if system == "Linux" and arch == "arm64":
        return "aarch64-unknown-linux-gnu"
    if system == "Darwin" and arch == "x64":
        return "x86_64-apple-darwin"
    if system == "Darwin" and arch == "arm64":
        return "aarch64-apple-darwin"
    return None


def asset_for(target: str) -> str:
    """The contracted asset name for a target, straight from the contract."""
    return f"{PRODUCT}-{target}.exe" if target.endswith("-pc-windows-msvc") else f"{PRODUCT}-{target}"

failures: list[str] = []
skips: list[tuple[str, str]] = []
passes = 0


def record(name: str, ok: bool, detail: str = "") -> None:
    global passes
    if ok:
        passes += 1
        print(f"  ok   {name}")
    else:
        failures.append(f"{name}: {detail}")
        print(f"  FAIL {name}: {detail}")


def skip(name: str, reason: str) -> None:
    """A case that could not run here, recorded as a skip and never as a pass.

    A skip and a pass are different claims. Printing `ok` for a case that never
    executed is the same false-green shape as the defect this suite was opened
    to fix: the run looks green and the coverage does not exist.
    """
    skips.append((name, reason))
    print(f"  skip {name}: {reason}")


def diagnostic_present(result: subprocess.CompletedProcess[str], needle: str) -> str | None:
    """None when the expected branch fired, else why it cannot be claimed.

    "exited non-zero and placed nothing" is satisfied by a missing shell, a
    dead fixture server, a typo in an argument, or a real Cargo install that
    failed for unrelated reasons. Only the wrapper's own message proves the
    branch under test is the branch that ran. Both wrappers emit these exact
    substrings, so one assertion holds on every platform.
    """
    combined = result.stdout + result.stderr
    if needle.lower() not in combined.lower():
        return (
            f"exit {result.returncode} without the expected diagnostic "
            f"{needle!r}; output: {combined.strip()[:240]!r}"
        )
    if result.returncode == 0:
        return f"exit 0 despite the expected diagnostic {needle!r}"
    return None


def require_diagnostic(name: str, result: subprocess.CompletedProcess[str], needle: str) -> None:
    """Record the branch assertion for `name`."""
    problem = diagnostic_present(result, needle)
    if problem is None:
        record(name, True)
    else:
        record(name, False, problem)


# --------------------------------------------------------------------- fixture
STUB_CACHE: dict[str, bytes] = {}


def make_candidate_stub(version: str = VERSION) -> bytes:
    """A real compiled executable that answers `--version` like cargo-cleanme.

    A shell script is not a usable stand-in for the release binary on Windows:
    the PowerShell wrapper must actually execute the candidate to check its
    identity, and a script named `.exe` cannot. The CI job already installs a
    Rust toolchain, so the stub is compiled once per version and cached.

    Falling back to a shell script would only be honest where a script *is*
    executable, so the fallback is allowed solely on POSIX hosts; on Windows a
    missing `rustc` is a hard failure rather than a silently weaker test.
    """
    if version in STUB_CACHE:
        return STUB_CACHE[version]
    payload = _compile_candidate_stub(version)
    STUB_CACHE[version] = payload
    return payload


def _compile_candidate_stub(version: str) -> bytes:
    import tempfile as _tempfile

    rustc = shutil.which("rustc")
    if rustc is not None:
        with _tempfile.TemporaryDirectory(prefix="cargo-cleanme-stub-") as work:
            work_path = Path(work)
            source = work_path / "stub.rs"
            source.write_text(
                "fn main() {\n"
                f"    println!(\"{PRODUCT} {version}\");\n"
                "}\n",
                encoding="utf-8",
            )
            output = work_path / ("stub.exe" if os.name == "nt" else "stub")
            result = subprocess.run(
                [rustc, "-O", "-o", str(output), str(source)],
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode == 0 and output.is_file():
                return output.read_bytes()
            if os.name == "nt":
                raise SystemExit(
                    "could not compile the fixture candidate stub on Windows: "
                    f"{result.stderr.strip() or result.stdout.strip()}"
                )
    if os.name == "nt":
        raise SystemExit(
            "the Windows installer lane needs rustc to build a real fixture "
            "candidate; install a Rust toolchain or skip this block"
        )
    return f'#!/bin/sh\necho "{PRODUCT} {version}"\n'.encode()


def build_release_root(base: Path, targets: list[str]) -> Path:
    """Publish `<version>/<asset>` and the latest alias for each target.

    Only the assets the blocks actually running on this host will request are
    published. Publishing the wrong one turns every case into a 404-to-fallback
    that proves nothing about the subject under test.
    """
    release = base / "release"
    version_dir = release / f"v{VERSION}"
    version_dir.mkdir(parents=True)

    for target in targets:
        asset = asset_for(target)
        sidecar = f"{asset}.sha256"
        payload = make_candidate_stub()
        (version_dir / asset).write_bytes(payload)
        digest = hashlib.sha256(payload).hexdigest()
        (version_dir / sidecar).write_text(f"{digest}  {asset}\n")

        # Latest alias: the same version-free asset name at the release root.
        (release / asset).write_bytes(payload)
        (release / sidecar).write_text(f"{digest}  {asset}\n")

    (release / CONTROL_DIR).mkdir(exist_ok=True)
    return release


def set_control(release: Path, asset: str, **values: str) -> None:
    control = release / CONTROL_DIR
    control.mkdir(exist_ok=True)
    for key, value in values.items():
        (control / f"{asset}.{key}").write_text(f"{value}\n")


def clear_control(release: Path, asset: str) -> None:
    control = release / CONTROL_DIR
    for key in ("mode", "status"):
        path = control / f"{asset}.{key}"
        if path.exists():
            path.unlink()


# -------------------------------------------------------------------- runners
def host_env(base_url: str) -> dict[str, str]:
    env = dict(os.environ)
    env["CARGO_CLEANME_INSTALL_BASE_URL"] = base_url
    env["CARGO_CLEANME_INSTALL_LATEST_URL"] = base_url
    env["CARGO_CLEANME_INSTALL_ALLOW_INSECURE"] = "1"
    env["NO_COLOR"] = "1"
    return env


def run_sh(args: list[str], base_url: str, env_extra: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    env = host_env(base_url)
    if env_extra:
        env.update(env_extra)
    return subprocess.run(
        [posix_shell() or "sh", str(INSTALL_SH), *args],
        capture_output=True,
        text=True,
        env=env,
        timeout=180,
        check=False,
    )


def run_ps1(args: list[str], base_url: str, env_extra: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    env = host_env(base_url)
    if env_extra:
        env.update(env_extra)
    return subprocess.run(
        ["pwsh", "-NoProfile", "-NonInteractive", "-File", str(INSTALL_PS1), *args],
        capture_output=True,
        text=True,
        env=env,
        timeout=300,
        check=False,
    )


def have_pwsh() -> bool:
    return shutil.which("pwsh") is not None


def posix_shell() -> str | None:
    """The shell that can run packaging/install.sh, or None if absent.

    Windows CI runners provide Git Bash rather than a POSIX /bin, so the POSIX
    block runs under `bash` there and the fixture tool directory is resolved
    from whatever the runner actually has.
    """
    # install.sh maps `uname -s` to a Linux/macOS family and refuses anything
    # else, so running it under Git Bash on Windows only proves that refusal.
    if platform.system() not in ("Linux", "Darwin"):
        return None
    for candidate in ("sh", "bash"):
        if shutil.which(candidate) is not None:
            return candidate
    return None


def installed_binary(install_dir: Path) -> Path:
    return install_dir / (f"{PRODUCT}.exe" if os.name == "nt" else PRODUCT)


def fake_cargo(tmp: Path) -> Path:
    """A cargo stub that reports success without producing a binary.

    The stub has to be invocable by the *wrapper under test*, not just exist.
    On POSIX that means an executable `cargo` script. On Windows the PowerShell
    wrapper resolves `cargo` through PATHEXT, so an extensionless `#!/bin/sh`
    file named `cargo` is invisible to it: `Get-Command cargo` skips it and
    finds the real cargo further down PATH.

    That is not hypothetical. The Windows lane ran this case as green for the
    entire pre-release period while the real cargo was doing the work. It only
    turned red once `cargo-cleanme 0.1.0` was actually published and the real
    `cargo install` started succeeding. The case had never been exercised on
    Windows at all.
    """
    bindir = tmp / "fakebin"
    bindir.mkdir(parents=True, exist_ok=True)
    if os.name == "nt":
        # `cargo.cmd` is what PATHEXT resolution finds, and `@exit /b 0` is the
        # batch form of a successful exit. `.cmd` is used rather than `.bat`
        # because cmd.exe never auto-executes a bare `.bat` from a path lookup.
        stub = bindir / "cargo.cmd"
        stub.write_text("@exit /b 0\r\n")
    else:
        stub = bindir / "cargo"
        stub.write_text('#!/bin/sh\nexit 0\n')
        stub.chmod(stub.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
    return bindir


# The tools the POSIX wrapper shells out to. A curated PATH is what makes
# "cargo is absent" a real assertion instead of a claim: the wrapper must fail
# on the missing cargo, not on a missing shell or curl.
WRAPPER_TOOLS = (
    "sh", "dash", "bash", "curl", "uname", "id", "grep", "cut", "head", "cat",
    "chmod", "cp", "mv", "mkdir", "rm", "mktemp", "sha256sum", "shasum", "sed",
    "awk", "printf", "env", "test", "true", "false", "ls", "dirname", "basename",
    "pwsh",
)

# Canonical locations searched before PATH, so a sandbox shim on PATH (for
# example a recoverable-delete wrapper around `rm`) is not linked in and then
# broken by the relative path it resolves internally.
CANONICAL_DIRS = ("/usr/bin", "/bin", "/usr/local/bin")


def _is_real_binary(path: Path) -> bool:
    """True for a native executable, false for a shell shim script."""
    try:
        head = path.read_bytes()[:400]
    except OSError:
        return False
    return not head.startswith(b"#!")


def resolve_tool(name: str) -> Path | None:
    for directory in CANONICAL_DIRS:
        candidate = Path(directory) / name
        if candidate.is_file() and os.access(candidate, os.X_OK) and _is_real_binary(candidate):
            return candidate
    return None


def tools_without_cargo(bindir: Path) -> Path:
    """A PATH directory holding the wrapper's tools but deliberately no cargo.

    This is what makes "cargo is absent" a real assertion instead of a claim:
    the wrapper must fail because cargo is missing, not because its shell or
    curl went missing with it.
    """
    bindir.mkdir(parents=True, exist_ok=True)
    for tool in WRAPPER_TOOLS:
        link = bindir / tool
        if link.exists() or link.is_symlink():
            continue
        found = resolve_tool(tool) or (
            Path(shutil.which(tool)) if shutil.which(tool) and _is_real_binary(Path(shutil.which(tool))) else None
        )
        if found is None:
            continue
        try:
            link.symlink_to(found)
        except OSError:
            shutil.copy2(found, link)
    return bindir


# ---------------------------------------------------------------------- cases

def case_happy_path(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    clear_control(release, f"{asset}.sha256")
    dest = work / f"happy-{runner_name}"
    # No cargo on PATH at all: a correct binary install never needs it.
    result = runner(["--dir", str(dest)], base_url, {"PATH": str(tools_dir / "fallback")})
    name = f"[{runner_name}] verified binary install"
    if result.returncode != 0:
        record(name, False, f"exit {result.returncode}: {result.stderr.strip()[:200]}")
        return
    binary = installed_binary(dest)
    if not binary.is_file():
        record(name, False, f"{binary} was not placed")
        return
    observed = subprocess.run([str(binary), "--version"], capture_output=True, text=True, check=False)
    if observed.stdout.strip() != f"{PRODUCT} {VERSION}":
        record(name, False, f"installed candidate reports {observed.stdout.strip()!r}")
        return
    if "not on your PATH" not in result.stdout and "PATH" not in result.stdout:
        record(name, False, "no PATH guidance was emitted")
        return
    record(name, True)


def case_exact_version(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    dest = work / f"exact-{runner_name}"
    if runner_name == "posix":
        args = ["--version", VERSION, "--dir", str(dest)]
    else:
        args = ["-Version", VERSION, "-Directory", str(dest)]
    result = runner(args, base_url)
    name = f"[{runner_name}] exact version install"
    if result.returncode != 0:
        record(name, False, f"exit {result.returncode}: {result.stderr.strip()[:200]}")
        return
    record(name, installed_binary(dest).is_file(), "binary was not placed")


def case_binary_404_falls_back(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    """The one documented absence condition: the binary is genuinely absent."""
    clear_control(release, asset)
    set_control(release, asset, mode=MODE_404)
    dest = work / f"fallback-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    # Cargo is genuinely unavailable, so a correct installer must refuse rather
    # than pretend the fallback succeeded.
    result = runner(args, base_url, {"PATH": str(tools_dir / "fallback")})
    name = f"[{runner_name}] absent binary enters the documented Cargo fallback"
    if result.returncode == 0 and installed_binary(dest).is_file():
        record(name, False, "installed something even though the binary was absent")
    else:
        # The branch under test is "binary absent -> say so, then fall back".
        # "cargo" appearing anywhere in the output is not that branch.
        require_diagnostic(name, result, FALLBACK_ENTERED)


def case_checksum_absent_is_fatal(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    set_control(release, f"{asset}.sha256", mode=MODE_404)
    dest = work / f"nosum-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    result = runner(args, base_url)
    name = f"[{runner_name}] missing checksum is a hard failure, not a fallback"
    if result.returncode == 0:
        record(name, False, "installer succeeded without release evidence")
        return
    if installed_binary(dest).exists():
        record(name, False, "a binary was placed despite the missing checksum")
        return
    if FALLBACK_ENTERED.lower() in (result.stdout + result.stderr).lower():
        record(name, False, "fell back to Cargo after a verification failure")
        return
    record(name, True)


def case_malformed_digest(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    clear_control(release, f"{asset}.sha256")
    # The sidecar lives in the shared fixture, so the corruption must be
    # undone or every later case inherits a broken release.
    sidecar = release / f"{asset}.sha256"
    original = sidecar.read_text()
    try:
        sidecar.write_text("not-a-digest\n")
        dest = work / f"baddigest-{runner_name}"
        args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
        result = runner(args, base_url)
        name = f"[{runner_name}] malformed digest is a hard failure"
        if result.returncode == 0 or installed_binary(dest).exists():
            record(name, False, f"exit {result.returncode} with binary={installed_binary(dest).exists()}")
            return
        record(name, True)
    finally:
        sidecar.write_text(original)


def case_digest_mismatch(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    set_control(release, asset, mode="corrupt")
    dest = work / f"mismatch-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    result = runner(args, base_url)
    name = f"[{runner_name}] tampered payload fails the checksum"
    if result.returncode == 0 or installed_binary(dest).exists():
        record(name, False, f"exit {result.returncode} with binary={installed_binary(dest).exists()}")
        return
    record(name, True)


def case_wrong_candidate(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    for mode, label in (("wrong-product", "wrong product"), ("wrong-version", "wrong version")):
        clear_control(release, asset)
        set_control(release, asset, mode=mode)
        dest = work / f"wrong-{label.replace(' ', '')}-{runner_name}"
        args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
        result = runner(args, base_url)
        name = f"[{runner_name}] candidate with the {label} is rejected before placement"
        if result.returncode == 0 or installed_binary(dest).exists():
            record(name, False, f"exit {result.returncode} with binary={installed_binary(dest).exists()}")
        else:
            record(name, True)


def case_transport_failure(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    set_control(release, asset, status="503")
    dest = work / f"transport-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    result = runner(args, base_url)
    name = f"[{runner_name}] transport failure is not a Cargo fallback signal"
    if result.returncode == 0 or installed_binary(dest).exists():
        record(name, False, f"exit {result.returncode} with binary={installed_binary(dest).exists()}")
    elif FALLBACK_ENTERED.lower() in (result.stdout + result.stderr).lower():
        record(name, False, "fell back to Cargo after a transport failure")
    else:
        record(name, True)


def case_existing_destination(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    dest = work / f"existing-{runner_name}"
    dest.mkdir(parents=True, exist_ok=True)
    binary = installed_binary(dest)
    # A recognizable non-candidate file, so "was it replaced?" is decidable
    # by content rather than by the destination merely existing.
    foreign = b"foreign-binary-not-cargo-cleanme"
    binary.write_bytes(foreign)
    if os.name != "nt":
        binary.chmod(0o755)

    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    result = runner(args, base_url)
    name = f"[{runner_name}] existing destination is refused without --force"
    if result.returncode == 0:
        record(name, False, "overwrote an existing file without an explicit request")
        return
    if binary.read_bytes() == foreign:
        record(name, True)
    else:
        record(name, False, "the existing destination was modified despite refusal")

    force = (
        ["--dir", str(dest), "--force"]
        if runner_name == "posix"
        else ["-Directory", str(dest), "-Force"]
    )
    forced = runner(force, base_url)
    name = f"[{runner_name}] --force replaces an existing destination"
    if forced.returncode == 0 and binary.read_bytes() != foreign:
        record(name, True)
    else:
        record(name, False, f"exit {forced.returncode}: {forced.stderr.strip()[:160]}")


def case_unwritable_destination(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    clear_control(release, asset)
    if not hasattr(os, "geteuid") or os.geteuid() == 0:
        # Recorded as a skip, never as a pass: root can write to a 0500
        # directory, so the case did not test anything. Printing `ok` here would
        # make a run that never exercised the guard indistinguishable from one
        # that proved it.
        skip(
            f"[{runner_name}] unwritable destination is rejected",
            "running as root, where a 0500 directory is still writable",
        )
        return
    dest = work / f"readonly-{runner_name}"
    dest.mkdir(parents=True, exist_ok=True)
    dest.chmod(0o500)
    try:
        args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
        result = runner(args, base_url)
        name = f"[{runner_name}] unwritable destination is a hard failure"
        if result.returncode == 0 and installed_binary(dest).exists():
            record(name, False, "installed into a non-writable directory")
        else:
            record(name, True)
    finally:
        dest.chmod(0o700)


def case_bad_version_syntax(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    dest = work / f"badver-{runner_name}"
    args = (
        ["--version", "1.2", "--dir", str(dest)]
        if runner_name == "posix"
        else ["-Version", "1.2", "-Directory", str(dest)]
    )
    result = runner(args, base_url)
    name = f"[{runner_name}] malformed --version is rejected before any download"
    if result.returncode == 0 or installed_binary(dest).exists():
        record(name, False, f"exit {result.returncode}")
    else:
        record(name, True)


def case_cargo_missing(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    """Cargo is required for the fallback but is not on PATH."""
    clear_control(release, asset)
    set_control(release, asset, mode=MODE_404)
    dest = work / f"nocargo-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    result = runner(args, base_url, {"PATH": str(tools_dir / "fallback")})
    name = f"[{runner_name}] absent binary with no cargo is a hard failure"
    require_diagnostic(name, result, CARGO_ABSENT)


def probe_resolved_cargo(runner_name: str, env: dict[str, str]) -> str:
    """The `cargo` this platform's own lookup rules would run.

    POSIX uses the shell's `command -v`; Windows resolves through PATHEXT, so
    `Get-Command` is the only probe that answers the question the wrapper will
    actually ask. Asking the wrong question is what made the Windows case
    meaningless: the stub existed, was on PATH, and was not what ran.
    """
    if runner_name == "posix":
        probe = subprocess.run(
            [posix_shell() or "sh", "-c", "command -v cargo"],
            capture_output=True, text=True, env=env, timeout=60, check=False,
        )
        return probe.stdout.strip()
    probe = subprocess.run(
        ["pwsh", "-NoProfile", "-NonInteractive",
         "-Command", "(Get-Command cargo -ErrorAction SilentlyContinue).Source"],
        capture_output=True, text=True, env=env, timeout=120, check=False,
    )
    return probe.stdout.strip().splitlines()[-1].strip() if probe.stdout.strip() else ""


def cargo_premise(runner_name: str, stub_dir: Path, env: dict[str, str]) -> str | None:
    """None when the stub is provably the cargo that will run, else the reason.

    Returns the verdict instead of recording it so the negative direction can
    be exercised too (see `self_test`): a premise assertion that has only ever
    been observed to pass has not been shown to fail.
    """
    resolved = probe_resolved_cargo(runner_name, env)
    if not resolved:
        return "no cargo was resolvable at all"
    if Path(resolved).parent.resolve() != stub_dir.resolve():
        return f"resolved to {resolved!r}, not the stub in {stub_dir}"
    return None


def case_fake_cargo_is_actually_resolved(
    runner_name: str,
    runner,
    release: Path,
    base_url: str,
    work: Path,
    tools_dir: Path,
    asset: str,
) -> None:
    """The premise of the cargo-fallback cases must itself be asserted.

    `case_cargo_produces_nothing` only means something if the `cargo` the
    wrapper finds is the stub. When the stub was not invocable on the host --
    which is what an extensionless `#!/bin/sh` file is on Windows, where
    PowerShell resolves `cargo` through PATHEXT -- the wrapper silently used the
    real cargo instead. The case then passed or failed for a reason that had
    nothing to do with the wrapper, and stayed green on Windows for the entire
    pre-release period.

    So the stub's resolvability is checked directly, in the same environment
    the wrapper will see, before any case depends on it.
    """
    stub_dir = fake_cargo(work / f"stubpremise-{runner_name}")
    env = host_env("")
    env["PATH"] = os.pathsep.join([str(stub_dir), os.environ.get("PATH", "")])
    name = f"[{runner_name}] the cargo stub is the cargo the wrapper will resolve"
    problem = cargo_premise(runner_name, stub_dir, env)
    if problem is None:
        record(name, True)
    else:
        record(name, False, problem)


def case_cargo_produces_nothing(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    """Cargo reports success but produces no binary: that must not be success.

    The stub exits 0, so the wrapper's own `Cargo reported success but ... was
    not produced` guard is the only thing that can stop this install. Asserting
    merely "non-zero exit, no binary" would accept a fixture server that died,
    a shell that was missing, or a real `cargo install` that failed for an
    unrelated reason -- the exact false-green that kept C012's Windows case
    green while the real Cargo did the work. The premise case above proves the
    stub is the cargo that ran; this assertion proves the branch that fired was
    ours.
    """
    clear_control(release, asset)
    set_control(release, asset, mode=MODE_404)
    dest = work / f"emptycargo-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    stub_dir = fake_cargo(work / f"stub-{runner_name}")
    result = runner(
        args, base_url, {"PATH": os.pathsep.join([str(stub_dir), os.environ.get("PATH", "")])}
    )
    name = f"[{runner_name}] a Cargo run that produces no binary is a hard failure"
    if installed_binary(dest).exists():
        record(name, False, "a binary was placed even though Cargo produced none")
        return
    require_diagnostic(name, result, CARGO_PRODUCED_NOTHING)


def case_temp_cleanup(runner_name: str, runner, release: Path, base_url: str, work: Path, tools_dir: Path, asset: str) -> None:
    """A failing run must leave no invocation-owned state behind."""
    clear_control(release, asset)
    set_control(release, asset, status="503")
    dest = work / f"cleanup-{runner_name}"
    args = (["--dir", str(dest)] if runner_name == "posix" else ["-Directory", str(dest)])
    before = set(Path(tempfile.gettempdir()).glob("cargo-cleanme*"))
    runner(args, base_url)
    after = set(Path(tempfile.gettempdir()).glob("cargo-cleanme*"))
    leaked = sorted(str(p) for p in (after - before))
    name = f"[{runner_name}] temporary state is cleaned up on failure"
    record(name, not leaked, f"leaked: {leaked}")


def case_contract_projection() -> None:
    """The wrapper target mapping is a mechanical projection of the contract.

    Delegates to the repository check so the test and CI cannot disagree about
    what "a projection of the contract" means.
    """
    result = subprocess.run(
        [sys.executable, str(ROOT / "scripts" / "check-installer-contract.py")],
        capture_output=True,
        text=True,
        check=False,
    )
    name = "[contract] wrapper target mapping matches the Eggpack contract"
    if result.returncode == 0:
        record(name, True)
    else:
        record(name, False, result.stderr.strip()[:300] or result.stdout.strip()[:300])


# The roster is a list rather than a hand-written call sequence so the driver
# can state how many cases a run owes the operator. A case quietly dropped from
# this tuple would otherwise reduce coverage while every remaining case stayed
# green, which is the failure mode this suite exists to prevent.
CASES = (
    case_happy_path,
    case_exact_version,
    case_binary_404_falls_back,
    case_checksum_absent_is_fatal,
    case_malformed_digest,
    case_digest_mismatch,
    case_wrong_candidate,
    case_transport_failure,
    case_existing_destination,
    case_unwritable_destination,
    case_bad_version_syntax,
    case_cargo_missing,
    case_fake_cargo_is_actually_resolved,
    case_cargo_produces_nothing,
    case_temp_cleanup,
)


def self_test() -> int:
    """Prove the two new guards actually reject a broken premise.

    A premise assertion that has only ever been observed to pass has not been
    shown to work. C012's case was green for an entire release cycle precisely
    because nothing ever checked the guards in the direction that matters: with
    the stub *not* reachable, and with the real tool winning the lookup.

    Each check below asserts the guard's own verdict on a deliberately broken
    setup. If a guard stops rejecting these, this mode fails, and CI runs it on
    every lane.
    """
    problems: list[str] = []

    def expect(name: str, rejected: bool, detail: str) -> None:
        if rejected:
            print(f"  ok   {name}")
        else:
            problems.append(f"{name}: the guard accepted a broken premise ({detail})")
            print(f"  FAIL {name}: the guard accepted a broken premise ({detail})")

    def completed(returncode: int, stdout: str, stderr: str) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess(
            args=["install.sh"], returncode=returncode, stdout=stdout, stderr=stderr
        )

    print("--- self test: branch assertions reject an unrelated failure ---")
    # What the pre-C013 cases accepted: non-zero exit, nothing placed, and no
    # evidence at all about which branch produced it.
    unrelated = completed(1, "", "sh: cargo: not found")
    expect(
        "a non-zero exit with no wrapper diagnostic is not accepted as the branch",
        diagnostic_present(unrelated, CARGO_PRODUCED_NOTHING) is not None,
        "the assertion would have passed on any unrelated failure",
    )
    exit_zero = completed(0, f"{PRODUCT}: Cargo reported success but /tmp/bin/{PRODUCT}", "")
    expect(
        "exit 0 is not accepted even when the wording matches",
        diagnostic_present(exit_zero, CARGO_PRODUCED_NOTHING) is not None,
        "a successful install claiming a guard fired is not a pass",
    )
    expect(
        "the real diagnostic is accepted",
        diagnostic_present(
            completed(1, "", f"{PRODUCT}: Cargo reported success but /tmp/bin/{PRODUCT} was not produced"),
            CARGO_PRODUCED_NOTHING,
        )
        is None,
        "a correct branch was rejected",
    )

    print("\n--- self test: the resolution premise rejects a lost stub ---")
    runner_name = "posix" if posix_shell() is not None else "powershell"
    if runner_name != "posix":
        print("  skip the premise negatives: they need a POSIX host to stage a stub")
        print("  and the Windows lane proves the same two directions for PowerShell.\n")
        return 1 if problems else 0

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-installer-selftest-"))
    try:
        stub_dir = fake_cargo(work / "stub")
        inherited = os.environ.get("PATH", "")

        first = host_env("")
        first["PATH"] = os.pathsep.join([str(stub_dir), inherited])
        expect(
            "the stub is accepted when it is the first PATH entry",
            cargo_premise(runner_name, stub_dir, first) is None,
            "a correct premise was rejected",
        )

        reversed_env = host_env("")
        reversed_env["PATH"] = os.pathsep.join([inherited, str(stub_dir)])
        expect(
            "the stub is rejected when PATH precedence is reversed",
            cargo_premise(runner_name, stub_dir, reversed_env) is not None,
            "a real cargo ahead of the stub was accepted as the stub",
        )

        without = host_env("")
        without["PATH"] = str(tools_without_cargo(work / "nocargo"))
        expect(
            "the premise is rejected when no cargo is resolvable at all",
            cargo_premise(runner_name, stub_dir, without) is not None,
            "an unresolvable cargo was accepted as the stub",
        )
    finally:
        shutil.rmtree(work, ignore_errors=True)

    print()
    if problems:
        print(f"FAILED: {len(problems)} premise guard(s) did not reject a broken setup", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    print("self test passed: every premise guard rejects a broken setup")
    return 0


# ------------------------------------------------------------------------ main
def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep", action="store_true", help="keep the temporary fixture tree")
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the premise guards reject a broken setup, then exit",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-installer-tests-"))
    tools = work / "tools"
    tools_without_cargo(tools / "fallback")

    # Decide which blocks can honestly run here before building any fixture.
    # A block that cannot run must not influence the fixture contents, and a
    # block that runs must have its own asset published.
    posix_target = host_posix_target()
    run_posix = posix_shell() is not None and posix_target is not None
    run_powershell = os.name == "nt" and have_pwsh()

    blocks: list[tuple[str, object, str, str]] = []
    if run_posix:
        assert posix_target is not None
        blocks.append(("posix", run_sh, posix_target, asset_for(posix_target)))
    if run_powershell:
        blocks.append(("powershell", run_ps1, WINDOWS_TARGET, asset_for(WINDOWS_TARGET)))

    if not blocks:
        # Exiting 0 here would report a green run that qualified nothing: the
        # worst possible answer from a suite whose entire purpose is proving
        # which installer actually executed. A host that can run neither block
        # has no installer evidence, and that is a failure of the run, not a
        # pass. The hosted Linux/macOS/Windows lanes each run one block.
        print(
            "no installer block can run on this host, so this run qualified "
            "nothing; refusing to report success",
            file=sys.stderr,
        )
        print(
            "  install.sh needs a Linux or macOS host; install.ps1 needs "
            "Windows with pwsh on PATH",
            file=sys.stderr,
        )
        shutil.rmtree(work, ignore_errors=True)
        return 1

    release = build_release_root(work, [target for _, _, target, _ in blocks])
    server, port = serve(release)
    base_url = f"http://127.0.0.1:{port}"
    try:
        print(f"fixture release at {release}")
        print(f"fixture server on {base_url}\n")

        case_contract_projection()

        if not run_posix:
            print("  skip posix block: install.sh has no contracted target on this host;")
            print("  its cases are qualified on the Linux/macOS lanes instead.\n")
        if not run_powershell:
            print("  skip powershell block: install.ps1 only runs on Windows;")
            print("  its cases are qualified by the hosted Windows lane instead.\n")

        executed = 0
        for runner_name, runner, _target, asset in blocks:
            print(f"--- {runner_name} ({_target}) ---")
            for case in CASES:
                case(runner_name, runner, release, base_url, work, tools, asset)
                executed += 1
            print()
    finally:
        server.shutdown()
        server.server_close()
        if args.keep:
            print(f"fixture tree kept at {work}")
        else:
            shutil.rmtree(work, ignore_errors=True)

    # The roster and the driver must not drift apart: a case removed from
    # CASES would otherwise shrink coverage silently while the run stayed green.
    expected_cases = len(blocks) * len(CASES)
    if executed != expected_cases:
        record(
            "[suite] every declared case ran for every block",
            False,
            f"executed {executed} of {expected_cases} "
            f"({len(CASES)} cases x {len(blocks)} block(s))",
        )
    else:
        record(
            "[suite] every declared case ran for every block",
            True,
        )

    print(f"{passes} passed, {len(failures)} failed, {len(skips)} skipped")
    for name, reason in skips:
        print(f"  SKIP {name}: {reason}")
    for failure in failures:
        print(f"  FAIL {failure}")
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
