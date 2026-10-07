#!/usr/bin/env python3
"""Prove the hosted macOS user outcome for the POSIX installer (C027 WP-C).

M010B's requirement was "report the resulting path and PATH guidance", and the
fixture suite proved it by asserting the word `PATH` appeared in the output.
That is why the defect survived: a printed instruction is indistinguishable from
a persistent integration, and on a stock macOS account a first install left a
verified binary in `$HOME/.local/bin` that a newly opened terminal could not
resolve.

This script is the closure evidence for the corrected contract. It:

1. builds a local fixture release and serves it over HTTP, so nothing here
   touches the public GitHub service;
2. runs `packaging/install.sh` as a *normal user* into a fixture-owned HOME,
   with `SHELL=/bin/zsh`;
3. asserts the installer wrote its managed block into the selected startup file;
4. starts a **fresh** `zsh -l -i` with that HOME and asserts
   `command -v cargo-cleanme` resolves to the exact installed fixture binary.

Step 4 is the part that cannot be faked by text: it fails if the profile was
not selected, if the managed entry was absent, if the entry was syntactically
inert, or if a different `cargo-cleanme` won PATH precedence.

It runs on macOS CI. Elsewhere it skips with an explicit reason rather than
reporting a pass -- a host that cannot prove the macOS outcome must not claim
to have proved it.

Usage:
    python3 packaging/tests/qualify_macos_fresh_shell.py
"""
from __future__ import annotations

import argparse
import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_installers import (  # noqa: E402
    INSTALL_SH,
    PRODUCT,
    build_release_root,
    clear_control,
    host_env,
    host_posix_target,
    installed_binary,
    profile_problem,
    read_profile,
    serve,
    tools_without_cargo,
)


def fail(message: str) -> int:
    print(f"FAIL: {message}", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--require-macos",
        action="store_true",
        help="fail instead of skipping when the host is not macOS (CI sets this)",
    )
    parser.add_argument(
        "--platform",
        default=None,
        help=argparse.SUPPRESS,  # local harness only: pretend to be another OS
    )
    args = parser.parse_args()

    system = args.platform or platform.system()
    if system != "Darwin":
        note = (
            "the hosted macOS fresh-shell result can only be proven on macOS; "
            f"this host is {system}"
        )
        if args.require_macos:
            return fail(note)
        print(f"skip: {note}")
        return 0

    zsh = shutil.which("zsh")
    if zsh is None:
        note = "zsh is not installed on this host"
        if args.require_macos:
            return fail(note)
        print(f"skip: {note}")
        return 0

    # `-l -i -c` is the shape Terminal produces: a login+interactive shell running a
    # command string. Without `-c`, zsh reads the argument as a script file name
    # ("can't open input file"), which measures nothing here.
    return qualify(zsh, "/bin/zsh", ["-l", "-i", "-c"], ".zshrc")


