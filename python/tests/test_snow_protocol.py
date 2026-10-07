"""Prescribed snow parity through the same real Rust worker and MCP client."""

import asyncio
import json
import os
import subprocess
import time
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator, ValidationError
from mcp import Client
from mcp.client.stdio import StdioServerParameters


def test_snow_prescription_schema_approval_and_typed_applicability_match_cli_and_real_mcp(
    tmp_path, monkeypatch
):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    repo = Path(__file__).parents[2]
    spec = json.loads((repo / "examples/snow-reference.json").read_text())
    schemas = json.loads(subprocess.check_output([binary, "schema"]))
    Draft202012Validator(schemas["SnowReferenceSpec"]).validate(spec)
    planned = json.loads(
        subprocess.check_output(
            [binary, "case", "plan-snow-reference", "/dev/stdin"],
            input=json.dumps(spec).encode(),
        )
    )
    validated = json.loads(
        subprocess.check_output(
            [binary, "case", "validate-snow-reference", "/dev/stdin"],
            input=json.dumps(spec).encode(),
        )
    )
    Draft202012Validator(schemas["ExecutionPlan"]).validate(planned["plan"])
    Draft202012Validator(schemas["PreparedSnowBoundary"]).validate(validated)
    assert planned["snow_boundary"] == validated and validated["executed"] is False
    assert planned["plan"]["thermal"] == validated["native"]
    spectral = json.loads((repo / "examples/spectral-reference.json").read_text())
    Draft202012Validator(schemas["SpectralReferenceSpec"]).validate(spectral)
    spectral_reference = json.loads(
        subprocess.check_output(
            [binary, "case", "validate-spectral-reference", "/dev/stdin"],
            input=json.dumps(spectral).encode(),
        )
    )
    Draft202012Validator(schemas["PreparedSpectralReference"]).validate(
        spectral_reference
    )
    assert not spectral_reference["executed"]
    assert abs(spectral_reference["absorbed_irradiance_w_m2"] - 132.0) < 1e-10
    spectral_plan = json.loads(
        subprocess.check_output(
            [binary, "case", "plan-spectral-reference", "/dev/stdin"],
            input=json.dumps(spectral).encode(),
        )
    )
    Draft202012Validator(schemas["ExecutionPlan"]).validate(spectral_plan["plan"])
    assert (
        spectral_plan["plan"]["schema_version"] == 13
        and spectral_plan["plan"]["observation"]["retained_times_s"] == []
    )
    for old_field in ("freezing", "wetting", "case", "thermal_contact", "atmosphere"):
        with pytest.raises(ValidationError):
            Draft202012Validator(schemas["ExecutionPlan"]).validate(
                {**spectral_plan["plan"], old_field: None}
            )
    reflection = json.loads(
        (repo / "examples/spectral-reflection-reference.json").read_text()
    )
    Draft202012Validator(schemas["SpectralReflectionSpec"]).validate(reflection)
    reflection_reference = json.loads(
        subprocess.check_output(
            [binary, "case", "validate-spectral-reflection-reference", "/dev/stdin"],
            input=json.dumps(reflection).encode(),
        )
    )
    Draft202012Validator(schemas["PreparedSpectralReflection"]).validate(
        reflection_reference
    )
    assert (
        not reflection_reference["executed"]
        and reflection_reference["model_relative_error_bound"] < 1e-5
    )
    atmosphere = json.loads((repo / "examples/atmosphere-reference.json").read_text())
    Draft202012Validator(schemas["AtmosphericReferenceSpec"]).validate(atmosphere)
    atmosphere_reference = json.loads(
        subprocess.check_output(
            [binary, "case", "validate-atmospheric-reference", "/dev/stdin"],
            input=json.dumps(atmosphere).encode(),
        )
    )
    Draft202012Validator(schemas["PreparedAtmosphericReference"]).validate(
        atmosphere_reference
    )
    assert (
        not atmosphere_reference["executed"] and len(atmosphere_reference["umu"]) == 64
    )
    atmosphere_plan = json.loads(
        subprocess.check_output(
            [binary, "case", "plan-atmospheric-reference", "/dev/stdin"],
            input=json.dumps(atmosphere).encode(),
        )
    )
    Draft202012Validator(schemas["ExecutionPlan"]).validate(atmosphere_plan["plan"])
    assert (
        atmosphere_plan["plan"]["schema_version"] == 14
        and atmosphere_plan["atmospheric_reference"] == atmosphere_reference
    )
    for field in ("case", "spectral", "freezing", "thermal_contact", "atmosphere"):
        with pytest.raises(ValidationError):
            Draft202012Validator(schemas["ExecutionPlan"]).validate(
                {**atmosphere_plan["plan"], field: None}
            )
    profile = json.loads((repo / "profiles/ci.json").read_text())
    profile.update(
        policy="research", service_mode="systemd", allowed_input_root=str(tmp_path)
    )
    profile_path = tmp_path / "profile.json"
    profile_path.write_text(json.dumps(profile))
    state = tmp_path / "state"
    socket = state / "worker.sock"
    worker = subprocess.Popen(
        [binary, "worker", "--state", str(state), "--profile", str(profile_path)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
    )
    try:
        deadline = time.monotonic() + 10
        while not socket.exists():
            assert worker.poll() is None and time.monotonic() < deadline
            time.sleep(0.01)
        monkeypatch.setenv("HARBOR_CAD_SOCKET", str(socket))

        async def exercise():
            parameters = StdioServerParameters(
                command=str(repo / ".venv/bin/python"),
                args=["-m", "harbor_cad_mcp.server", "--profile", "simulation"],
                env=dict(os.environ),
            )
            async with Client(parameters) as client:
                actual = await client.call_tool(
                    "case_plan_snow_reference", {"spec": spec}
                )
                assert not actual.is_error and actual.structured_content == planned
                actual = await client.call_tool(
                    "snow_reference_validate", {"spec": spec}
                )
                assert not actual.is_error and actual.structured_content == validated
                actual = await client.call_tool(
                    "spectral_reference_validate", {"spec": spectral}
                )
                assert (
                    not actual.is_error
                    and actual.structured_content == spectral_reference
                )
                actual = await client.call_tool(
                    "spectral_reference_plan", {"spec": spectral}
                )
                assert (
                    not actual.is_error and actual.structured_content == spectral_plan
                )
                rejected = await client.call_tool(
                    "job_submit",
                    {
                        "plan": spectral_plan["plan"],
                        "approved_digest": spectral_plan["approval_digest"],
                        "idempotency_key": "no-authority-spectral",
                    },
                )
                assert rejected.is_error and "unqualified" in str(rejected.content)
                actual = await client.call_tool(
                    "spectral_reflection_reference_validate", {"spec": reflection}
                )
                assert (
                    not actual.is_error
                    and actual.structured_content == reflection_reference
                )
                actual = await client.call_tool(
                    "atmospheric_reference_validate", {"spec": atmosphere}
                )
                assert (
                    not actual.is_error
                    and actual.structured_content == atmosphere_reference
                )
                actual = await client.call_tool(
                    "atmospheric_reference_plan", {"spec": atmosphere}
                )
                assert (
                    not actual.is_error and actual.structured_content == atmosphere_plan
                )
                rejected = await client.call_tool(
                    "job_submit",
                    {
                        "plan": atmosphere_plan["plan"],
                        "approved_digest": atmosphere_plan["approval_digest"],
                        "idempotency_key": "no-authority-atmosphere",
                    },
                )
                assert rejected.is_error and "unqualified" in str(rejected.content)
                for field, value in [
                    ("diffuse_isotropic", True),
                    ("backend", "hip"),
                    ("relative_tolerance", 0.1),
                    ("profile", "../../credentials"),
                ]:
                    rejected = await client.call_tool(
                        "atmospheric_reference_validate",
                        {"spec": {**atmosphere, field: value}},
                    )
                    assert rejected.is_error and "invalid_input" in str(
                        rejected.content
                    )
                for field, value in (
                    ("reflectance", 1.1),
                    ("maximum_model_error", 0.02),
                    ("reflectance_provenance", ""),
                    ("temperature_k", 273.15),
                ):
                    rejected = await client.call_tool(
                        "spectral_reflection_reference_validate",
                        {"spec": {**reflection, field: value}},
                    )
                    assert rejected.is_error and "invalid_input" in str(
                        rejected.content
                    )
                for field, value in (
                    ("precision", "Float64"),
                    ("variant", "cuda_ad_spectral"),
                    ("absorptivity", [0.2, 1.1]),
                    ("history_interpolation", "measured_solar_history"),
                ):
                    rejected = await client.call_tool(
                        "spectral_reference_validate",
                        {"spec": {**spectral, field: value}},
                    )
                    assert rejected.is_error and "invalid_input" in str(
                        rejected.content
                    )
                rejected = await client.call_tool(
                    "snow_reference_validate",
                    {"spec": {**spec, "snow": {**spec["snow"], "coverage": "partial"}}},
                )
                assert rejected.is_error and "invalid_input" in str(rejected.content)
                assert "dry snow" in str(rejected.content)
                rejected = await client.call_tool(
                    "job_submit",
                    {
                        "plan": planned["plan"],
                        "approved_digest": planned["approval_digest"],
                        "idempotency_key": "snow-without-authority",
                    },
                )
                assert rejected.is_error and "unqualified" in str(rejected.content)
                assert "authoritative same-user admission" in str(rejected.content)

        asyncio.run(exercise())
    finally:
        worker.terminate()
        worker.wait(timeout=5)
