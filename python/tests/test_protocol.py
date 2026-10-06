import asyncio
import json
import os
import subprocess
import time
from pathlib import Path

import pytest
from harbor_cad_mcp.server import build_server
from jsonschema import Draft202012Validator, ValidationError


def binary() -> str:
    return os.environ["HARBOR_CAD_TEST_BINARY"]


def cold_inputs():
    missing = {
        "availability": "missing",
        "reason": "physical input unavailable in this synthetic contract fixture",
    }
    return {
        "schema_version": 1,
        "synthetic": True,
        "geometry_sha256": "a" * 64,
        "thermal_region": "enclosure",
        "material": {
            key: dict(missing) for key in ("density", "conductivity", "specific_heat")
        },
        "minimum_valid_temperature": {"value": 200, "unit": "K"},
        "maximum_valid_temperature": {"value": 400, "unit": "K"},
        "initial_temperature": {"value": -40, "unit": "degC"},
        "duration": {"value": 1, "unit": "h"},
        "history_interpolation": "piecewise_linear",
        "ambient_history": dict(missing),
        "heater_history": dict(missing),
        "convection_coefficient": dict(missing),
        "observation_times": [{"value": 0, "unit": "s"}, {"value": 1, "unit": "h"}],
        "numerical_tolerance": 0.01,
        "moisture": {
            "assessment": "missing",
            "reason": "humidity and surface history missing",
        },
    }


def test_schema_parity_and_unknown_input_rejection():
    schemas = json.loads(subprocess.check_output([binary(), "schema"]))
    case = json.loads(subprocess.check_output([binary(), "case", "init"]))
    Draft202012Validator(schemas["CaseSpec"]).validate(case)
    cold = cold_inputs()
    Draft202012Validator(schemas["ColdRestartSpec"]).validate(cold)
    cold["heater_history"]["value"] = []
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ColdRestartSpec"]).validate(cold)
    planned = json.loads(
        subprocess.check_output(
            [binary(), "case", "plan", "/dev/stdin"], input=json.dumps(case).encode()
        )
    )["plan"]
    Draft202012Validator(schemas["ExecutionPlan"]).validate(planned)
    injected = dict(planned, source=None)
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ExecutionPlan"]).validate(injected)
    injected_filter = dict(planned, filter={"time_s": 0, "field": "velocity"})
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ExecutionPlan"]).validate(injected_filter)
    missing_filter = dict(planned, schema_version=4)
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ExecutionPlan"]).validate(missing_filter)
    incomplete_v2 = dict(planned, schema_version=2)
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ExecutionPlan"]).validate(incomplete_v2)
    case["execute_python"] = "print('no')"
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["CaseSpec"]).validate(case)


