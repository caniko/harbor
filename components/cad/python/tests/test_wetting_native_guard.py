"""Exercise the actual C++ output guard without native solver or live hardware."""

import subprocess
from pathlib import Path


def test_native_wetting_guard_accepts_worker_log_and_rejects_stale_or_aliased_output(
    tmp_path,
):
    source = Path(__file__).resolve().parents[2] / "scripts/verify_wetting_outputs.cpp"
    executable = tmp_path / "native-guard"
    subprocess.run(
        [
            "c++",
            "-std=c++17",
            "-Wall",
            "-Wextra",
            "-Werror",
            str(source),
            "-o",
            str(executable),
        ],
        check=True,
        timeout=30,
    )
    result = subprocess.run(
        [str(executable), str(tmp_path / "output")],
        capture_output=True,
        text=True,
        check=True,
        timeout=5,
    )
    assert "seven rejections passed" in result.stdout
