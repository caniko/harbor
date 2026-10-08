"""Versioned native freezing approval parity through a real MCP client/worker."""

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


def test_exact_freezing_plan_schema_and_real_worker_mcp_preserve_approval_and_native_boundaries(
    tmp_path, monkeypatch
):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    repo = Path(__file__).parents[2]
    spec = json.loads((repo / "examples/freezing-reference.json").read_text())
    approved = json.loads(
        subprocess.check_output(
            [binary, "case", "plan-freezing-reference", "/dev/stdin"],
            input=json.dumps(spec).encode(),
        )
    )
    schemas = json.loads(subprocess.check_output([binary, "schema"]))
    validator = Draft202012Validator(schemas["ExecutionPlan"])
    validator.validate(approved["plan"])
    assert approved["plan"]["schema_version"] == 12
    for key in (
        "case",
        "thermal",
        "wetting",
        "contact",
        "thermal_contact",
        "fem",
        "source",
        "filter",
        "cad_source",
    ):
        with pytest.raises(ValidationError):
            validator.validate({**approved["plan"], key: None})
    old = json.loads(
        subprocess.check_output(
            [binary, "case", "plan", "/dev/stdin"],
            input=subprocess.check_output([binary, "case", "init"]),
        )
    )["plan"]
    for injected in (spec, None):
        with pytest.raises(ValidationError):
            validator.validate({**old, "freezing": injected})
    profile = json.loads((repo / "profiles/ci.json").read_text())
    profile["policy"] = "research"
    profile["service_mode"] = "systemd"
    profile_path = tmp_path / "profile.json"
    profile_path.write_text(json.dumps(profile))
    state = tmp_path / "state"
    socket = state / "worker.sock"
    monkeypatch.setenv("HARBOR_CAD_SOCKET", str(socket))
    process = subprocess.Popen(
        [binary, "worker", "--state", str(state), "--profile", str(profile_path)]
    )
    try:
        for _ in range(100):
            if socket.exists():
                break
            assert process.poll() is None
            time.sleep(0.01)
        assert socket.exists()

        async def check():
            parameters = StdioServerParameters(
                command=os.sys.executable,
                args=["-m", "harbor_cad_mcp.server", "--profile", "simulation"],
                env=dict(os.environ),
            )
            async with Client(parameters) as client:
                response = await client.call_tool(
                    "case_plan_freezing_reference", {"spec": spec}
                )
                assert not response.is_error
                assert response.structured_content == approved
                rejected = await client.call_tool(
                    "case_plan_freezing_reference",
                    {"spec": {**spec, "temperature_tolerance": 0.03}},
                )
                assert rejected.is_error and "invalid_input" in str(rejected.content)
                submitted = await client.call_tool(
                    "job_submit",
                    {
                        "plan": approved["plan"],
                        "approved_digest": approved["approval_digest"],
                        "idempotency_key": "native-freezing-needs-authority",
                    },
                )
                assert submitted.is_error and "unqualified" in str(submitted.content)
                described = await client.call_tool("backend_list")
                assert not described.is_error

        asyncio.run(check())
    finally:
        process.terminate()
        process.wait(timeout=5)
