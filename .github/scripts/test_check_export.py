"""Synthetic negative controls for the public-export gate; run in Actions."""

import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location(
    "check_export", Path(__file__).with_name("check_export.py")
)
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)


def source_metadata(path):
    return {
        "source_path": path,
        "source_last_change": "b" * 40,
        "contribution_history": (
            f"https://github.com/TogetherWeOwn/nntunev2/commits/{guard.SOURCE_REVISION}/{path}"
        ),
    }


class ExportGuardTests(unittest.TestCase):
    def test_contribution_metadata_is_required_and_pinned(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "rewritten.md"
            source.write_bytes(b"standalone documentation")
            record = {
                **source_metadata("docs/original.md"),
                "source_blob": "a" * 40,
                "destination": "rewritten.md",
                "destination_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            }
            manifest = {"source_revision": guard.SOURCE_REVISION, "files": [record]}
            self.assertEqual(guard.provenance_errors(root, manifest), [])
            for key, value in [
                ("source_path", None), ("source_path", "../outside.md"),
                ("source_path", "/outside.md"), ("source_path", ""),
                ("source_last_change", None), ("source_last_change", "not-a-commit"),
                ("contribution_history", None),
                ("contribution_history", record["contribution_history"].replace(
                    guard.SOURCE_REVISION, "main")),
                ("contribution_history", record["contribution_history"] + "-different"),
            ]:
                with self.subTest(key=key, value=value):
                    invalid = {**record, key: value}
                    self.assertTrue(guard.provenance_errors(
                        root, {**manifest, "files": [invalid]}))

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
                **source_metadata("sample.rs"),
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
            record = {**source_metadata("docs/original.md"),
                      "source_blob": "a" * 40, "destination": "rewritten.md"}
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

    def test_provenance_rejects_paths_duplicates_symlinks_and_bad_blobs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "sample.rs"
            source.write_bytes(b"original")
            record = {**source_metadata("sample.rs"), "source_blob": "a" * 40,
                      "destination": "sample.rs",
                      "destination_sha256": hashlib.sha256(b"original").hexdigest()}
            manifest = {"source_revision": guard.SOURCE_REVISION, "files": [record]}
            self.assertEqual(guard.provenance_errors(root, manifest), [])
            for destination in ["../outside.rs", "/outside.rs"]:
                with self.subTest(destination=destination):
                    self.assertTrue(guard.provenance_errors(root, {
                        **manifest, "files": [{**record, "destination": destination}]}))
            self.assertTrue(guard.provenance_errors(root, {
                **manifest, "files": [record, dict(record)]}))
            for blob in ["", "a" * 39, "A" * 40, "g" * 40]:
                with self.subTest(blob=blob):
                    self.assertTrue(guard.provenance_errors(root, {
                        **manifest, "files": [{**record, "source_blob": blob}]}))
            alias = root / "alias.rs"
            alias.symlink_to(source)
            self.assertTrue(guard.provenance_errors(root, {
                **manifest, "files": [{**record, "destination": "alias.rs"}]}))

    def test_main_enforces_payload_and_workspace_boundaries(self):
        for case in ["valid", "binary", "large", "symlink", "members", "no-rust"]:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / ("sample.md" if case == "no-rust" else "sample.rs")
                source.write_bytes(b"pub fn valid() {}")
                if case == "binary":
                    source.write_bytes(b"\\xff")
                if case == "large":
                    source.write_bytes(b"x" * 1_000_001)
                record = {**source_metadata("sample.rs"), "source_blob": "a" * 40,
                          "destination": source.name,
                          "destination_sha256": hashlib.sha256(source.read_bytes()).hexdigest()}
                if case == "symlink":
                    target = root / "target.txt"
                    target.write_bytes(source.read_bytes())
                    source.unlink()
                    source.symlink_to(target)
                (root / "provenance").mkdir()
                (root / "provenance/source-export.json").write_text(json.dumps({
                    "source_revision": guard.SOURCE_REVISION, "files": [record]}))
                (root / "workers/mb").mkdir(parents=True)
                (root / "workers/mb/wrangler.toml").write_text(
                    "workers_dev = false\\npreview_urls = false\\n")
                members = '["workers/mb", "crates/nntune-logging"]' if case != "members" else '["other"]'
                (root / "Cargo.toml").write_text("[workspace]\\nmembers = " + members + "\\n")
                with mock.patch.object(guard, "__file__", str(root / ".github/scripts/check_export.py")), \
                        mock.patch.object(guard.subprocess, "check_output", return_value=(source.name + "\\0").encode()), \
                        contextlib.redirect_stderr(io.StringIO()), contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(guard.main(), 0 if case == "valid" else 1)

    def test_config_rejects_unapproved_bindings(self):
        safe = {"workers_dev": False, "preview_urls": False,
                "kv_namespaces": [{"binding": "MB_CACHE", "id": "UNCONFIGURED"}]}
        self.assertEqual(guard.config_errors(safe), [])
        for key, binding in [
            ("d1_databases", [{"binding": "DB", "database_id": "1" * 32}]),
            ("services", [{"binding": "API", "service": "example-service"}]),
            ("queues", {"producers": [{"binding": "QUEUE", "queue": "example"}]}),
            ("durable_objects", {"bindings": [{"name": "STATE", "class_name": "State"}]}),
        ]:
            for environment in [False, True]:
                with self.subTest(key=key, environment=environment):
                    section = {**safe, key: binding}
                    config = {**safe, "env": {"staging": section}} if environment else section
                    self.assertTrue(guard.config_errors(config))

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
