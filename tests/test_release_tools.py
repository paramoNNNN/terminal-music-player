"""Release/install tests use synthetic headers and a mocked gh; no real install."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))
from release_common import SYSTEMS, verified_binary


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, SCRIPTS / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


packager = load("package_release", "package-release.py")
installer = load("install_binary", "install-binary.py")


def binary(system):
    data = bytearray(64)
    if system.endswith("linux"):
        data[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", data, 18, 62 if system.startswith("x86_64") else 183)
    else:
        data[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", data, 4, 0x1000007 if system.startswith("x86_64") else 0x100000C)
    return bytes(data)


class ReleaseTools(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.system = "aarch64-darwin"
        self.source = self.root / "source"
        self.source.write_bytes(binary(self.system))
        self.archive = packager.package(self.system, self.source, self.root / "assets", "a" * 40)
        self.checksum = self.archive.with_name(self.archive.name + ".sha256")
        self.destination = self.root / "install/textamp"
        self.destination.parent.mkdir()
        self.destination.write_bytes(b"existing installation")

    def fake_download(self, args, **kwargs):
        self.assertEqual(args[:4], ["gh", "release", "download", "v1.0.0"])
        folder = Path(args[args.index("--dir") + 1])
        shutil.copy(self.archive, folder / self.archive.name)
        shutil.copy(self.checksum, folder / self.checksum.name)
        return subprocess.CompletedProcess(args, 0)

    def install(self, run=None):
        with patch.object(installer, "host_system", return_value=self.system), \
             patch.object(installer.shutil, "which", return_value="gh"), \
             patch.object(installer.subprocess, "check_output", return_value="v1.0.0\n") as lookup, \
             patch.object(installer.subprocess, "run", side_effect=run or self.fake_download):
            result = installer.install(destination=self.destination)
            lookup.assert_called_once()
            return result

    def test_roundtrip_all_platforms_and_licenses(self):
        for system in SYSTEMS:
            with self.subTest(system=system):
                self.source.write_bytes(binary(system))
                archive = packager.package(system, self.source, self.root / "assets", "b" * 40)
                verified, metadata = verified_binary(archive, archive.with_name(archive.name + ".sha256"), system)
                self.assertEqual(verified, binary(system))
                self.assertEqual(metadata["revision"], "b" * 40)
                with tarfile.open(archive) as bundle:
                    self.assertIn("LICENSE", bundle.getnames())
                    self.assertIn("LICENSES/ratatui-image.txt", bundle.getnames())

    def test_latest_download_is_pinned_and_replaces_without_backups(self):
        self.assertEqual(self.install(), self.destination)
        self.assertEqual(self.destination.read_bytes(), binary(self.system))
        self.assertEqual(self.destination.stat().st_mode & 0o777, 0o755)
        self.assertEqual(list(self.destination.parent.iterdir()), [self.destination])

    def test_bad_checksum_preserves_existing_install(self):
        self.checksum.write_text("0" * 64 + "  " + self.archive.name + "\n")
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.install()
        self.assertEqual(self.destination.read_bytes(), b"existing installation")

    def test_failed_download_preserves_existing_install(self):
        def fail(args, **kwargs):
            raise subprocess.CalledProcessError(1, args)
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(fail)
        self.assertEqual(self.destination.read_bytes(), b"existing installation")

    def test_wrong_architecture_is_rejected_before_packaging(self):
        with self.assertRaisesRegex(ValueError, "architecture"):
            packager.package("x86_64-linux", self.source, self.root / "bad", "c" * 40)
        self.assertFalse((self.root / "bad").exists())

    def test_archive_symlink_is_never_extracted(self):
        with tarfile.open(self.archive, "w:gz") as bundle:
            link = tarfile.TarInfo("textamp")
            link.type, link.linkname = tarfile.SYMTYPE, "../../outside"
            bundle.addfile(link)
            data = json.dumps({"format": 1, "system": self.system}).encode()
            info = tarfile.TarInfo("BUILD.json")
            info.size = len(data)
            bundle.addfile(info, io.BytesIO(data))
        digest = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.checksum.write_text(digest + "  " + self.archive.name + "\n")
        with self.assertRaisesRegex(ValueError, "required file"):
            self.install()
        self.assertEqual(self.destination.read_bytes(), b"existing installation")
        self.assertFalse((self.root / "outside").exists())

    def test_installer_selects_native_architecture_and_directs_nixos_to_cache(self):
        for operating_system, machine, expected in [
            ("Darwin", "arm64", "aarch64-darwin"),
            ("Linux", "aarch64", "aarch64-linux"),
            ("Linux", "x86_64", "x86_64-linux"),
        ]:
            with patch.object(installer.platform, "system", return_value=operating_system), \
                 patch.object(installer.platform, "machine", return_value=machine), \
                 patch.object(installer.Path, "exists", return_value=False):
                self.assertEqual(installer.host_system(), expected)
        with patch.object(installer.platform, "system", return_value="Darwin"), \
             patch.object(installer.platform, "machine", return_value="x86_64"):
            with self.assertRaisesRegex(ValueError, "Apple Silicon"):
                installer.host_system()
        with self.assertRaisesRegex(ValueError, "Unsupported platform"):
            packager.package("x86_64-darwin", self.source, self.root / "intel", "c" * 40)
        with patch.object(installer.platform, "system", return_value="Linux"), \
             patch.object(installer.platform, "machine", return_value="x86_64"), \
             patch.object(installer.Path, "exists", return_value=True):
            with self.assertRaisesRegex(ValueError, "Cachix"):
                installer.host_system()

    def test_symlink_destination_is_not_replaced(self):
        self.destination.unlink()
        self.destination.symlink_to(self.source)
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.install()
        self.assertTrue(self.destination.is_symlink())
        self.assertEqual(self.source.read_bytes(), binary(self.system))


if __name__ == "__main__":
    unittest.main()
