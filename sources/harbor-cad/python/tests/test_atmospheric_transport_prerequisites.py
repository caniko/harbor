"""Development diagnostics cannot satisfy registered-source production prerequisites."""

import importlib.util
import json
from pathlib import Path

import pytest


def verifier(monkeypatch):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    spec = importlib.util.spec_from_file_location(
        "atmospheric_transport_prerequisites",
        scripts / "atmospheric_transport_prerequisites.py",
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize(
    ("source_claim", "renderer_claim"),
    [
        (
            "development source overlay",
            "exact immutable renderer and operation sandbox",
        ),
        (
            "exact immutable standalone CPU reference",
            "unqualified extracted-wheel diagnostic",
        ),
    ],
)
def test_development_source_or_extracted_renderer_cannot_authorize_worker(
    monkeypatch, tmp_path, source_claim, renderer_claim
):
    module = verifier(monkeypatch)
    source = tmp_path / "source"
    source.mkdir()
    renderer = tmp_path / "renderer"
    renderer.mkdir()
    (source / "verification.json").write_text(
        json.dumps({"runtime_qualification": source_claim})
    )
    (renderer / "verification.json").write_text(
        json.dumps({"package_qualification": renderer_claim})
    )
    with pytest.raises(ValueError, match="both complete exact-package"):
        module.prerequisites(
            tmp_path / "binary",
            tmp_path / "source-runtime",
            tmp_path / "runtime",
            source,
            renderer,
        )
    assert not (tmp_path / "state").exists()


def test_exact_claims_without_immutable_packages_are_insufficient(
    monkeypatch, tmp_path
):
    module = verifier(monkeypatch)
    for label, content in (
        (
            "source",
            {"runtime_qualification": "exact immutable standalone CPU reference"},
        ),
        (
            "renderer",
            {"package_qualification": "exact immutable renderer and operation sandbox"},
        ),
    ):
        directory = tmp_path / label
        directory.mkdir()
        (directory / "verification.json").write_text(json.dumps(content))
    runtime = tmp_path / "runtime.json"
    runtime.write_text("{}")
    with pytest.raises(ValueError, match="exact immutable production package"):
        module.prerequisites(
            tmp_path / "binary",
            runtime,
            runtime,
            tmp_path / "source",
            tmp_path / "renderer",
        )


def test_prerequisite_cannot_omit_original_identity_or_use_alias(monkeypatch, tmp_path):
    module = verifier(monkeypatch)
    work = tmp_path / "native"
    work.mkdir()
    case = {"case": "native", "original_files_sha256": {}}
    with pytest.raises(ValueError, match="complete prerequisite original"):
        module.original_files(tmp_path, case, {"original.txt"})
    source = tmp_path / "source.txt"
    source.write_text("original")
    (work / "original.txt").symlink_to(source)
    case["original_files_sha256"]["original.txt"] = module.checksum(source)
    with pytest.raises(ValueError, match="unchanged prerequisite original"):
        module.original_files(tmp_path, case, {"original.txt"})


def test_prerequisite_rejects_same_count_foreign_canary_and_non_boolean_pass(
    monkeypatch,
):
    module = verifier(monkeypatch)
    checks = dict.fromkeys(
        (
            "operation_closure_only",
            "no_gpu_nodes",
            "no_sysfs",
            "no_host_home",
            "no_session_bus",
            "no_worker_socket",
            "network_namespace_isolated",
            "descriptor_readonly",
            "original_source_readonly",
        ),
        True,
    )
    receipt = {
        "sandbox": {
            "policy": "harbor-cad-atmospheric-spectral-cpu-v1",
            "checks": checks,
        }
    }
    module.sandbox(receipt, receipt["sandbox"]["policy"], original_source=True)
    checks["original_source_readonly"] = 1
    with pytest.raises(ValueError, match="measured sandbox"):
        module.sandbox(receipt, receipt["sandbox"]["policy"], original_source=True)
    del checks["original_source_readonly"]
    checks["unrelated_readonly"] = True
    with pytest.raises(ValueError, match="measured sandbox"):
        module.sandbox(receipt, receipt["sandbox"]["policy"], original_source=True)
