"""Release integrity checks; run with unittest discovery from the repository root."""

import hashlib
from pathlib import Path
import tarfile
import tempfile
import unittest

from release import TARGETS, checksums, package, validate_tag


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = self.root / "Cargo.toml"
        self.manifest.write_text('[package]\nversion = "0.1.0"\n')
        (self.root / "docs").mkdir()
        (self.root / "docs/binaries.md").write_text("Installation instructions\n")
        (self.root / "LICENSE").write_text("MIT\n")
        (self.root / ".env.example").write_text("ADMIN_TOKEN=\n")
        self.binary = self.root / "cargo-rocket"
        self.binary.write_bytes(b"test-binary-bytes")
        self.binary.chmod(0o755)
        self.out = self.root / "dist"

    def test_tag_must_be_versioned_and_match_manifest(self):
        validate_tag("v0.1.0", self.manifest)
        for tag in ["main", "v0.2.0", "v01.1.0", "v0.1.0;echo bad", "v0.1.0\n", "../v0.1.0"]:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                validate_tag(tag, self.manifest)
        self.manifest.write_text('[package]\nversion = "0.2.0-rc.1"\n')
        validate_tag("v0.2.0-rc.1", self.manifest)

    def test_archive_contains_only_distribution_files_and_preserves_execution(self):
        (self.root / ".env").write_text("ADMIN_TOKEN=not-for-distribution")
        archive = package("v0.1.0", TARGETS[0], self.binary, self.out, self.root)
        prefix = f"cargo-rocket-v0.1.0-{TARGETS[0]}"
        with tarfile.open(archive) as tar:
            self.assertEqual(set(tar.getnames()), {
                f"{prefix}/cargo-rocket", f"{prefix}/LICENSE",
                f"{prefix}/README.md", f"{prefix}/.env.example",
            })
            binary = tar.getmember(f"{prefix}/cargo-rocket")
            self.assertEqual(binary.mode, 0o755)
            self.assertEqual((binary.uid, binary.gid, binary.uname, binary.gname), (0, 0, "", ""))
            self.assertEqual(tar.extractfile(binary).read(), self.binary.read_bytes())

    def test_checksums_require_exactly_the_expected_targets(self):
        self.out.mkdir()
        for target in TARGETS:
            (self.out / f"cargo-rocket-v0.1.0-{target}.tar.gz").write_bytes(target.encode())
        checksums("v0.1.0", self.out, self.manifest)
        for line in (self.out / "SHA256SUMS").read_text().splitlines():
            digest, name = line.split("  ")
            self.assertEqual(digest, hashlib.sha256((self.out / name).read_bytes()).hexdigest())
        extra = self.out / "unexpected.tar.gz"
        extra.touch()
        with self.assertRaises(ValueError):
            checksums("v0.1.0", self.out, self.manifest)
        extra.unlink()
        next(self.out.glob("*.tar.gz")).unlink()
        with self.assertRaises(ValueError):
            checksums("v0.1.0", self.out, self.manifest)


if __name__ == "__main__":
    unittest.main()
