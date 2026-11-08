"""Synthetic negative controls for the public-export gate; run in Actions."""

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "check_export", Path(__file__).with_name("check_export.py")
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


class ExportGuardTests(unittest.TestCase):
    def test_public_source_is_accepted(self):
        self.assertEqual(guard.reference_errors("sample.rs", b"pub fn valid() {}"), [])

    def test_internal_reference_is_rejected(self):
        for text in ["TWO" + "-123", "/home" + "/private/file", ".paper" + "clip/private"]:
            with self.subTest(text=text):
                self.assertTrue(guard.reference_errors("sample.rs", text.encode()))

    def test_changed_export_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "sample.rs"
            source.write_bytes(b"original")
            manifest = {"source_revision": guard.SOURCE_REVISION, "files": [{
                "source_blob": "a" * 40,
                "destination": "sample.rs",
                "destination_sha256": hashlib.sha256(b"original").hexdigest(),
            }]}
            self.assertEqual(guard.provenance_errors(root, manifest), [])
            source.write_bytes(b"mutant")
            self.assertTrue(guard.provenance_errors(root, manifest))
            source.unlink()
            self.assertTrue(guard.provenance_errors(root, manifest))

    def test_missing_or_invalid_export_digest_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "rewritten.md"
            source.write_bytes(b"standalone documentation")
            record = {"source_blob": "a" * 40, "destination": "rewritten.md"}
            manifest = {"source_revision": guard.SOURCE_REVISION, "files": [record]}
            for digest in [None, "", "not-a-digest", "a" * 63, "A" * 64, 123]:
                with self.subTest(digest=digest):
                    if digest is None:
                        record.pop("destination_sha256", None)
                    else:
                        record["destination_sha256"] = digest
                    self.assertTrue(guard.provenance_errors(root, manifest))
            record["destination_sha256"] = hashlib.sha256(source.read_bytes()).hexdigest()
            self.assertEqual(guard.provenance_errors(root, manifest), [])
            source.write_bytes(b"unrecorded documentation edit")
            self.assertTrue(guard.provenance_errors(root, manifest))

    def test_config_rejects_public_surface_and_usable_ids(self):
        safe = {"workers_dev": False, "preview_urls": False,
                "kv_namespaces": [{"id": "UNCONFIGURED"}]}
        self.assertEqual(guard.config_errors(safe), [])
        for key, value in [("workers_dev", True), ("preview_urls", True),
                           ("routes", ["example.invalid/*"]),
                           ("kv_namespaces", [{"id": "1" * 32}])]:
            with self.subTest(key=key):
                self.assertTrue(guard.config_errors({**safe, key: value}))
        self.assertTrue(guard.config_errors({**safe, "env": {"production": {}}}))


if __name__ == "__main__":
    unittest.main()
