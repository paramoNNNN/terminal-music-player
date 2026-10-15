#!/usr/bin/env python3
"""Download a verified private GitHub Release binary without building Rust."""
import argparse
import os
from pathlib import Path
import platform
import re
import tarfile
import shutil
import subprocess
import tempfile

from release_common import SYSTEMS, verified_binary

REPO = "paramoNNNN/terminal-music-player"


def host_system():
    operating_system = {"Darwin": "darwin", "Linux": "linux"}.get(platform.system())
    architecture = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64", "amd64": "x86_64"}.get(platform.machine().lower())
    if f"{architecture}-{operating_system}" not in SYSTEMS:
        raise ValueError("Supported platforms are Apple Silicon macOS and Linux on x86_64 or ARM64")
    if operating_system == "linux" and Path("/etc/NIXOS").exists():
        raise ValueError("On NixOS, use the Textamp flake with Cachix; see docs/binary-distribution.md")
    return f"{architecture}-{operating_system}"


def atomic_install(binary, destination):
    if destination.is_symlink():
        raise ValueError("Destination is a symlink; choose a regular binary path")
    destination.parent.mkdir(parents=True, exist_ok=True)
    pending = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".textamp-install-", dir=destination.parent, delete=False) as stream:
            pending = Path(stream.name)
            stream.write(binary)
            stream.flush()
            os.fsync(stream.fileno())
        pending.chmod(0o755)
        os.replace(pending, destination)
    finally:
        if pending is not None:
            pending.unlink(missing_ok=True)


def install(tag=None, destination=None):
    system = host_system()
    if shutil.which("gh") is None:
        raise ValueError("Install GitHub CLI and run gh auth login for the private repository")
    if tag is None:
        tag = subprocess.check_output(["gh", "release", "view", "--repo", REPO, "--json", "tagName", "--jq", ".tagName"], text=True).strip()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/-]*", tag):
        raise ValueError("Invalid release tag")
    destination = destination or (Path.home() / "Applications/textamp" if system.endswith("darwin") else Path.home() / ".local/bin/textamp")
    asset = f"textamp-{system}.tar.gz"
    with tempfile.TemporaryDirectory(prefix="textamp-release-") as temporary:
        directory = Path(temporary)
        subprocess.run(["gh", "release", "download", tag, "--repo", REPO, "--pattern", asset,
                        "--pattern", asset + ".sha256", "--dir", str(directory)], check=True)
        binary, metadata = verified_binary(directory / asset, directory / (asset + ".sha256"), system)
        atomic_install(binary, destination)
    print(f"Installed {tag} ({metadata['revision'][:12]}) to {destination}")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", help="Release tag (default: latest published release)")
    parser.add_argument("--destination", type=Path, help="Binary path (default: ~/Applications/textamp on macOS)")
    args = parser.parse_args()
    try:
        install(args.tag, args.destination)
    except (ValueError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Installation failed: {error}\n")


if __name__ == "__main__":
    main()
