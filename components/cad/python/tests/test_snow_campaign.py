"""Snow campaigns bind an explicit immutable operation-only thermal descriptor."""

import importlib.util
from pathlib import Path

import pytest


def test_snow_worker_descriptor_cannot_mix_operations_or_hide_missing_closure(
    monkeypatch,
):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    spec = importlib.util.spec_from_file_location(
        "snow_campaign", scripts / "verify_snow_cpu.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    source = {
        "bwrap": "/nix/store/bwrap/bin/bwrap",
        "thermal": "/nix/store/thermal/bin/harbor-cad-thermal",
        "thermal_closure": "/nix/store/closure/store-paths",
        "openlb_backend": "cpu",
        "cad": None,
        "filter": None,
        "openlb": None,
        "render": None,
        "video": None,
    }
    assert module.thermal_runtime_descriptor(source, worker_runtime=True) == source
    for override in (
        {"thermal_closure": None},
        {"openlb_backend": "hip"},
        {"cad": "/nix/store/importer/bin/cad"},
        {"filter": "/nix/store/filters/bin/filter"},
        {"render": "/nix/store/renderer/bin/renderer"},
    ):
        with pytest.raises(ValueError):
            module.thermal_runtime_descriptor(
                {**source, **override}, worker_runtime=True
            )
    with pytest.raises(ValueError):
        module.thermal_runtime_descriptor(source, worker_runtime=False)
    standalone = {
        "schema_version": 1,
        "backend": "cpu",
        "bwrap": source["bwrap"],
        "thermal": source["thermal"],
    }
    with pytest.raises(ValueError, match="closure"):
        module.thermal_runtime_descriptor(standalone, worker_runtime=False)
    standalone["thermal_closure"] = source["thermal_closure"]
    assert (
        module.thermal_runtime_descriptor(standalone, worker_runtime=False)
        == standalone
    )
