#!/usr/bin/env python3
"""Package a native release binary, licenses, build identity and SHA-256 sum."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile

from release_common import SYSTEMS, validate_binary


def package(system, binary_path, output, revision):
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Expected a full Git commit SHA")
    binary = binary_path.read_bytes()
    validate_binary(binary, system)
    root = Path(__file__).resolve().parent.parent
    version = re.search(r'^version = "([^"]+)"', (root / "Cargo.toml").read_text(), re.MULTILINE).group(1)
    metadata = {"format": 1, "system": system, "revision": revision,
                "version": version, "binarySha256": hashlib.sha256(binary).hexdigest()}
    files = [("textamp", binary, 0o755),
             ("BUILD.json", (json.dumps(metadata, indent=2) + "\n").encode(), 0o644),
             ("LICENSE", (root / "LICENSE").read_bytes(), 0o644),
             ("LICENSES/ratatui-image.txt", (root / "vendor/ratatui-image/LICENSE").read_bytes(), 0o644)]
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"textamp-{system}.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        for name, content, mode in files:
            info = tarfile.TarInfo(name)
            info.size, info.mode, info.mtime = len(content), mode, 0
            bundle.addfile(info, io.BytesIO(content))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("system", choices=sorted(SYSTEMS))
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--revision", required=True)
    args = parser.parse_args()
    print(package(args.system, args.binary, args.output, args.revision))


if __name__ == "__main__":
    main()
