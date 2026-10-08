"""Fail-closed packaged artifact acceptance and deployed HTTP identity checks."""

import argparse
import hashlib
import json
from pathlib import Path
import sys
import time
from urllib.parse import quote, urlsplit
from urllib.request import urlopen


def files(root):
    root = Path(root)
    if not root.is_dir():
        raise ValueError(f"artifact is not a directory: {root}")
    result = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative.split("/")[0] == ".harbor":
            continue
        if path.is_file():
            result.append(
                {
                    "path": relative,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            )
    if not result:
        raise ValueError(f"artifact has no files: {root}")
    return result


def snapshot(roots):
    return {
        "schema_version": 1,
        "artifacts": {
            name: {"root": str(root), "files": files(root)}
            for name, root in roots.items()
        },
    }


def validate_report(report, required):
    tests = report.get("tests")
    if not isinstance(tests, list) or not tests:
        raise ValueError("acceptance report contains no tests")
    seen = set()
    for test in tests:
        identifier = test.get("id")
        if not isinstance(identifier, str) or not identifier or identifier in seen:
            raise ValueError(f"invalid or duplicate acceptance test: {identifier}")
        seen.add(identifier)
        if test.get("status") != "passed":
            raise ValueError(
                f"required acceptance run did not pass: {identifier}: {test.get('status')}"
            )
    missing = set(required) - seen
    if missing:
        raise ValueError(f"missing required acceptance tests: {sorted(missing)}")


def verify_local(manifest, root, backend=None):
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported artifact identity schema")
    artifacts = manifest["artifacts"]
    if files(root) != artifacts["artifact"]["files"]:
        raise ValueError(f"artifact changed after acceptance: {root}")
    if backend is not None:
        expected = artifacts.get("backend")
        if expected is None or str(backend) != expected["root"]:
            raise ValueError(f"backend does not match acceptance: {backend}")
        if files(backend) != expected["files"]:
            raise ValueError(f"backend changed after acceptance: {backend}")


def verify_http(manifest, base_url, prefix):
    if urlsplit(base_url).scheme not in ("http", "https"):
        raise ValueError("HTTP verification requires an http(s) origin")
    selected = [
        entry
        for entry in manifest["artifacts"]["artifact"]["files"]
        if entry["path"] == "index.html" or entry["path"].startswith(prefix)
    ]
    if not selected or not any(entry["path"].startswith(prefix) for entry in selected):
        raise ValueError(f"manifest has no deployed assets under {prefix}")
    for entry in selected:
        url = base_url.rstrip("/") + "/" + quote(entry["path"], safe="/")
        with urlopen(url, timeout=15) as response:
            if response.status != 200 or response.url != url:
                raise ValueError(f"asset redirected or unavailable: {url}")
            digest = hashlib.sha256(response.read()).hexdigest()
        if digest != entry["sha256"]:
            raise ValueError(
                f"served asset differs from accepted artifact: {entry['path']}"
            )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action", choices=["snapshot", "accept", "verify-local", "verify-http"]
    )
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--roots", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--required", type=Path)
    parser.add_argument("--root", type=Path)
    parser.add_argument("--backend", type=Path)
    parser.add_argument("--url")
    parser.add_argument("--prefix", default="app/")
    parser.add_argument("--attempts", type=int, default=1)
    args = parser.parse_args()
    if args.action == "snapshot":
        args.manifest.write_text(
            json.dumps(snapshot(json.loads(args.roots.read_text())), sort_keys=True)
        )
        return
    manifest = json.loads(args.manifest.read_text())
    if args.action == "accept":
        report = json.loads(args.report.read_text())
        required = json.loads(args.required.read_text())
        validate_report(report, required)
        if (
            snapshot(
                {name: value["root"] for name, value in manifest["artifacts"].items()}
            )
            != manifest
        ):
            raise ValueError("artifacts changed during acceptance")
        manifest.update(required_tests=required, tests=report["tests"])
        args.manifest.write_text(json.dumps(manifest, sort_keys=True))
    else:
        validate_report(manifest, manifest.get("required_tests", []))
        if not manifest.get("required_tests"):
            raise ValueError("manifest has no required acceptance contract")
        if args.action == "verify-local":
            verify_local(manifest, args.root, args.backend)
        else:
            for attempt in range(args.attempts):
                try:
                    verify_http(manifest, args.url, args.prefix)
                    return
                except (OSError, ValueError):
                    if attempt + 1 == args.attempts:
                        raise
                    time.sleep(1)
            raise ValueError("verification attempts must be positive")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError) as error:
        sys.exit(f"harbor artifact acceptance: {error}")