def test_real_mcp_client_and_rust_worker(tmp_path, monkeypatch):
    from mcp import Client
    from mcp.client.stdio import StdioServerParameters

    root = tmp_path / "state"
    profile = Path(__file__).parents[2] / "profiles" / "ci.json"
    worker = subprocess.Popen(
        [binary(), "worker", "--state", str(root), "--profile", str(profile.resolve())]
    )
    socket = root / "worker.sock"
    monkeypatch.setenv("HARBOR_CAD_SOCKET", str(socket))
    try:
        deadline = time.monotonic() + 5
        while not socket.exists():
            assert time.monotonic() < deadline
            time.sleep(0.01)
        case = json.loads(subprocess.check_output([binary(), "case", "init"]))
        schemas = json.loads(subprocess.check_output([binary(), "schema"]))
        result = json.loads(
            subprocess.check_output(
                [binary(), "case", "plan", "/dev/stdin"],
                input=json.dumps(case).encode(),
            )
        )

        expected_cold = json.loads(
            subprocess.check_output(
                [binary(), "case", "validate-cold-restart", "/dev/stdin"],
                input=json.dumps(cold_inputs()).encode(),
            )
        )

        async def check():
            params = StdioServerParameters(
                command=str(Path(os.sys.executable)),
                args=["-m", "harbor_cad_mcp.server", "--profile", "all"],
                env=dict(os.environ),
            )
            async with Client(params) as client:
                names = {t.name for t in (await client.list_tools()).tools}
                assert "job_submit" in names and "results_describe" in names
                assert "case_plan_openlb_reference" in names
                assert "cad_plan_inspection" in names
                assert "cad_regions" in names and "cad_submit" in names
                assert "cold_restart_validate" in names
                assert "filter_plan" in names
                assert "case_plan_fem_reference" in names
                assert "case_plan_thermal_reference" in names
                from test_thermal_history import fixture

                rejected_thermal = await client.call_tool(
                    "case_plan_thermal_reference", {"spec": fixture()}
                )
                assert rejected_thermal.is_error
                cold = cold_inputs()
                checked = await client.call_tool(
                    "cold_restart_validate", {"case": cold}
                )
                assert not checked.is_error, checked
                report = checked.structured_content
                assert report["inputs_complete"] is False
                assert report["prescribed_heater_energy_j"] is None
                assert report["execution"] == "not_requested"
                assert report["physical_validation"] == "unqualified"
                assert report == expected_cold
                assert not any(x in names for x in ("shell", "python", "install"))
                # CI cannot implicitly turn a reference into an unsandboxed
                # native job. Planning rejection is returned by the same worker.
                native_case = dict(case)
                native_case["applicability"] = dict(
                    case["applicability"], formulation="periodic_forced_channel"
                )
                rejected = await client.call_tool(
                    "case_plan_openlb_reference", {"case": native_case}
                )
                assert rejected.is_error
                inspected = dict(
                    case,
                    geometry={
                        "source": "approved.FCStd",
                        "sha256": "a" * 64,
                        "synthetic": True,
                    },
                )
                rejected = await client.call_tool(
                    "cad_plan_inspection", {"case": inspected}
                )
                assert rejected.is_error
                response = await client.call_tool(
                    "job_submit",
                    {
                        "plan": result["plan"],
                        "approved_digest": result["approval_digest"],
                        "idempotency_key": "real-mcp-client",
                    },
                )
                job = response.structured_content
                assert job and job["id"], response
                deadline = time.monotonic() + 10
                while True:
                    status = (
                        await client.call_tool("job_status", {"job_id": job["id"]})
                    ).structured_content
                    if status["state"] == "succeeded":
                        break
                    assert status["state"] != "failed", status
                    assert time.monotonic() < deadline
                    await asyncio.sleep(0.02)
                duplicate = (
                    await client.call_tool(
                        "job_submit",
                        {
                            "plan": result["plan"],
                            "approved_digest": result["approval_digest"],
                            "idempotency_key": "real-mcp-client",
                        },
                    )
                ).structured_content
                assert duplicate["id"] == job["id"]
                description = (
                    await client.call_tool("results_describe", {"job_id": job["id"]})
                ).structured_content
                assert "velocity_m_s" not in json.dumps(description)
                assert len(description["artifacts"]["items"]) == 5
                Draft202012Validator(schemas["ExecutionBinding"]).validate(
                    description["execution_binding"]
                )
                assert description["artifacts"]["next_after"] is None
                evidence = (
                    await client.call_tool(
                        "qualification_report", {"job_id": job["id"]}
                    )
                ).structured_content
                Draft202012Validator(schemas["JobEvidenceReport"]).validate(evidence)
                assert evidence["execution_id"] == result["approval_digest"]
                assert evidence["scope"] == "historical_job_evidence"
                assert evidence["current_runtime_qualification"] == "not_assessed"
                assert evidence["physical_validation"] == "unqualified"
                assert evidence["capabilities"][0]["runtime_execution"] == "recorded"
                assert (
                    evidence["capabilities"][0]["numerical_verification"]
                    == "reported_pass"
                )
                artifacts = await client.call_tool(
                    "artifact_list", {"job_id": job["id"], "limit": 1}
                )
                page = artifacts.structured_content
                assert len(page["items"]) == 1 and page["total"] == 5
                remaining = await client.call_tool(
                    "artifact_list", {"job_id": job["id"], "after": page["next_after"]}
                )
                assert len(remaining.structured_content["items"]) == 4
                assert remaining.structured_content["next_after"] is None
            async with Client(build_server("results")) as client:
                names = {t.name for t in (await client.list_tools()).tools}
                assert "job_submit" not in names and "results_describe" in names
                assert "qualification_report" in names
                assert "cad_plan_inspection" not in names
                assert "cad_regions" in names and "cad_submit" not in names
                assert "cold_restart_validate" not in names
                assert "filter_plan" not in names
                assert "case_plan_fem_reference" not in names
                assert "case_plan_thermal_reference" not in names
                assert {"render_plan", "video_plan", "presentation_submit"}.issubset(
                    names
                )
                rejected = await client.call_tool(
                    "presentation_submit",
                    {
                        "plan": result["plan"],
                        "approved_digest": result["approval_digest"],
                        "idempotency_key": "no-simulation-in-results",
                    },
                )
                assert rejected.is_error
            async with Client(build_server("cad")) as client:
                names = {t.name for t in (await client.list_tools()).tools}
                assert "cad_plan_inspection" in names and "job_submit" not in names
                assert "cad_regions" in names and "cad_submit" in names
                assert "job_status" in names and "job_logs" in names
                assert "case_plan_fem_reference" not in names
                assert "case_plan_thermal_reference" not in names
                rejected = await client.call_tool("cad_regions", {"job_id": job["id"]})
                assert rejected.is_error
                rejected = await client.call_tool(
                    "cad_submit",
                    {
                        "plan": result["plan"],
                        "approved_digest": result["approval_digest"],
                        "idempotency_key": "wrong-cad-operation",
                    },
                )
                assert rejected.is_error

        asyncio.run(check())
    finally:
        worker.terminate()
        worker.wait(timeout=5)
