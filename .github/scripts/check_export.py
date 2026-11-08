"""Fail closed on source-export drift and non-public payload references."""

import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib


SOURCE_REVISION = "3ecdd743b26c0881ac17520883aab717fb813ec6"
BANNED = re.compile(
    r"\b(?:TWO|TOG|PAP|PAPA)-[0-9]+\b"
    r"|/[A-Z]{2,6}/(?:issues|agents|projects|approvals|runs)/"
    r"|/(?:home|paperclip)/[^\s\"'`]+|\." r"paperclip/",
    re.IGNORECASE,
)


def reference_errors(path, data):
    text = data.decode("utf-8")
    return [f"{path}:{number}: non-public reference"
            for number, line in enumerate(text.splitlines(), 1)
            if BANNED.search(line)]


def provenance_errors(root, manifest):
    errors = []
    if manifest.get("source_revision") != SOURCE_REVISION:
        errors.append("unexpected extraction revision")
    seen = set()
    for record in manifest["files"]:
        relative = Path(record["destination"])
        if relative.is_absolute() or ".." in relative.parts or relative in seen:
            errors.append(f"invalid/duplicate provenance destination: {relative}")
            continue
        seen.add(relative)
        path = root / relative
        if not path.is_file() or path.is_symlink():
            errors.append(f"missing/non-regular export: {relative}")
            continue
        expected = record.get("destination_sha256")
        if not isinstance(expected, str) or not re.fullmatch(r"[0-9a-f]{64}", expected):
            errors.append(f"missing/invalid export digest: {relative}")
        elif hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            errors.append(f"export digest drift: {relative}")
        if not re.fullmatch(r"[0-9a-f]{40}", record["source_blob"]):
            errors.append(f"invalid source blob: {relative}")
        source_path = record.get("source_path")
        if (not isinstance(source_path, str) or not source_path
                or Path(source_path).is_absolute() or ".." in Path(source_path).parts):
            errors.append(f"missing/invalid source path: {relative}")
        else:
            history = (
                "https://github.com/TogetherWeOwn/nntunev2/commits/"
                f"{SOURCE_REVISION}/{source_path}"
            )
            if record.get("contribution_history") != history:
                errors.append(f"missing/unpinned contribution history: {relative}")
        last_change = record.get("source_last_change")
        if not isinstance(last_change, str) or not re.fullmatch(r"[0-9a-f]{40}", last_change):
            errors.append(f"missing/invalid source last change: {relative}")
    return errors


def config_errors(config):
    errors = []
    allowed = {"name", "main", "compatibility_date", "workers_dev", "preview_urls",
               "vars", "hyperdrive", "kv_namespaces", "r2_buckets"}
    for name, section in [("default", config), *config.get("env", {}).items()]:
        allowed_section = allowed | {"env"} if section is config else allowed
        for key in section.keys() - allowed_section:
            errors.append(f"{name}: unapproved configuration key: {key}")
        if section.get("workers_dev") is not False:
            errors.append(f"{name}: workers.dev must be disabled")
        if section.get("preview_urls") is not False:
            errors.append(f"{name}: preview URLs must be disabled")
        if any(key in section for key in ["routes", "route", "triggers", "build"]):
            errors.append(f"{name}: configured runtime/build trigger not allowed")
        for key in ["hyperdrive", "kv_namespaces"]:
            for binding in section.get(key, []):
                if binding.get("id") != "UNCONFIGURED":
                    errors.append(f"{name}: usable provider ID not allowed")
    return errors


def main():
    root = Path(__file__).resolve().parents[2]
    paths = subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=root
    ).decode().split("\0")
    errors = []
    if not any(path.endswith(".rs") for path in paths):
        errors.append("no exported Rust source")
    for relative in filter(None, paths):
        path = root / relative
        if path.is_symlink():
            errors.append(f"symlink not allowed: {relative}")
            continue
        data = path.read_bytes()
        if len(data) > 1_000_000:
            errors.append(f"unexpected large export: {relative}")
        try:
            errors.extend(reference_errors(relative, data))
        except UnicodeDecodeError:
            errors.append(f"unreviewed binary payload: {relative}")
    manifest = json.loads((root / "provenance/source-export.json").read_text())
    errors.extend(provenance_errors(root, manifest))
    config = tomllib.loads((root / "workers/mb/wrangler.toml").read_text())
    errors.extend(config_errors(config))
    workspace = tomllib.loads((root / "Cargo.toml").read_text())
    if set(workspace["workspace"]["members"]) != {"workers/mb", "crates/nntune-logging"}:
        errors.append("unexpected workspace members")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"export guard: {len(list(filter(None, paths)))} tracked files; provenance/config valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
