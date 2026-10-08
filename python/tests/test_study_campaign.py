"""The exact native study campaign refuses development packages before outputs."""

import os
import subprocess
from pathlib import Path


def test_study_qualifier_requires_exact_immutable_packages_before_state(tmp_path):
    repo = Path(__file__).parents[2]
    local = tmp_path / "development-runtime.json"
    local.write_text("{}")
    output = tmp_path / "must-not-exist"
    result = subprocess.run(
        [
            os.sys.executable,
            str(repo / "scripts/verify_study_worker.py"),
            "--executable",
            os.environ["HARBOR_CAD_TEST_BINARY"],
            "--mcp",
            str(local),
            "--runtime",
            str(local),
            "--authority",
            str(local),
            "--native-reference",
            str(tmp_path),
            "--output",
            str(output),
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=10,
    )
    assert (
        result.returncode
        and "exact immutable production package required" in result.stderr
    )
    assert not output.exists()
