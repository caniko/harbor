"""Strict approved one-way coupling surfaces; no native import or solve implied."""

import asyncio
import json
import os
import subprocess
import time
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator, ValidationError


def fixture():
    return json.loads(
        (
            Path(__file__).resolve().parents[2] / "examples/thermal-contact.json"
        ).read_text()
    )


def test_native_coupling_cli_schema_keeps_independent_legacy_approvals(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    spec = fixture()
    path = tmp_path / "spec.json"
    path.write_text(json.dumps(spec))
    result = subprocess.run(
        [binary, "case", "plan-thermal-contact", str(path)],
        capture_output=True,
        check=True,
    )
    planned = json.loads(result.stdout)
    schemas = json.loads(subprocess.check_output([binary, "schema"]))
    validator = Draft202012Validator(schemas["ExecutionPlan"])
    validator.validate(planned["plan"])
    assert planned["plan"]["thermal_contact"] == spec
    assert planned["plan"]["schema_version"] == 11
    assert "contact" not in planned["plan"]
    assert "thermal" not in planned["plan"]
    assert "final_temperatures_k" not in spec["mechanical"]
    for key in ("contact", "thermal", "case", "wetting", "source"):
        injected = {**planned["plan"], key: None}
        with pytest.raises(ValidationError):
            validator.validate(injected)
    for version in range(1, 11):
        with pytest.raises(ValidationError):
            validator.validate({**planned["plan"], "schema_version": version})
    spec["mechanical"]["final_temperatures_k"] = [273.15, 283.15]
    path.write_text(json.dumps(spec))
    rejected = subprocess.run(
        [binary, "case", "plan-thermal-contact", str(path)],
        capture_output=True,
        check=False,
    )
    assert rejected.returncode != 0


def test_real_mcp_coupling_planning_matches_cli_and_rejects_unauthorized_submission(
    tmp_path,
):
    from mcp import Client
    from mcp.client.stdio import StdioServerParameters

    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    state = tmp_path / "state"
    profile = tmp_path / "profile.json"
    profile.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "policy": "research",
                "allowed_input_root": str(tmp_path),
                "max_ram_bytes": 2 * 1024**3,
                "max_disk_bytes": 2 * 1024**3,
                "threads": 1,
                "timeout_seconds": 30,
                "native_runtime": None,
                "service_mode": "systemd",
            }
        )
    )
    request = tmp_path / "spec.json"
    request.write_text(json.dumps(fixture()))
    cli = json.loads(
        subprocess.check_output([binary, "case", "plan-thermal-contact", str(request)])
    )
    worker = subprocess.Popen(
        [binary, "worker", "--state", str(state), "--profile", str(profile)],
        stdout=subprocess.DEVNULL,
    )
    socket = state / "worker.sock"
    try:
        deadline = time.monotonic() + 5
        while not socket.exists():
            assert worker.poll() is None and time.monotonic() < deadline
            time.sleep(0.01)

        async def check():
            params = StdioServerParameters(
                command=os.sys.executable,
                args=["-m", "harbor_cad_mcp.server", "--profile", "simulation"],
                env={**os.environ, "HARBOR_CAD_SOCKET": str(socket)},
            )
            async with Client(params) as client:
                result = await client.call_tool(
                    "case_plan_thermal_contact", {"spec": fixture()}
                )
                assert not result.is_error and result.structured_content == cli
                result = await client.call_tool(
                    "job_submit",
                    {
                        "plan": cli["plan"],
                        "approved_digest": cli["approval_digest"],
                        "idempotency_key": "no-authority-coupling",
                    },
                )
                assert result.is_error
                assert "authoritative same-user admission" in str(result.content)

        asyncio.run(check())
    finally:
        worker.terminate()
        worker.wait(timeout=5)
