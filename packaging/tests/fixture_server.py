#!/usr/bin/env python3
"""Local fixture release server for the cargo-cleanme installer tests.

Serves a synthetic release over HTTP from a fixture directory so the POSIX and
PowerShell wrappers can be exercised end to end without touching the public
GitHub service. The directory layout mirrors the real release:

    <root>/<version>/<asset>
    <root>/<asset>                      (latest alias)

Supported control files read from the fixture root let a test shape the
response for one asset without a custom server:

    <root>/_control/<asset>.mode        one of: ok, 404, corrupt, wrong-version,
                                        wrong-product, empty
    <root>/_control/<asset>.status      an explicit HTTP status to return

The installers read their base URL from `CARGO_CLEANME_INSTALL_BASE_URL` and
`CARGO_CLEANME_INSTALL_LATEST_URL`; both are set by the test driver to this
server. HTTP (not HTTPS) is used deliberately, so the driver must also set
`CARGO_CLEANME_INSTALL_ALLOW_INSECURE=1`; a real installation never does.
"""
from __future__ import annotations

import argparse
import http.server
import os
import shutil
import socketserver
import sys
import threading
from pathlib import Path

CONTROL_DIR = "_control"
MODE_404 = "404"
MODE_CORRUPT = "corrupt"
MODE_WRONG_VERSION = "wrong-version"
MODE_WRONG_PRODUCT = "wrong-product"
MODE_EMPTY = "empty"
MODE_OK = "ok"


class FixtureHandler(http.server.BaseHTTPRequestHandler):
    root: Path = Path(".")

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        path = self.path.lstrip("/")
        if path.startswith("__ping__"):
            self._send(200, b"pong", "text/plain")
            return

        relative = path.split("?")[0]
        if not relative or relative.endswith("/"):
            self._send(404, b"not found", "text/plain")
            return

        target = (self.root / relative).resolve()
        try:
            target.relative_to(self.root.resolve())
        except ValueError:
            self._send(403, b"forbidden", "text/plain")
            return

        asset = target.name
        control = self.root / CONTROL_DIR / asset
        mode = self._read_control(control, "mode", MODE_OK)
        status = self._read_control(control, "status", None)

        if status is not None:
            self._send(int(status), b"fixture status", "text/plain")
            return
        if mode == MODE_404:
            self._send(404, b"not found", "text/plain")
            return
        if not target.is_file():
            self._send(404, b"not found", "text/plain")
            return

        payload = target.read_bytes()
        if mode == MODE_EMPTY:
            payload = b""
        elif mode == MODE_CORRUPT:
            payload = payload + b"tampered"
        elif mode in (MODE_WRONG_VERSION, MODE_WRONG_PRODUCT):
            payload = _rewrite_version_stub(target, mode)

        self._send(200, payload, "application/octet-stream")

    @staticmethod
    def _read_control(control: Path, key: str, default):
        # Append, never `with_suffix`: an asset name contains dots
        # (`....sha256`) and `with_suffix` would rewrite the asset name itself,
        # so the control file would never be found.
        path = control.parent / f"{control.name}.{key}"
        if not path.is_file():
            return default
        return path.read_text(encoding="utf-8").strip()

    def _send(self, code: int, body: bytes, content_type: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def log_message(self, *_args) -> None:  # noqa: D102 - keep test output clean
        return


def _rewrite_version_stub(target: Path, mode: str) -> bytes:
    """Produce a fake executable that reports the wrong identity.

    A tiny POSIX shell stub is enough: the installers only execute the
    candidate to read `--version`, and the negative cases must be rejected
    before anything is placed.
    """
    if os.name == "nt" or target.suffix == ".exe":
        return b"MZ fake windows candidate"
    if mode == MODE_WRONG_VERSION:
        return b'#!/bin/sh\necho "cargo-cleanme 9.9.9"\n'
    return b'#!/bin/sh\necho "not-cargo-cleanme 0.1.0"\n'


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def serve(root: Path, port: int = 0) -> tuple[Server, int]:
    handler = type("BoundHandler", (FixtureHandler,), {"root": root})
    server = Server(("127.0.0.1", port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, server.server_address[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="fixture release root")
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()
    root = args.root.resolve()
    if not root.is_dir():
        print(f"fixture root is not a directory: {root}", file=sys.stderr)
        return 2
    server, bound = serve(root, args.port)
    print(f"serving {root} on http://127.0.0.1:{bound}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.shutdown()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
