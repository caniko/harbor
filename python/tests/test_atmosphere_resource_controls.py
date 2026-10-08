"""Actual kernel limits round the separately recorded systemd byte request down."""

import importlib.util
from pathlib import Path

import pytest


def test_page_granular_native_controls_require_exact_stricter_ram_and_independent_limits(
    monkeypatch,
):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    spec = importlib.util.spec_from_file_location(
        "atmosphere_worker_controls", scripts / "verify_atmosphere_worker.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    approved = 664859648
    controls = {
        "memory.max": "664858624",
        "memory.swap.max": "0",
        "pids.max": "128",
        "cpu.max": "200000 100000",
    }
    module.verify_effective_controls(controls, approved, 4096)
    for key, value in (
        ("memory.max", str(approved)),
        ("memory.max", "664854528"),
        ("memory.swap.max", "4096"),
        ("pids.max", "129"),
        ("cpu.max", "max 100000"),
        ("cpu.max", "300000 100000"),
    ):
        with pytest.raises(ValueError):
            module.verify_effective_controls({**controls, key: value}, approved, 4096)
    with pytest.raises(ValueError):
        module.verify_effective_controls(controls, approved, 0)
