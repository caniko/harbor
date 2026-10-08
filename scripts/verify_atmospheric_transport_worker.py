"""Exact registered atmospheric-source CPU transport, originals and owned lifecycle."""

import argparse
import asyncio
import copy
import json
import math
import subprocess
from pathlib import Path

from atmospheric_transport_prerequisites import immutable, prerequisites
from native_worker_campaign import WorkerCampaign
from verify_atmospheric_spectral_cpu import verify_observation
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum
from verify_spectral_worker import mcp_call
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_retention_release,
)


def tree_identity(root):
    return {
        str(path.relative_to(root)): checksum(path)
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "mcp",
        "source-runtime",
        "runtime",
        "authority",
        "source-reference",
        "native-reference",
        "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary, mcp, source_runtime, runtime = (
        immutable(path)
        for path in (args.executable, args.mcp, args.source_runtime, args.runtime)
    )
    source_case, verifier, prerequisite = prerequisites(
        binary, source_runtime, runtime, args.source_reference, args.native_reference
    )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact registered-source atmospheric transport originals and failed attempts; preserve.\n"
    )
    before = service_resources()
    results, rejections = [], []
    with WorkerCampaign(
        binary, mcp, source_runtime, args.authority, root, timeout=600
    ) as campaign:
        source_plan = campaign.planned(
            "plan-atmospheric-reference", source_case["request"], "source"
        )
        source_job = campaign.submit(source_plan, "source")
        campaign.wait(source_job, {"succeeded"})
        source_bundle = root / "bundle-source"
        campaign.command(
            "artifact",
            "export",
            "--state",
            campaign.state,
            source_job["id"],
            source_bundle,
        )
        verify_manifest(source_bundle)
        source_artifacts = campaign.state / "artifacts" / source_job["id"]
        original = source_artifacts / "stages/atmosphere/uvspec-original.txt"
        if (
            checksum(original)
            != source_case["original_files_sha256"]["uvspec-original.txt"]
        ):
            raise ValueError(
                "new registered atmospheric source must reproduce the exact native prerequisite"
            )
        source_history = campaign.command(
            "--socket", campaign.endpoint, "qualify", "--job", source_job["id"]
        )["data"]
        if not all(
            v["runtime_execution"] == "recorded"
            and v["numerical_verification"] == "reported_pass"
            for v in source_history["capabilities"]
        ):
            raise ValueError(
                "succeeded registered atmospheric source runtime/numerical evidence required"
            )
        wait_admission_release(campaign.state, source_job)
        wait_retention_release(campaign.state, source_job)
        source_identity = tree_identity(source_artifacts)
        (root / "source-profile.json").write_bytes(campaign.profile.read_bytes())
        profile = json.loads(campaign.profile.read_text())
        profile["native_runtime"] = str(runtime)
        campaign.profile.write_text(json.dumps(profile))
        campaign.restart()

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
                receiver = json.loads(
                    (
                        Path(__file__).resolve().parents[1]
                        / "examples/spectral-reference.json"
                    ).read_text()
                )
                atmosphere = source_case["request"]
                receiver.update(
                    wavelengths=atmosphere["wavelengths"],
                    absorptivity=[0.5] * len(atmosphere["wavelengths"]),
                    ageing_action=[0.25] * len(atmosphere["wavelengths"]),
                    samples=16384,
                )
                receiver["source"] = {
                    "kind": "directional",
                    "propagation_direction": source_case["prepared"][
                        "propagation_direction"
                    ],
                    "irradiance": atmosphere["toa_irradiance"],
                }
                receiver["source_provenance"] = (
                    "unchanged original registered native atmospheric TOA inputs; all ground radiance comes from retained fields"
                )
                request = {
                    "schema_version": 1,
                    "source_job": source_job["id"],
                    "receiver": receiver,
                    "angular_mapping": "native_midpoint_solid_angle_quadrature",
                    "maximum_relative_conservation_error": 1e-10,
                }

                def plan(value, key):
                    path = root / f"request-{key}.json"
                    path.write_text(json.dumps(value, allow_nan=False))
                    return campaign.command(
                        "--socket",
                        campaign.endpoint,
                        "results",
                        "plan-atmospheric-transport",
                        path,
                    )["data"]

                async def submit(interface, planned, key):
                    if interface == "cli":
                        return campaign.submit(planned, key)
                    job = await mcp_call(
                        client,
                        "job_submit",
                        plan=planned["plan"],
                        approved_digest=planned["approval_digest"],
                        idempotency_key=key,
                    )
                    if job["unit"] not in campaign.owned:
                        campaign.owned.append(job["unit"])
                    return job

                for interface, normal in (
                    ("cli", [0.0, 0.0, 1.0]),
                    ("mcp", [math.sqrt(0.5), 0.0, math.sqrt(0.5)]),
                ):
                    request["receiver"]["sensor_normal"] = normal
                    planned = plan(request, interface)
                    assert planned == await mcp_call(
                        client, "atmospheric_transport_plan", request_spec=request
                    )
                    assert (
                        planned["plan"]["schema_version"] == 15
                        and planned["plan"]["observation"]["retained_times_s"] == []
                    )
                    bound = planned["plan"]["atmospheric_transport"]
                    assert bound["source"]["original"]["sha256"] == checksum(original)
                    raw = original.read_bytes()
                    try:
                        original.write_bytes(raw + b"changed")
                        assert not campaign.submit(
                            planned, "reject-source-" + interface, allow_error=True
                        )["ok"]
                        rejected = await client.call_tool(
                            "atmospheric_transport_plan", {"request_spec": request}
                        )
                        assert rejected.is_error
                        rejections.append(
                            {"source_mutation_before_ack": interface, "rejected": True}
                        )
                    finally:
                        original.write_bytes(raw)
                    job = await submit(interface, planned, interface)
                    owner = campaign.wait(job, {"running"})
                    active = retention_snapshot(campaign.state, job, str(binary))
                    reservation = admission_record(campaign.state, job)
                    assert (
                        reservation is not None
                        and active["intent"]["binding"]["sandbox_policy"]
                        == "harbor-cad-atmospheric-spectral-cpu-v1"
                    )
                    retained = (
                        campaign.state
                        / "artifacts"
                        / job["id"]
                        / "source-atmosphere-original.txt"
                    )
                    assert (
                        retained.read_bytes() == raw
                        and retained.stat().st_ino != original.stat().st_ino
                    )
                    (root / f"request-{interface}.json").write_text("{}")
                    campaign.restart()
                    assert (await submit(interface, planned, interface))["id"] == job[
                        "id"
                    ]
                    outcome = campaign.wait(job, {"succeeded"})
                    assert outcome["invocation_id"] == owner["invocation_id"]
                    bundle = root / f"bundle-{interface}"
                    campaign.command(
                        "artifact",
                        "export",
                        "--state",
                        campaign.state,
                        job["id"],
                        bundle,
                    )
                    manifests = verify_manifest(bundle)
                    work = bundle / "stages/atmospheric-transport"
                    receipt = json.loads(
                        (work / "atmospheric-spectral-receipt.json").read_text()
                    )
                    assert receipt["request_sha256"] == checksum(
                        bundle / "native-atmospheric-spectral-request.json"
                    )
                    assert (
                        receipt["original_atmosphere_sha256"]
                        == checksum(bundle / "source-atmosphere-original.txt")
                        == checksum(original)
                    )
                    assert (
                        receipt["sandbox"]["policy"]
                        == "harbor-cad-atmospheric-spectral-cpu-v1"
                        and len(receipt["sandbox"]["checks"]) == 9
                        and all(
                            v is True for v in receipt["sandbox"]["checks"].values()
                        )
                    )
                    normalized = verifier.normalize_source(
                        atmosphere, request["receiver"], original.read_text()
                    )
                    assert [v["seed"] for v in receipt["observations"]] == request[
                        "receiver"
                    ]["seeds"]
                    reconstructed = [
                        verify_observation(
                            work, observation, request["receiver"], normalized, verifier
                        )
                        for observation in receipt["observations"]
                    ]
                    fields = [m for m in manifests if m["path"].endswith(".csv")]
                    assert len(fields) == 6 and all(
                        m["time_s"] is None
                        and m["association"] == "native_surface_emitter_spectral_packet"
                        and m["units"]
                        == "position:m,propagation:1,cosine:1,pdf:1,weight:W/(m2*nm)"
                        for m in fields
                    )
                    historical = campaign.command(
                        "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                    )["data"]
                    assert historical == await mcp_call(
                        client, "qualification_report", job_id=job["id"]
                    )
                    assert (
                        len(historical["capabilities"]) == 1
                        and historical["capabilities"][0]["runtime_execution"]
                        == "recorded"
                        and historical["capabilities"][0]["numerical_verification"]
                        == "reported_pass"
                    )
                    assert (
                        historical["physical_validation"] == "unqualified"
                        and historical["current_runtime_qualification"]
                        == "not_assessed"
                    )
                    for path in (
                        retained,
                        campaign.state
                        / "artifacts"
                        / job["id"]
                        / "source-atmosphere-receipt.json",
                        campaign.state
                        / "artifacts"
                        / job["id"]
                        / f"stages/atmospheric-transport/direct-{receiver['seeds'][0]}.csv",
                        original,
                    ):
                        saved = path.read_bytes()
                        try:
                            path.write_bytes(saved + b"changed")
                            assert not campaign.command(
                                "--socket",
                                campaign.endpoint,
                                "qualify",
                                "--job",
                                job["id"],
                                allow_error=True,
                            )["ok"]
                            rejected = await client.call_tool(
                                "qualification_report", {"job_id": job["id"]}
                            )
                            assert rejected.is_error
                        finally:
                            path.write_bytes(saved)
                    controls = json.loads((bundle / "service-owner.json").read_text())[
                        "kernel_resources"
                    ]
                    resources = json.loads(
                        (bundle / "service-resources.json").read_text()
                    )["kernel_resources"]
                    assert (
                        controls["controls"]["cpu.max"] == "200000 100000"
                        and controls["controls"]["pids.max"] == "128"
                        and controls["controls"]["memory.swap.max"] == "0"
                    )
                    assert (
                        int(controls["controls"]["memory.max"])
                        == planned["plan"]["stages"][0]["ram_bytes"]
                        and resources["aggregate_memory_peak_bytes"] > 0
                    )
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                    assert tree_identity(source_artifacts) == source_identity
                    results.append(
                        {
                            "interface": interface,
                            "job": outcome,
                            "source": bound["source"],
                            "historical": historical,
                            "original_reconstruction": reconstructed,
                            "active_retention": active,
                            "reservation": reservation,
                            "resources": resources,
                            "released": True,
                            "same_invocation_after_restart": True,
                            "source_unchanged": True,
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                        }
                    )

                for field, value in (
                    ("atmosphere", None),
                    ("spectral", None),
                    ("schema_version", 14),
                ):
                    bad = copy.deepcopy(planned["plan"])
                    bad[field] = value
                    rejected = await client.call_tool(
                        "job_submit",
                        {
                            "plan": bad,
                            "approved_digest": planned["approval_digest"],
                            "idempotency_key": "reject-" + field,
                        },
                    )
                    assert rejected.is_error
                    rejections.append({"plan_mutation": field, "rejected": True})
                assert not campaign.submit(
                    planned, "reject-approval", approval="0" * 64, allow_error=True
                )["ok"]
                rejections.append({"approval_mutation": True, "rejected": True})
                request["receiver"]["samples"] = 65536
                slow = plan(request, "lifecycle")
                for action in ("forced-death", "cancel"):
                    job = campaign.submit(slow, action)
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
                    assert campaign.submit(slow, action)["id"] == job["id"]
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
                            "same_terminal_job_after_retry": True,
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                        }
                    )

        asyncio.run(exercise())
        assert tree_identity(source_artifacts) == source_identity
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "source_runtime": str(source_runtime),
        "source_runtime_sha256": checksum(source_runtime),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "authority_sha256": checksum(args.authority),
        "prerequisites": prerequisite,
        "source_job": source_job,
        "source_originals_unchanged": True,
        "results": results,
        "rejections": rejections,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "exact original registered libRadtran source to CPU Mitsuba direct/diffuse transport, CLI/MCP approvals, original packets and owned lifecycle; source refinement, transport convergence and physical validation remain separate",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
                "rejections": len(rejections),
            }
        )
    )


if __name__ == "__main__":
    main()
