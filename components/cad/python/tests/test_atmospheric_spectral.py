"""Original full-sphere atmospheric handoff without importing the renderer ABI."""

import copy
import hashlib
import importlib.util
import json
import math
from pathlib import Path
from types import SimpleNamespace

import pytest


def adapter(monkeypatch):
    directory = Path(__file__).parents[2] / "adapters"
    monkeypatch.syspath_prepend(str(directory))
    loader = importlib.util.spec_from_file_location(
        "atmospheric_spectral", directory / "atmospheric_spectral.py"
    )
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    return module


def fixture():
    repo = Path(__file__).parents[2]
    atmosphere = json.loads((repo / "examples/atmosphere-reference.json").read_text())
    receiver = json.loads((repo / "examples/atmosphere-transfer.json").read_text())[
        "receiver"
    ]
    atmosphere.update(mu_bins=8, phi_bins=8)
    for source in (atmosphere["toa_irradiance"], receiver["source"]["irradiance"]):
        for value in source:
            value["value"] = 1000.0
    raw = "\n".join(
        " ".join(
            [f"{value['value']:.3f}", "0", str(math.pi), "0"] + ["1"] * 64 + ["0"] * 64
        )
        for value in atmosphere["wavelengths"]
    )
    return atmosphere, receiver, raw


def test_diffuse_originals_keep_every_native_cell_units_and_separate_optical_reference(
    monkeypatch,
):
    native = adapter(monkeypatch)
    atmosphere, receiver, raw = fixture()
    normalized = native.normalize_source(atmosphere, receiver, raw)
    assert len(normalized["diffuse_emitters"]) == 128
    assert normalized["zero_diffuse_emitters"] == 64
    assert normalized["angular_shape"] == [3, 16, 8]
    assert normalized["transfer_relative_conservation_error"] <= 1e-10
    assert math.isclose(
        normalized["reference"]["incident"], 120.0 * math.pi, rel_tol=1e-14
    )
    assert (
        normalized["reference"]["absorbed"] == normalized["reference"]["incident"] / 2
    )
    assert normalized["reference"]["ageing"] == normalized["reference"]["incident"] / 4
    first = normalized["diffuse_emitters"][0]
    assert first["umu"] == -0.9375 and first["phi_deg"] == 22.5
    assert first["irradiance_w_m2_nm"] == [math.pi / 32] * 3
    assert all(
        math.isclose(a, b, rel_tol=1e-14)
        for a, b in zip(
            first["propagation_direction"],
            [
                math.sqrt(1 - 0.9375**2) * math.sin(math.pi / 8),
                math.sqrt(1 - 0.9375**2) * math.cos(math.pi / 8),
                -0.9375,
            ],
            strict=True,
        )
    )
    receiver["sensor_normal"] = [0, 0, -1]
    assert (
        native.normalize_source(atmosphere, receiver, raw)["reference"]["incident"]
        == 0.0
    )


def test_native_phi_propagation_is_not_reversed_or_replaced_with_isotropic_sky(
    monkeypatch,
):
    native = adapter(monkeypatch)
    atmosphere, receiver, _ = fixture()
    raw = "\n".join(
        " ".join(
            [f"{v['value']:.3f}", "0", str(math.pi / 8), "0"]
            + [
                "1" if mu < 8 and phi == 0 else "0"
                for mu in range(16)
                for phi in range(8)
            ]
        )
        for v in atmosphere["wavelengths"]
    )
    receiver["sensor_normal"] = [0, 1, 0]
    assert (
        native.normalize_source(atmosphere, receiver, raw)["reference"]["incident"]
        == 0.0
    )
    receiver["sensor_normal"] = [0, -1, 0]
    assert (
        native.normalize_source(atmosphere, receiver, raw)["reference"]["incident"]
        > 0.0
    )


