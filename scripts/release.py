#!/usr/bin/env python3
"""Validate, smoke-test, and package Cargo Rocket release binaries (Python 3.11+)."""

import argparse
import hashlib
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import tarfile
import tempfile
import time
import tomllib
import urllib.error
import urllib.request

TARGETS = (
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)
TAG_PATTERN = r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"


def validate_tag(tag, manifest):
    if re.fullmatch(TAG_PATTERN, tag) is None:
        raise ValueError("Expected a version tag such as v0.1.0 or v0.2.0-rc.1")
    version = tomllib.loads(Path(manifest).read_text())["package"]["version"]
    if tag != f"v{version}":
        raise ValueError(f"Tag {tag} does not match Cargo.toml version {version}")


def smoke(binary):
    binary = Path(binary).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="cargo-rocket-smoke-") as scratch:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        env = {
            k: v for k, v in os.environ.items()
            if k not in ("BIND_ADDR", "PUBLIC_BASE_URL")
        }
        env.update(
            ADMIN_TOKEN="release-smoke-test-only-000000000000000000",
            HOST_URL=base,
            DATABASE_PATH=str(Path(scratch) / "test.db"),
        )
        # This token is only for an isolated test process on loopback.
        with tempfile.TemporaryFile() as output:
            process = subprocess.Popen([str(binary)], env=env, stdout=output, stderr=output)
            try:
                deadline = time.monotonic() + 20
                while time.monotonic() < deadline:
                    if process.poll() is not None:
                        output.seek(0)
                        raise RuntimeError(f"Binary exited at startup: {output.read().decode()}")
                    try:
                        with urllib.request.urlopen(base + "/health", timeout=1) as response:
                            assert response.status == 200
                        break
                    except (urllib.error.URLError, TimeoutError):
                        time.sleep(0.1)
                else:
                    raise RuntimeError("Binary did not become healthy within 20 seconds")
                with urllib.request.urlopen(base + "/", timeout=5) as response:
                    if b"Cargo Rocket" not in response.read():
                        raise RuntimeError("Embedded dashboard is missing")
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
    print(f"Smoke test passed: {binary.name}")


def package(tag, target, binary, out, root):
    validate_tag(tag, root / "Cargo.toml")
    if target not in TARGETS:
        raise ValueError(f"Unsupported release target: {target}")
    binary = Path(binary).resolve(strict=True)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("Release binary must be an executable regular file")
    out.mkdir(parents=True, exist_ok=True)
    name = f"cargo-rocket-{tag}-{target}"
    archive = out / f"{name}.tar.gz"

    def metadata(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mode = 0o755 if info.name.endswith("/cargo-rocket") else 0o644
        return info

    with tarfile.open(archive, "w:gz") as tar:
        for source, dest in (
            (binary, "cargo-rocket"),
            (root / "docs/binaries.md", "README.md"),
            (root / "LICENSE", "LICENSE"),
            (root / ".env.example", ".env.example"),
        ):
            tar.add(source, arcname=f"{name}/{dest}", filter=metadata)
    print(archive)
    return archive


def checksums(tag, out, manifest):
    validate_tag(tag, manifest)
    expected = {f"cargo-rocket-{tag}-{target}.tar.gz" for target in TARGETS}
    found = {p.name for p in out.glob("*.tar.gz")}
    if found != expected:
        raise ValueError(f"Unexpected release assets; missing={expected - found}, extra={found - expected}")
    lines = []
    for name in sorted(expected):
        with (out / name).open("rb") as archive:
            digest = hashlib.file_digest(archive, "sha256").hexdigest()
        lines.append(f"{digest}  {name}\n")
    (out / "SHA256SUMS").write_text("".join(lines))
    print(out / "SHA256SUMS")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate")
    validate.add_argument("tag")
    validate.add_argument("--manifest", type=Path, default=Path("Cargo.toml"))
    check = commands.add_parser("smoke")
    check.add_argument("binary", type=Path)
    pack = commands.add_parser("package")
    pack.add_argument("tag")
    pack.add_argument("target", choices=TARGETS)
    pack.add_argument("binary", type=Path)
    pack.add_argument("--out", type=Path, default=Path("dist"))
    pack.add_argument("--root", type=Path, default=Path("."))
    sums = commands.add_parser("checksums")
    sums.add_argument("tag")
    sums.add_argument("--out", type=Path, default=Path("dist"))
    sums.add_argument("--manifest", type=Path, default=Path("Cargo.toml"))
    args = parser.parse_args()
    try:
        if args.command == "validate":
            validate_tag(args.tag, args.manifest)
        elif args.command == "smoke":
            smoke(args.binary)
        elif args.command == "package":
            package(args.tag, args.target, args.binary, args.out, args.root)
        else:
            checksums(args.tag, args.out, args.manifest)
    except (ValueError, OSError, RuntimeError) as error:
        parser.exit(1, f"Release error: {error}\n")


if __name__ == "__main__":
    main()
