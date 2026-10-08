"""Development campaign subprocesses must keep their isolated virtualenv ABI."""

import asyncio
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path

from mcp import Client
from mcp.client.stdio import StdioServerParameters


def test_development_mcp_keeps_virtualenv_interpreter_and_imports_sdk(monkeypatch):
    scripts = Path(__file__).parents[2] / "scripts"
    monkeypatch.syspath_prepend(str(scripts))
    loader = importlib.util.spec_from_file_location(
        "retention_campaign", scripts / "verify_wetting_retention.py"
    )
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    invoked = Path(sys.executable).absolute()
    command = module.mcp_executable(invoked, development=True)
    result = json.loads(
        subprocess.check_output(
            [
                str(command),
                "-c",
                "import json,sys;from mcp import Client;print(json.dumps({'prefix':sys.prefix,'sdk':Client.__name__}))",
            ],
            env={"PATH": "/nonexistent", "PYTHONDONTWRITEBYTECODE": "1"},
        )
    )
    assert result == {"prefix": sys.prefix, "sdk": "Client"}
    assert command == invoked


def test_cli_and_real_results_mcp_agree_on_unknown_retention_fields(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    repo = Path(__file__).parents[2]
    state = tmp_path / "state"
    profile = tmp_path / "profile.json"
    settings = json.loads((repo / "profiles/ci.json").read_text())
    settings["allowed_input_root"] = str(tmp_path)
    profile.write_text(json.dumps(settings))
    environment = {
        **os.environ,
        "HARBOR_CAD_SOCKET": str(state / "worker.sock"),
        "PYTHONPATH": str(repo / "python"),
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    worker = subprocess.Popen(
        [binary, "worker", "--state", str(state), "--profile", str(profile)],
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    request = {
        "schema_version": 1,
        "source_job": "00000000-0000-0000-0000-000000000001",
        "physical_time_s": 0.0,
        "extrusion": {"value": 1.0, "unit": "mm"},
        "extrusion_provenance": "synthetic explicit depth",
        "destination_region": "retained_phase",
        "destination_origin_m": [0.0, 0.0, 0.0],
        "maximum_relative_conservation_error": 1e-10,
    }
    try:
        deadline = time.monotonic() + 10
        while not (state / "worker.sock").exists():
            assert worker.poll() is None and time.monotonic() < deadline, (
                worker.communicate(timeout=5)
            )
            time.sleep(0.01)

        async def exercise():
            parameters = StdioServerParameters(
                command=sys.executable,
                args=["-m", "harbor_cad_mcp.server", "--profile", "results"],
                env=environment,
            )
            async with Client(parameters) as client:
                for value in (273.15, None):
                    invalid = {**request, "temperature_k": value}
                    cli = await asyncio.to_thread(
                        subprocess.run,
                        [binary, "results", "retain-wetting", "/dev/stdin"],
                        input=json.dumps(invalid).encode(),
                        env=environment,
                        capture_output=True,
                        timeout=30,
                        check=False,
                    )
                    mcp = await client.call_tool(
                        "results_retain_wetting", {"request_spec": invalid}
                    )
                    assert cli.returncode != 0
                    assert json.loads(cli.stdout)["error"]["code"] == "invalid_input"
                    assert mcp.is_error and "invalid_input" in str(mcp.content)

        asyncio.run(exercise())
    finally:
        worker.terminate()
        worker.communicate(timeout=5)