def qualify(shell_path: str, login_shell: str, shell_flags: list[str], profile_name: str) -> int:
    """Run the fixture install and the fresh-shell assertion.

    `login_shell` is the value the wrapper will see in `$SHELL`, and
    `profile_name` the startup file it will therefore select; both are parameters
    so a Linux host drives the identical sequence with bash. A local harness
    reimplementing this would be a second version of the evidence, which is the
    thing C027 exists to stop; calling it is the whole point.
    """
    if os.name == "nt" or (not hasattr(os, "geteuid")) or os.geteuid() == 0:
        # Root installs take the /usr/local/bin system destination, which is
        # deliberately never persisted into a user profile, so this case cannot
        # mean anything as root. There is no skip switch here: a caller that
        # reached `qualify` asked for the assertion.
        print("skip: running as root, where the system-scope policy applies")
        return 0

    target = host_posix_target()
    if target is None:
        return fail(f"install.sh has no contracted target on {platform.machine()}")

    work = Path(tempfile.mkdtemp(prefix="cargo-cleanme-macos-fresh-shell-"))
    try:
        release = build_release_root(work, [target])
        asset = f"{PRODUCT}-{target}"
        clear_control(release, asset)
        clear_control(release, f"{asset}.sha256")
        server, port = serve(release)
        base_url = f"http://127.0.0.1:{port}"
        try:
            print(f"fixture release at {release}")
            print(f"fixture server on {base_url}\n")

            home = work / "home"
            (home / ".local" / "bin").mkdir(parents=True)
            # A curated PATH with no user-local bin directory on it, so the
            # installer's "is the destination already on PATH" check is a real
            # condition rather than an accident of the runner's environment.
            tools_path = str(tools_without_cargo(work / "tools"))

            env = host_env(base_url)
            env.update({
                "HOME": str(home),
                "SHELL": login_shell,
                "PATH": tools_path,
                "ZDOTDIR": "",
                "NO_COLOR": "1",
            })

            print(f"--- canonical non-root {login_shell} install ---")
            install = subprocess.run(
                ["/bin/sh", str(INSTALL_SH)],
                capture_output=True, text=True, env=env, timeout=180, check=False,
            )
            if install.returncode != 0:
                print(install.stdout)
                print(install.stderr, file=sys.stderr)
                return fail(f"the installer exited {install.returncode}")

            installed = installed_binary(home / ".local" / "bin")
            if not installed.is_file():
                return fail(f"{installed} was not installed")
            print(f"  ok   installed {installed}")

            profile = home / profile_name
            problem = profile_problem(profile, home)
            if problem is not None:
                return fail(problem)
            print("  ok   the selected startup file holds one managed block")
            print(f"  --- {profile} ---")
            for line in (read_profile(profile) or "").splitlines():
                print(f"  | {line}")

            # The point of the case: a *fresh* login+interactive shell, which is
            # what Terminal starts, not this process.
            #
            # Two environment details decide whether this measures the profile at
            # all. ZDOTDIR is *removed* rather than set empty: zsh consults
            # ZDOTDIR in place of HOME whenever it is present at all, so an empty
            # value sends it looking for `/.zshrc`. And PATH stays the host's --
            # the curated tool PATH is for the installer only, because macOS
            # `/etc/zprofile` calls `path_helper`, which a tool-only PATH cannot
            # resolve, and startup then stops before `.zshrc` is read.
            flags = " ".join(shell_flags)
            print(f"\n--- fresh {Path(shell_path).name} {flags} resolves the installed binary ---")
            probe_env = {k: v for k, v in os.environ.items() if k != "ZDOTDIR"}
            probe_env["HOME"] = str(home)
            probe = subprocess.run(
                [shell_path, *shell_flags, f"command -v {PRODUCT}"],
                capture_output=True, text=True, env=probe_env, timeout=60, check=False,
            )
            resolved = probe.stdout.strip().splitlines()[-1].strip() if probe.stdout.strip() else ""
            print(f"  {Path(shell_path).name} {flags} 'command -v {PRODUCT}' -> {resolved!r}")
            if probe.returncode != 0:
                print(
                    f"  {Path(shell_path).name} exited {probe.returncode}: "
                    f"{probe.stderr.strip()[-400:]!r}",
                    file=sys.stderr,
                )
                return fail("the fresh shell exited non-zero")
            if resolved != str(installed):
                return fail(
                    f"a fresh {Path(shell_path).name} resolved {resolved!r}, expected the "
                    f"installed fixture binary {str(installed)!r}"
                )
            print(f"  ok   the fresh shell resolves the exact installed fixture binary")

            # The negative direction, in the same environment: without the
            # profile the same shell must NOT resolve the command. Without this,
            # a PATH that happened to contain the directory would make the
            # positive result meaningless.
            print("\n--- without the profile, the same shell must not resolve it ---")
            # Renaming the profile away is the cleanest way to remove the entry:
            # ZDOTDIR would work for zsh but not for a bash harness, and an
            # override that only applies to one shell would let the two runs
            # differ in more than the one thing under test.
            hidden = profile.with_name(profile.name + ".hidden")
            profile.rename(hidden)
            try:
                bare = subprocess.run(
                    [shell_path, *shell_flags, f"command -v {PRODUCT}"],
                    capture_output=True, text=True, env=probe_env, timeout=60, check=False,
                )
            finally:
                hidden.rename(profile)
            bare_resolved = bare.stdout.strip().splitlines()[-1].strip() if bare.stdout.strip() else ""
            print(f"  {Path(shell_path).name} {flags} 'command -v {PRODUCT}' -> {bare_resolved!r}")
            if bare_resolved == str(installed):
                return fail(
                    "the command resolved even with the profile hidden, so the "
                    "positive result was not caused by the persisted entry"
                )
            print("  ok   the resolution depends on the persisted entry, not on ambient PATH")

            print(f"\n{login_shell} fresh-shell qualification passed")
            return 0
        finally:
            server.shutdown()
            server.server_close()
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    raise SystemExit(main())