"""Exercise artifact validation without Xcode or network access."""

import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

import moltenvk


class ArtifactTests(unittest.TestCase):
    def archive(self, path, extra=None, manifest=None):
        files = {
            "manifest.json": json.dumps(manifest or moltenvk.BASELINE).encode(),
            "libMoltenVK.a": b"!<arch>\n",
            "licenses/MoltenVK/LICENSE": b"test fixture",
        }
        with tarfile.open(path, "w:gz") as bundle:
            for name, content in files.items():
                member = tarfile.TarInfo(name)
                member.size = len(content)
                bundle.addfile(member, io.BytesIO(content))
            if extra:
                bundle.addfile(extra)

    def test_verified_cache_and_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / "input.tar.gz"
            self.archive(archive)
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            archive.rename(root / f"{digest}.tar.gz")
            moltenvk.download("https://unused.invalid/artifact", digest, root / "output")
            moltenvk.validate(root / "output")

    def test_corrupt_cache_is_rejected_before_extraction(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            digest = "0" * 64
            (root / f"{digest}.tar.gz").write_bytes(b"corrupt")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                moltenvk.download("https://unused.invalid/artifact", digest, root / "output")
            self.assertFalse((root / "output").exists())

    def test_paths_and_links_cannot_escape(self):
        for member in [tarfile.TarInfo("../escape"), tarfile.TarInfo("/escape"),
                       tarfile.TarInfo("link")]:
            if member.name == "link":
                member.type = tarfile.SYMTYPE
                member.linkname = "../escape"
            with self.subTest(member=member.name), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                self.archive(root / "input.tar.gz", extra=member)
                with self.assertRaises(ValueError):
                    moltenvk.unpack(root / "input.tar.gz", root / "output")
                self.assertFalse((root / "escape").exists())

    def test_wrong_source_revision_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.archive(root / "input.tar.gz",
                         manifest=dict(moltenvk.BASELINE, revision="wrong"))
            with self.assertRaisesRegex(ValueError, "revision"):
                moltenvk.unpack(root / "input.tar.gz", root / "output")


if __name__ == "__main__":
    unittest.main()