@pytest.mark.parametrize(
    "change", ["knots", "direction", "toa", "occlusion", "radiance"]
)
def test_handoff_rejects_changed_original_inputs_and_unsupported_visibility(
    monkeypatch, change
):
    native = adapter(monkeypatch)
    atmosphere, receiver, raw = fixture()
    receiver = copy.deepcopy(receiver)
    if change == "knots":
        receiver["wavelengths"][0]["value"] += 0.001
    elif change == "direction":
        receiver["source"]["propagation_direction"] = [0, 0, -1]
    elif change == "toa":
        receiver["source"]["irradiance"][0]["value"] *= 2
    elif change == "occlusion":
        receiver["occlusion"] = "full_directional_occluder"
    else:
        raw = raw.replace("1 1 1", "100 1 1", 1)
    with pytest.raises(ValueError):
        native.normalize_source(atmosphere, receiver, raw)


@pytest.mark.parametrize(
    "refusal", ["checksum", "duplicate", "missing_policy", "writable_original"]
)
def test_entrypoint_refuses_unbound_or_writable_originals_before_native_outputs(
    monkeypatch, tmp_path, refusal
):
    native = adapter(monkeypatch)
    atmosphere, receiver, original = fixture()
    request = {
        "schema_version": 1,
        "atmosphere": atmosphere,
        "receiver": receiver,
        "original_path": "/inputs/atmosphere-original.txt",
        "original_sha256": hashlib.sha256(original.encode()).hexdigest(),
    }
    if refusal == "checksum":
        request["original_sha256"] = "0" * 64
    raw = json.dumps(request).encode()
    if refusal == "duplicate":
        raw = raw.replace(
            b'"schema_version": 1', b'"schema_version": 1, "schema_version": 1', 1
        )
    monkeypatch.setattr(
        native.sandbox_foundation,
        "read_regular",
        lambda path, limit: (
            raw if str(path) == "/inputs/request.json" else original.encode()
        ),
    )
    monkeypatch.setattr(
        native.sandbox_foundation,
        "cpu_sandbox",
        lambda *args: None if refusal == "missing_policy" else {"checks": {}},
    )
    monkeypatch.setattr(native.os, "statvfs", lambda path: SimpleNamespace(f_flag=0))
    monkeypatch.setattr(
        native.sys,
        "argv",
        ["harbor-cad-atmospheric-spectral", "reference", "/inputs/request.json"],
    )
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError):
        native.main()
    assert list(tmp_path.iterdir()) == []


def test_entrypoint_accepts_only_owned_transport_capture_before_native_dispatch(
    monkeypatch, tmp_path
):
    native = adapter(monkeypatch)
    atmosphere, receiver, original = fixture()
    request = {
        "schema_version": 1,
        "atmosphere": atmosphere,
        "receiver": receiver,
        "original_path": "/inputs/atmosphere-original.txt",
        "original_sha256": hashlib.sha256(original.encode()).hexdigest(),
    }
    raw = json.dumps(request).encode()
    monkeypatch.setattr(
        native.sandbox_foundation,
        "read_regular",
        lambda path, limit: (
            raw if str(path) == "/inputs/request.json" else original.encode()
        ),
    )
    monkeypatch.setattr(
        native.sandbox_foundation, "cpu_sandbox", lambda *args: {"checks": {}}
    )
    monkeypatch.setattr(
        native.os, "statvfs", lambda path: SimpleNamespace(f_flag=native.os.ST_RDONLY)
    )
    monkeypatch.setattr(
        native.sys,
        "argv",
        ["harbor-cad-atmospheric-spectral", "reference", "/inputs/request.json"],
    )
    monkeypatch.chdir(tmp_path)
    capture = tmp_path / "atmospheric-transport.log"
    capture.write_bytes(b"owned worker capture")

    class NativeDispatchReached(Exception):
        pass

    def dispatched(*args):
        raise NativeDispatchReached

    monkeypatch.setattr(native, "execute", dispatched)
    with pytest.raises(NativeDispatchReached):
        native.main()
    assert capture.read_bytes() == b"owned worker capture"
    extra = tmp_path / "original.csv"
    extra.write_bytes(b"retained scientific original")
    with pytest.raises(ValueError, match="new bounded native reference"):
        native.main()
    extra.unlink()
    capture.unlink()
    capture.symlink_to(tmp_path / "absent.log")
    with pytest.raises(ValueError, match="new bounded native reference"):
        native.main()
