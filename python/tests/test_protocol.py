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


def test_schema_parity_and_unknown_input_rejection():
    schemas = json.loads(subprocess.check_output([binary(), "schema"]))
    case = json.loads(subprocess.check_output([binary(), "case", "init"]))
    Draft202012Validator(schemas["CaseSpec"]).validate(case)
    planned = json.loads(
        subprocess.check_output(
            [binary(), "case", "plan", "/dev/stdin"], input=json.dumps(case).encode()
        )
    )["plan"]
    Draft202012Validator(schemas["ExecutionPlan"]).validate(planned)
    injected = dict(planned, source=None)
    with pytest.raises(ValidationError):
        Draft202012Validator(schemas["ExecutionPlan"]).validate(injected)
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
                assert "cad_plan_inspection" not in names
                assert "render_plan" in names and "presentation_submit" in names
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

        asyncio.run(check())
    finally:
        worker.terminate()
        worker.wait(timeout=5)
