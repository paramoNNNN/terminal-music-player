"""Release archive format shared by packaging, installation and fixture tests."""
import hashlib
import json
import re
import struct
import tarfile

SYSTEMS = {"x86_64-linux", "aarch64-linux", "x86_64-darwin", "aarch64-darwin"}
MAX_BINARY = 128 * 1024 * 1024


def validate_binary(data, system):
    if system not in SYSTEMS:
        raise ValueError("Unsupported platform")
    if not data or len(data) > MAX_BINARY:
        raise ValueError("Invalid binary size")
    if system.endswith("-linux"):
        expected = 62 if system.startswith("x86_64") else 183
        if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", data, 18)[0] != expected:
            raise ValueError("Binary is not the expected Linux architecture")
    else:
        expected = 0x1000007 if system.startswith("x86_64") else 0x100000C
        if len(data) < 32 or data[:4] != b"\xcf\xfa\xed\xfe" or struct.unpack_from("<I", data, 4)[0] != expected:
            raise ValueError("Binary is not the expected macOS architecture")


def verified_binary(archive, checksum, system):
    name = f"textamp-{system}.tar.gz"
    fields = checksum.read_text().strip().split()
    if len(fields) != 2 or not re.fullmatch(r"[0-9a-f]{64}", fields[0]) or fields[1] != name:
        raise ValueError("Invalid checksum file")
    with archive.open("rb") as stream:
        digest = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if digest.hexdigest() != fields[0]:
        raise ValueError("Archive checksum does not match")
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        by_name = {member.name: member for member in members}
        if len(by_name) != len(members):
            raise ValueError("Archive has duplicate entries")
        # Read exact regular files; never extract paths, symlinks or hard links.
        data = {}
        for member_name, limit in [("textamp", MAX_BINARY), ("BUILD.json", 4096)]:
            member = by_name.get(member_name)
            if member is None or not member.isfile() or member.size > limit:
                raise ValueError("Archive has an invalid required file")
            data[member_name] = bundle.extractfile(member).read(limit + 1)
        metadata = json.loads(data["BUILD.json"])
        if not isinstance(metadata, dict) or metadata.get("format") != 1 or metadata.get("system") != system:
            raise ValueError("Release metadata does not match this platform")
        revision = metadata.get("revision", "")
        if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise ValueError("Release metadata has no valid revision")
        binary = data["textamp"]
        if hashlib.sha256(binary).hexdigest() != metadata.get("binarySha256"):
            raise ValueError("Binary checksum does not match metadata")
    validate_binary(binary, system)
    return binary, metadata
