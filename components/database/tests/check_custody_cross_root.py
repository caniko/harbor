"""Real bind-mount alias rejection in an isolated mount namespace or test VM."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("--native", required=True)
parser.add_argument("--root")
parser.add_argument("--engine", choices=("native", "python"), action="append")
args = parser.parse_args()
results = []
for engine in args.engine or ("native", "python"):
    with tempfile.TemporaryDirectory(dir=args.root) as temporary:
        base = Path(temporary)
        a, b, restored_a, restored_b, state = [
            base / n for n in ("a", "b", "restore-a", "restore-b", "state")
        ]
        for root in (a, b, restored_a, restored_b, state):
            root.mkdir(mode=0o700)
        for root in (a, b, restored_a, restored_b):
            (root / "empty").mkdir(mode=0o700)
            (root / "history").write_bytes(b"identical historical corpus")
        resource = {
            "kind": "filesystem",
            "user": "root",
            "runtime_units": [],
            "authority": {
                "resource": "crossroot",
                "state_dir": str(state),
                "directories": [str(a), str(b)],
                "binding": {"backend": "files"},
            },
            "custody_file": str(state / "custody.json"),
            "max_age_seconds": 60,
        }
        contract = base / "contract.json"
        contract.write_text(
            json.dumps(
                {
                    "version": 1,
                    "enforced": True,
                    "host": "fixture",
                    "timeout_seconds": 30,
                    "resources": {"crossroot": resource},
                }
            )
        )
        command = (
            [
                args.native,
                "certify-worker",
                "--contract",
                str(contract),
                "--host",
                "fixture",
                "--phase",
                "certify",
                "--worker",
                "crossroot",
                "--certify-identity",
                "history",
                "--certify-roots",
                str(restored_a),
                str(restored_b),
            ]
            if engine == "native"
            else [
                sys.executable,
                "-B",
                "-c",
                "import json,sys; from harbor_db import cutover; "
                "cutover.certify_filesystem(json.loads(sys.argv[1]),json.loads(sys.argv[2]),'history')",
                json.dumps(resource),
                json.dumps([str(restored_a), str(restored_b)]),
            ]
        )
        # Bind mounts retain nlink=1 and pass path canonicalization. Include a
        # file-only and empty-directory alias so root identity checks alone
        # cannot make this regression pass.
        for name in ("", "history", "empty"):
            source, restore = (
                (b, restored_a) if not name else (b / name, restored_a / name)
            )
            subprocess.run(["mount", "--bind", str(source), str(restore)], check=True)
            try:
                assert os.stat(restore).st_ino == os.stat(source).st_ino
                assert os.stat(restored_a / "history").st_nlink == 1
                result = subprocess.run(
                    command, capture_output=True, text=True, timeout=30
                )
                assert result.returncode != 0 and "aliases" in result.stderr, (
                    engine,
                    name,
                    result.stdout,
                    result.stderr,
                )
                assert not (state / "identity.json").exists()
                assert not (state / "custody.json").exists()
                assert not (a / ".harbor-db-crossroot-identity.json").exists()
                assert not (b / ".harbor-db-crossroot-identity.json").exists()
            finally:
                subprocess.run(["umount", str(restore)], check=True)
        # Disjoint copies with identical content remain certifiable.
        result = subprocess.run(command, capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, (engine, result.stdout, result.stderr)
        assert (state / "custody.json").is_file()
        results.append(
            {
                "engine": engine,
                "crossRootBindAliasRejected": True,
                "fileOnlyAndEmptyDirectoryAliasesRejected": True,
                "noAliasPublication": True,
                "independentCopiesAccepted": True,
            }
        )
print(json.dumps({"status": "pass", "results": results}))
