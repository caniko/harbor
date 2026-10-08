"""Exact native CPU studies, CLI/MCP parity, original fields and durable recovery."""

import argparse
import asyncio
import copy
import json
import sqlite3
import subprocess
from itertools import pairwise
from pathlib import Path

from atmospheric_transport_prerequisites import immutable, sandbox
from native_worker_campaign import WorkerCampaign
from verify_contact_cpu import load
from verify_native_cpu import verify_manifest
from verify_spectral_cpu import checksum
from verify_spectral_worker import mcp_call
from verify_systemd import (
    retention_snapshot,
    wait_admission_release,
    wait_retention_release,
)
from verify_thermal_cpu import temporal_self_convergence


def prerequisites(runtime, source, repo):
    """Reconstruct every original thermal reference and the separate refinement."""
    report_file = source / "verification.json"
    report = json.loads(report_file.read_text())
    standalone = immutable(report["runtime"])
    declared, native = (
        json.loads(standalone.read_text()),
        json.loads(runtime.read_text()),
    )
    if (
        report["runtime_sha256"] != checksum(standalone)
        or declared["thermal"] != native["thermal"]
        or declared["thermal_closure"] != native["thermal_closure"]
        or declared["bwrap"] != native["bwrap"]
        or report["adapter"] != native["thermal"]
        or len(report["results"]) != 8
        or len(report["rejections"]) != 9
    ):
        raise ValueError(
            "complete matching exact native thermal prerequisites required"
        )
    fem = load("study_fem", repo / "adapters/fem_reference.py")
    thermal = load("study_thermal", repo / "adapters/thermal_history.py")
    cases, observations, meshes = {}, {}, {}
    for case in report["results"]:
        spec = case["request"]
        thermal.validate(spec)
        if case["recipe"] not in {"adiabatic_heater", "cold_restart_robin"}:
            raise ValueError("known original thermal prerequisite recipe required")
        key = (case["recipe"], spec["resolution"], spec["max_step_s"])
        if key in cases:
            raise ValueError("unique complete thermal prerequisite cases required")
        label = f"{key[0]}-n{key[1]}-dt{key[2]:g}"
        work = source / label
        if work.is_symlink() or not work.is_dir():
            raise ValueError("closed original prerequisite directory required")
        for name, record in case["artifacts"].items():
            path = work / name
            if (
                Path(name).name != name
                or path.is_symlink()
                or not path.is_file()
                or path.stat().st_size != record["bytes"]
                or checksum(path) != record["sha256"]
            ):
                raise ValueError("unchanged original prerequisite bytes required")
        if (
            not {
                "mesh.json",
                "reference.dat",
                "thermal-fields.json",
                "thermal-receipt.json",
            }
            <= case["artifacts"].keys()
        ):
            raise ValueError("complete native thermal originals required")
        mesh = json.loads((work / "mesh.json").read_text())
        nodes, cells = (
            {int(k): v for k, v in mesh[name].items()} for name in ("nodes", "elements")
        )
        checks, _, fields = thermal.verify(
            spec, nodes, cells, fem.read_dat((work / "reference.dat").read_text())
        )
        receipt = json.loads((work / "thermal-receipt.json").read_text())
        observed = json.loads((work / "thermal-fields.json").read_text())
        if (
            receipt != case["receipt"]
            or receipt["numerical_verification"] != checks
            or receipt["native_field_sha256"] != checksum(work / "reference.dat")
            or receipt["mesh_sha256"] != checksum(work / "mesh.json")
            or observed["times"] != json.loads(json.dumps(fields))
        ):
            raise ValueError(
                "complete independently reconstructed original thermal fields required"
            )
        cases[key], meshes[key], observations[key] = case, mesh, observed
    expected = {("adiabatic_heater", n, dt) for n, dt in ((2, 2.0), (4, 1.0), (8, 0.5))}
    expected |= {
        ("cold_restart_robin", n, dt)
        for n, dt in ((2, 0.5), (4, 0.5), (8, 0.5), (8, 1.0), (8, 2.0))
    }
    if set(cases) != expected:
        raise ValueError("exact complete original thermal reference coverage required")
    keys = [("cold_restart_robin", 8, dt) for dt in (2.0, 1.0, 0.5)]
    convergence = temporal_self_convergence(
        [cases[k]["request"] for k in keys],
        [observations[k] for k in keys],
        [meshes[k] for k in keys],
    )
    spatial = [
        cases["cold_restart_robin", n, 0.5]["receipt"]["numerical_verification"][
            "temperature"
        ]["normalized_max_abs_error"]
        for n in (2, 4, 8)
    ]
    if (
        convergence != report["temporal_self_convergence"]
        or spatial != report["spatial_errors_n2_n4_n8"]
        or not all(a > b for a, b in pairwise(spatial))
    ):
        raise ValueError(
            "unchanged separate temporal and spatial refinement gates required"
        )
    return report_file, cases["adiabatic_heater", 4, 1.0]["request"], fem, thermal


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "mcp",
        "runtime",
        "authority",
        "native-reference",
        "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary, mcp, runtime = (
        immutable(p) for p in (args.executable, args.mcp, args.runtime)
    )
    repo = Path(__file__).resolve().parents[1]
    source, baseline, fem, thermal = prerequisites(
        runtime, args.native_reference.resolve(strict=True), repo
    )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact native study originals and all independent failures; preserve.\n"
    )
    results, refusals = [], []
    with WorkerCampaign(
        binary, mcp, runtime, args.authority, root, timeout=300
    ) as campaign:
        planned = [
            campaign.planned(
                "plan-thermal-reference", {**baseline, "max_step_s": step}, name
            )
            for name, step in (("baseline", 1.0), ("refined", 0.5))
        ]
        study = {
            "schema_version": 1,
            "name": "explicit_thermal_time_study",
            "provenance": "synthetic fixed geometry/history/material/observation and unchanged temperature/energy gates; explicit temporal refinement",
            "cases": [
                {
                    "name": name,
                    "plan": plan["plan"],
                    "approved_digest": plan["approval_digest"],
                }
                for name, plan in zip(("baseline", "refined"), planned, strict=True)
            ],
            "max_total_artifact_bytes": sum(
                p["plan"]["observation"]["max_artifact_bytes"] for p in planned
            ),
        }
        request = root / "study.json"
        request.write_text(json.dumps(study))
        prepared = campaign.command("study", "prepare", request)

        def submit(key):
            value = campaign.command(
                "--socket",
                campaign.endpoint,
                "study",
                "submit",
                request,
                "--idempotency-key",
                key,
            )["data"]
            campaign.owned.extend(
                c["job"]["unit"]
                for c in value["cases"]
                if c["job"]["unit"] not in campaign.owned
            )
            return value

        def counts():
            with sqlite3.connect(
                "file:" + str(campaign.state / "jobs.sqlite3") + "?mode=ro", uri=True
            ) as db:
                return [
                    db.execute("select count(*) from " + table).fetchone()[0]
                    for table in ("studies", "jobs", "artifacts")
                ]

        async def exercise():
            from mcp import Client
            from mcp.client.stdio import StdioServerParameters

            parameters = StdioServerParameters(
                command=str(mcp),
                args=["--profile", "all"],
                env={
                    **campaign.environment,
                    "HARBOR_CAD_SOCKET": str(campaign.endpoint),
                },
            )
            async with Client(parameters) as client:
                assert prepared == await mcp_call(client, "study_prepare", study=study)
                before = counts()
                changed = copy.deepcopy(study)
                changed["cases"][1]["approved_digest"] = "0" * 64
                denied = await client.call_tool(
                    "study_submit",
                    {"study": changed, "idempotency_key": "bad-approval"},
                )
                assert denied.is_error and counts() == before
                refusals.append("all-case approval refusal before intent or child jobs")
                accepted = submit("stable-study")
                jobs = [case["job"] for case in accepted["cases"]]
                running = campaign.wait(jobs[0], {"running"})
                active = retention_snapshot(campaign.state, jobs[0], str(binary))
                campaign.restart()
                repeat = await mcp_call(
                    client, "study_submit", study=study, idempotency_key="stable-study"
                )
                assert repeat["id"] == accepted["id"] and [
                    c["job"]["id"] for c in repeat["cases"]
                ] == [j["id"] for j in jobs]
                for index, job in enumerate(jobs):
                    outcome = campaign.wait(job, {"succeeded"})
                    if index == 0:
                        assert outcome["invocation_id"] == running["invocation_id"]
                    bundle = root / f"bundle-{index}"
                    campaign.command(
                        "artifact",
                        "export",
                        "--state",
                        campaign.state,
                        job["id"],
                        bundle,
                    )
                    verify_manifest(bundle)
                    work = bundle / "stages/thermal"
                    mesh = json.loads((work / "mesh.json").read_text())
                    nodes, cells = (
                        {int(k): v for k, v in mesh[name].items()}
                        for name in ("nodes", "elements")
                    )
                    checks, _, fields = thermal.verify(
                        planned[index]["plan"]["thermal"],
                        nodes,
                        cells,
                        fem.read_dat((work / "reference.dat").read_text()),
                    )
                    receipt = json.loads((work / "thermal-receipt.json").read_text())
                    assert checks == receipt["numerical_verification"]
                    assert json.loads((work / "thermal-fields.json").read_text())[
                        "times"
                    ] == json.loads(json.dumps(fields))
                    sandbox(receipt, "harbor-cad-thermal-cpu-v1")
                    historical = await mcp_call(
                        client, "qualification_report", job_id=job["id"]
                    )
                    assert (
                        historical["capabilities"][0]["numerical_verification"]
                        == "reported_pass"
                    )
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                    results.append(
                        {
                            "name": study["cases"][index]["name"],
                            "job": outcome,
                            "independent_checks": checks,
                            "historical": historical,
                            "resources": json.loads(
                                (bundle / "service-resources.json").read_text()
                            ),
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                            "released": True,
                        }
                    )
                complete = campaign.command(
                    "--socket", campaign.endpoint, "study", "status", accepted["id"]
                )["data"]
                assert complete == await mcp_call(
                    client, "study_status", study_id=accepted["id"]
                )
                assert (
                    complete["execution"] == "completed_successfully"
                    and complete["physical_validation"] == "unqualified"
                )
                comparison = {
                    "schema_version": 1,
                    "left": {
                        "schema_version": 1,
                        "job_id": jobs[0]["id"],
                        "field": "temperature",
                        "physical_time_s": 120.0,
                        "locations": [{"association": "node", "node_id": 1}],
                    },
                    "right": {
                        "schema_version": 1,
                        "job_id": jobs[1]["id"],
                        "field": "temperature",
                        "physical_time_s": 120.0,
                        "locations": [{"association": "node", "node_id": 1}],
                    },
                }
                comparison_file = root / "compare.json"
                comparison_file.write_text(json.dumps(comparison))
                compared = campaign.command(
                    "--socket",
                    campaign.endpoint,
                    "results",
                    "compare-thermal",
                    comparison_file,
                )["data"]
                assert compared == await mcp_call(
                    client, "results_compare_thermal", request_spec=comparison
                )
                # Admission and roots for a second collection are ordinary per-job
                # resources; cancellation never rewrites its original approvals.
                lifecycle = submit("owned-lifecycle-study")
                children = [case["job"] for case in lifecycle["cases"]]
                campaign.wait(children[0], {"running"})
                queued = campaign.command(
                    "--socket", campaign.endpoint, "job", "status", children[1]["id"]
                )["data"]
                assert queued["state"] == "queued"
                campaign.command(
                    "--socket", campaign.endpoint, "job", "cancel", children[1]["id"]
                )
                campaign.worker.kill()
                campaign.worker.wait(timeout=5)
                await asyncio.to_thread(
                    subprocess.run,
                    [
                        "systemctl",
                        "--user",
                        "kill",
                        "--signal=KILL",
                        "--kill-whom=all",
                        children[0]["unit"],
                    ],
                    env=campaign.environment,
                    check=True,
                    timeout=30,
                )
                campaign.start()
                campaign.wait(children[0], {"failed"})
                recovered = await mcp_call(
                    client,
                    "study_submit",
                    study=study,
                    idempotency_key="owned-lifecycle-study",
                )
                assert [c["job"]["id"] for c in recovered["cases"]] == [
                    j["id"] for j in children
                ]
                assert recovered["execution"] == "completed_with_failures"
                for job in children:
                    bundle = root / ("bundle-terminal-" + job["id"])
                    campaign.command(
                        "artifact",
                        "export",
                        "--state",
                        campaign.state,
                        job["id"],
                        bundle,
                    )
                    verify_manifest(bundle)
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                return {
                    "completed_study": complete,
                    "lifecycle_study": recovered,
                    "active_retention": active,
                    "comparison": compared,
                    "restart_kept_invocation": True,
                }

        observations = asyncio.run(exercise())
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "authority_sha256": checksum(args.authority),
        "native_reference": str(source),
        "native_reference_sha256": checksum(source),
        "campaign_sha256": checksum(Path(__file__)),
        "results": results,
        "refusals": refusals,
        **observations,
        "scope": "exact-package bounded synthetic CPU thermal study and per-job lifecycle; two steps retain the same scientific gates; no optimization winner or physical validation inferred",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
