"""Exact prescribed-snow CLI/MCP thermal fields, provenance and owned lifecycle."""

import argparse
import asyncio
import copy
import json
import math
import subprocess
from itertools import pairwise
from pathlib import Path

from atmospheric_transport_prerequisites import immutable, original_files, sandbox
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
        immutable(path) for path in (args.executable, args.mcp, args.runtime)
    )
    repo = Path(__file__).resolve().parents[1]
    native = json.loads(runtime.read_text())
    source = args.native_reference.resolve(strict=True)
    reference_file = source / "verification.json"
    reference = json.loads(reference_file.read_text())
    standalone = immutable(reference["runtime"])
    descriptor = json.loads(standalone.read_text())
    if (
        reference["planner_qualification"] != "exact immutable package"
        or reference["planner"] != str(binary)
        or reference["planner_sha256"] != checksum(binary)
        or reference["runtime_sha256"] != checksum(standalone)
        or descriptor["thermal"] != native["thermal"]
        or descriptor["thermal_closure"] != native["thermal_closure"]
        or reference["adapter"] != native["thermal"]
        or reference["source_verifier_sha256"]
        != checksum(repo / "adapters/thermal_history.py")
        or len(reference["results"]) != 6
        or len(reference["rejections"]) != 8
        or not reference["temporal_self_convergence"]["passed"]
    ):
        raise ValueError(
            "complete matching exact-package snow native and separate refinement prerequisites required"
        )
    fem = load("snow_worker_fem", repo / "adapters/fem_reference.py")
    thermal = load("snow_worker_thermal", repo / "adapters/thermal_history.py")
    cases = {case["case"]: case for case in reference["results"]}
    if set(cases) != {
        "snow-n2-dt40",
        "snow-n4-dt40",
        "snow-n8-dt40",
        "snow-n8-dt80",
        "snow-n8-dt20",
        "bare-n8-dt20",
    }:
        raise ValueError(
            "complete snow/bare spatial and temporal original case coverage required"
        )
    meshes, observations = {}, {}
    for case in reference["results"]:
        original_files(
            source,
            case,
            {
                "mesh.json",
                "reference.dat",
                "thermal-fields.json",
                "thermal-receipt.json",
            },
        )
        name = case["case"]
        work = source / name
        mesh = json.loads((work / "mesh.json").read_text())
        nodes, cells = (
            {int(k): v for k, v in mesh[field].items()}
            for field in ("nodes", "elements")
        )
        checks, _, fields = thermal.verify(
            case["request"],
            nodes,
            cells,
            fem.read_dat((work / "reference.dat").read_text()),
        )
        if checks != case["independent_checks"] or case["exit_code"] != 0:
            raise ValueError(
                "independent unchanged source thermal numerical checks required"
            )
        observations[name] = json.loads((work / "thermal-fields.json").read_text())
        if observations[name]["times"] != json.loads(json.dumps(fields)):
            raise ValueError("complete original native thermal observations required")
        meshes[name] = mesh
    spatial = [
        cases[f"snow-n{n}-dt40"]["independent_checks"]["temperature"][
            "normalized_max_abs_error"
        ]
        for n in (2, 4, 8)
    ]
    names = [f"snow-n8-dt{dt}" for dt in (80, 40, 20)]
    temporal = temporal_self_convergence(
        [cases[name]["request"] for name in names],
        [observations[name] for name in names],
        [meshes[name] for name in names],
    )
    if (
        spatial != reference["spatial_errors_n2_n4_n8"]
        or any(not math.isfinite(v) or not 0 <= v <= 0.02 for v in spatial)
        or not all(a > b for a, b in pairwise(spatial))
        or temporal != reference["temporal_self_convergence"]
    ):
        raise ValueError(
            "separate unchanged original snow spatial/temporal refinement gates required"
        )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact prescribed-snow original worker fields; preserve every failed attempt.\n"
    )
    results, rejections = [], []
    with WorkerCampaign(
        binary, mcp, runtime, args.authority, root, timeout=300
    ) as campaign:

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
                for interface, label in (
                    ("cli", "snow-n8-dt40"),
                    ("mcp", "snow-n8-dt20"),
                ):
                    case = cases[label]
                    spec = case["plan"]["snow_boundary"]["input"]
                    planned = campaign.planned("plan-snow-reference", spec, interface)
                    assert planned == await mcp_call(
                        client, "case_plan_snow_reference", spec=spec
                    )
                    assert (
                        planned["plan"]["schema_version"] == 6
                        and planned["plan"]["thermal"]
                        == planned["snow_boundary"]["native"]
                        == case["request"]
                    )
                    snow = planned["snow_boundary"]
                    expected_h = 1 / (
                        1 / spec["thermal"]["convection_w_m2_k"]
                        + snow["thickness_m"] / spec["snow"]["conductivity"]["value"]
                    )
                    assert (
                        abs(snow["effective_convection_w_m2_k"] / expected_h - 1)
                        < 1e-14
                    )
                    changed = copy.deepcopy(spec)
                    changed["snow"]["thickness"]["value"] *= 1.5
                    other = campaign.planned(
                        "plan-snow-reference", changed, "changed-" + interface
                    )
                    assert other["approval_digest"] != planned["approval_digest"]
                    assert not campaign.submit(
                        other,
                        "reject-prescription-" + interface,
                        approval=planned["approval_digest"],
                        allow_error=True,
                    )["ok"]
                    rejected = await client.call_tool(
                        "job_submit",
                        {
                            "plan": other["plan"],
                            "approved_digest": planned["approval_digest"],
                            "idempotency_key": "reject-mcp-" + interface,
                        },
                    )
                    assert rejected.is_error
                    rejections.append(
                        {
                            "interface": interface,
                            "changed_original_prescription_rejected": True,
                        }
                    )

                    async def submit(interface, planned):
                        if interface == "cli":
                            return campaign.submit(planned, interface)
                        job = await mcp_call(
                            client,
                            "job_submit",
                            plan=planned["plan"],
                            approved_digest=planned["approval_digest"],
                            idempotency_key=interface,
                        )
                        if job["unit"] not in campaign.owned:
                            campaign.owned.append(job["unit"])
                        return job

                    job = await submit(interface, planned)
                    running = campaign.wait(job, {"running"})
                    active = retention_snapshot(campaign.state, job, str(binary))
                    assert (
                        active["intent"]["binding"]["sandbox_policy"]
                        == "harbor-cad-thermal-cpu-v1"
                    )
                    (root / f"spec-{interface}.json").write_text("{}")
                    campaign.restart()
                    assert (await submit(interface, planned))["id"] == job["id"]
                    outcome = campaign.wait(job, {"succeeded"})
                    assert outcome["invocation_id"] == running["invocation_id"]
                    bundle = root / f"bundle-{interface}"
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
                        planned["plan"]["thermal"],
                        nodes,
                        cells,
                        fem.read_dat((work / "reference.dat").read_text()),
                    )
                    receipt = json.loads((work / "thermal-receipt.json").read_text())
                    assert (
                        checks
                        == receipt["numerical_verification"]
                        == case["independent_checks"]
                    )
                    assert json.loads((work / "thermal-fields.json").read_text())[
                        "times"
                    ] == json.loads(json.dumps(fields))
                    assert receipt["request_sha256"] == checksum(
                        bundle / "native-thermal-request.json"
                    )
                    assert (
                        checksum(work / "reference.dat")
                        == case["original_files_sha256"]["reference.dat"]
                    )
                    sandbox(receipt, "harbor-cad-thermal-cpu-v1")
                    assert (
                        planned["plan"]["thermal"]["moisture_risk"]["assessment"]
                        == "inapplicable"
                    )
                    historical = campaign.command(
                        "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                    )["data"]
                    assert historical == await mcp_call(
                        client, "qualification_report", job_id=job["id"]
                    )
                    assert (
                        historical["physical_validation"] == "unqualified"
                        and historical["current_runtime_qualification"]
                        == "not_assessed"
                    )
                    assert len(historical["capabilities"]) == 1 and all(
                        cap["runtime_execution"] == "recorded"
                        and cap["numerical_verification"] == "reported_pass"
                        for cap in historical["capabilities"]
                    )
                    artifact = (
                        campaign.state
                        / "artifacts"
                        / job["id"]
                        / "stages/thermal/reference.dat"
                    )
                    saved = artifact.read_bytes()
                    try:
                        artifact.write_bytes(saved + b"changed")
                        assert not campaign.command(
                            "--socket",
                            campaign.endpoint,
                            "qualify",
                            "--job",
                            job["id"],
                            allow_error=True,
                        )["ok"]
                    finally:
                        artifact.write_bytes(saved)
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                    results.append(
                        {
                            "interface": interface,
                            "job": outcome,
                            "snow_boundary": snow,
                            "historical": historical,
                            "independent_checks": checks,
                            "active_retention": active,
                            "same_invocation_after_restart": True,
                            "released": True,
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                        }
                    )

                for action in ("forced-death", "cancel"):
                    job = campaign.submit(planned, action)
                    campaign.wait(job, {"running"})
                    active = retention_snapshot(campaign.state, job, str(binary))
                    if action == "forced-death":
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
                                job["unit"],
                            ],
                            check=True,
                            timeout=30,
                            env=campaign.environment,
                        )
                        campaign.start()
                        outcome = campaign.wait(job, {"failed"})
                    else:
                        campaign.command(
                            "--socket", campaign.endpoint, "job", "cancel", job["id"]
                        )
                        outcome = campaign.wait(job, {"cancelled"})
                    assert campaign.submit(planned, action)["id"] == job["id"]
                    bundle = root / f"bundle-{action}"
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
                    results.append(
                        {
                            "action": action,
                            "job": outcome,
                            "active_retention": active,
                            "released": True,
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                        }
                    )

        asyncio.run(exercise())
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "authority_sha256": checksum(args.authority),
        "native_reference": str(reference_file),
        "native_reference_sha256": checksum(reference_file),
        "results": results,
        "rejections": rejections,
        "physical_validation": "unqualified",
        "scope": "prescribed dry full-face quasi-steady snow resistance, original transient thermal fields and owned CLI/MCP worker lifecycle; blocked-opening flow, deposition, melt and physical validation remain separate",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
            }
        )
    )


if __name__ == "__main__":
    main()
