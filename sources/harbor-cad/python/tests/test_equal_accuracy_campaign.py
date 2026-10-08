"""Equal-accuracy comparisons reject science/output changes and package stand-ins."""

import importlib.util
import os
import subprocess
from pathlib import Path

import pytest


def test_equal_accuracy_requires_complete_identical_original_times_nodes_and_values(
    monkeypatch,
):
    repo = Path(__file__).parents[2]
    monkeypatch.syspath_prepend(str(repo / "scripts"))
    source = importlib.util.spec_from_file_location(
        "equal_accuracy", repo / "scripts/verify_equal_accuracy_cpu.py"
    )
    module = importlib.util.module_from_spec(source)
    source.loader.exec_module(module)
    original = [
        {
            "requested_s": 120.0,
            "observed_s": 120.0,
            "temperature_k": {"1": 273.15, "2": 283.15},
        }
    ]
    assert module.compare_originals(original, original) == 0.0
    for changed in (
        [],
        [{**original[0], "observed_s": 119.0}],
        [{**original[0], "temperature_k": {"1": 273.15}}],
        [{**original[0], "temperature_k": {"1": 274.15, "2": 283.15}}],
    ):
        with pytest.raises(ValueError):
            module.compare_originals(original, changed)


def test_equal_accuracy_qualifier_refuses_development_package_before_outputs(tmp_path):
    repo = Path(__file__).parents[2]
    local = tmp_path / "runtime.json"
    local.write_text("{}")
    output = tmp_path / "not-created"
    completed = subprocess.run(
        [
            os.sys.executable,
            str(repo / "scripts/verify_equal_accuracy_cpu.py"),
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
        completed.returncode
        and "exact immutable production package required" in completed.stderr
    )
    assert not output.exists()
