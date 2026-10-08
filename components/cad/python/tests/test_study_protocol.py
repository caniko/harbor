"""Real worker study approval, all-case refusal, stable retries and CLI/MCP parity."""

import asyncio
import copy
import json
import os
import sqlite3
import subprocess
import time
from pathlib import Path

from jsonschema import Draft202012Validator
from mcp import Client
from mcp.client.stdio import StdioServerParameters


def test_study_cli_mcp_original_approvals_preflight_and_restart(tmp_path):
    binary = os.environ["HARBOR_CAD_TEST_BINARY"]
    repo = Path(__file__).parents[2]
    state = tmp_path / "state"
    endpoint = state / "worker.sock"
    profile = tmp_path / "profile.json"
    worker = None

    def start():
        process = subprocess.Popen(
            [binary, "worker", "--state", str(state), "--profile", str(profile)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        deadline = time.monotonic() + 5
        while not endpoint.exists():
            assert process.poll() is None, process.stderr.read().decode()
            assert time.monotonic() < deadline
            time.sleep(0.01)
        return process

    def command(*arguments, allow_error=False):
        process = subprocess.run(
            [binary, *map(str, arguments)], capture_output=True, timeout=30, check=False
        )
        value = json.loads(process.stdout)
        if not allow_error:
            assert process.returncode == 0, value
        return value

    fixture = command("case", "init")
    cases = []
    for name, force in (("baseline", 0.1), ("less_force", 0.05)):
        case = copy.deepcopy(fixture)
        case["acceleration"]["value"] = force
        request = tmp_path / f"case-{name}.json"
        request.write_text(json.dumps(case))
        planned = command("case", "plan", request)
        cases.append(
            {
                "name": name,
                "plan": planned["plan"],
                "approved_digest": planned["approval_digest"],
            }
        )
    study = {
        "schema_version": 1,
        "name": "explicit_force_comparison",
        "provenance": "synthetic explicit parameters with unchanged original field gates",
        "cases": cases,
        "max_total_artifact_bytes": sum(
            c["plan"]["observation"]["max_artifact_bytes"] for c in cases
        ),
    }
    host = json.loads((repo / "profiles/ci.json").read_text())
    host["max_ram_bytes"] = max(
        stage["ram_bytes"] for case in cases for stage in case["plan"]["stages"]
    )
    host["max_disk_bytes"] = study["max_total_artifact_bytes"] * 2
    profile.write_text(json.dumps(host))
    schemas = command("schema")
    Draft202012Validator(schemas["StudyRequest"]).validate(study)
    original = tmp_path / "study.json"
    original.write_text(json.dumps(study))
    prepared = command("study", "prepare", original, "--policy", "ci")
    Draft202012Validator(schemas["PreparedStudy"]).validate(prepared)
    assert (
        prepared["executed"] is False
        and prepared["physical_validation"] == "unqualified"
    )

    def counts():
        with sqlite3.connect(
            "file:" + str(state / "jobs.sqlite3") + "?mode=ro", uri=True
        ) as connection:
            return [
                connection.execute("select count(*) from " + table).fetchone()[0]
                for table in ("jobs", "studies")
            ]

    try:
        worker = start()

        async def exercise():
            parameters = StdioServerParameters(
                command=os.sys.executable,
                args=["-m", "harbor_cad_mcp.server", "--profile", "simulation"],
                env={**os.environ, "HARBOR_CAD_SOCKET": str(endpoint)},
            )
            async with Client(parameters) as client:
                response = await client.call_tool("study_prepare", {"study": study})
                assert not response.is_error and response.structured_content == prepared
                before = counts()
                invalid = copy.deepcopy(study)
                invalid["cases"][1]["approved_digest"] = "0" * 64
                rejected = await client.call_tool(
                    "study_submit", {"study": invalid, "idempotency_key": "bad"}
                )
                assert rejected.is_error and "invalid_input" in rejected.content[0].text
                assert counts() == before
                # A valid first case cannot be queued when a later valid plan
                # exceeds the worker's effective RAM budget.
                larger_spec = copy.deepcopy(fixture)
                larger_spec["resolution"] = 65
                larger_file = tmp_path / "larger.json"
                larger_file.write_text(json.dumps(larger_spec))
                larger = command("case", "plan", larger_file)
                unsupported = copy.deepcopy(study)
                unsupported["cases"][1] = {
                    "name": "larger",
                    "plan": larger["plan"],
                    "approved_digest": larger["approval_digest"],
                }
                unsupported["max_total_artifact_bytes"] = sum(
                    c["plan"]["observation"]["max_artifact_bytes"]
                    for c in unsupported["cases"]
                )
                response = await client.call_tool(
                    "study_submit",
                    {"study": unsupported, "idempotency_key": "over_budget"},
                )
                assert (
                    response.is_error
                    and "resource_unavailable" in response.content[0].text
                )
                assert counts() == before
                accepted = command(
                    "--socket",
                    endpoint,
                    "study",
                    "submit",
                    original,
                    "--idempotency-key",
                    "same-intent",
                )["data"]
                response = await client.call_tool(
                    "study_submit", {"study": study, "idempotency_key": "same-intent"}
                )
                assert (
                    not response.is_error
                    and response.structured_content["id"] == accepted["id"]
                )
                assert [
                    v["job"]["id"] for v in response.structured_content["cases"]
                ] == [v["job"]["id"] for v in accepted["cases"]]
                return accepted

        accepted = asyncio.run(exercise())
        deadline = time.monotonic() + 20
        while True:
            status = command("--socket", endpoint, "study", "status", accepted["id"])[
                "data"
            ]
            if status["execution"] == "completed_successfully":
                break
            assert time.monotonic() < deadline, status
            time.sleep(0.02)
        worker.terminate()
        worker.wait(timeout=5)
        endpoint.unlink(missing_ok=True)
        worker = start()
        repeat = command(
            "--socket",
            endpoint,
            "study",
            "submit",
            original,
            "--idempotency-key",
            "same-intent",
        )["data"]
        assert repeat["id"] == accepted["id"] and counts() == [2, 1]
        assert repeat["cases"] == status["cases"]
        assert (
            status["physical_validation"] == "unqualified"
            and status["submission"] == "complete"
        )
        assert [c["name"] for c in status["cases"]] == [c["name"] for c in cases]
        assert [c["plan_digest"] for c in status["cases"]] == [
            c["approved_digest"] for c in cases
        ]
        for case in status["cases"]:
            command(
                "artifact",
                "export",
                "--state",
                state,
                case["job"]["id"],
                tmp_path / ("export-" + case["name"]),
            )
        changed = copy.deepcopy(study)
        changed["provenance"] += " different intent"
        original.write_text(json.dumps(changed))
        rejected = command(
            "--socket",
            endpoint,
            "study",
            "submit",
            original,
            "--idempotency-key",
            "same-intent",
            allow_error=True,
        )
        assert (
            rejected["ok"] is False
            and rejected["error"]["code"] == "idempotency_conflict"
        )
        assert counts() == [2, 1]
    finally:
        if worker is not None:
            worker.terminate()
            worker.wait(timeout=5)
