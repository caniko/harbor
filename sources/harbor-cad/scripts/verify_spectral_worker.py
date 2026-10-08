"""Exact-package directional UV CLI/MCP originals, approval and lifecycle gate."""

import argparse
import asyncio
import copy
import csv
import importlib.util
import json
import math
import subprocess
from pathlib import Path

from native_worker_campaign import WorkerCampaign
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_retention_release,
)


async def mcp_call(client, operation, **inputs):
    reply = await client.call_tool(operation, inputs)
    if reply.is_error:
        raise RuntimeError(reply.content)
    return reply.structured_content


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
        p.resolve(strict=True) for p in (args.executable, args.mcp, args.runtime)
    )
    if any(
        not p.is_relative_to("/nix/store") or not p.is_file()
        for p in (binary, mcp, runtime)
    ):
        raise ValueError(
            "exact immutable CLI, MCP and directional spectral worker runtime required"
        )
    reference_path = args.native_reference.resolve(strict=True) / "verification.json"
    reference = json.loads(reference_path.read_text())
    if (
        reference["planner_qualification"] != "exact immutable package"
        or reference["native_qualification"]
        != "exact immutable standalone CPU reference; worker unqualified"
    ):
        raise ValueError(
            "complete exact native spectral prerequisite required; development replay cannot qualify the worker"
        )
    standalone = Path(reference["runtime"]).resolve(strict=True)
    native = json.loads(runtime.read_text())
    standalone_native = json.loads(standalone.read_text())
    if (
        not standalone.is_relative_to("/nix/store")
        or checksum(standalone) != reference["runtime_sha256"]
        or any(
            native[k] != standalone_native[k]
            for k in ("spectral", "spectral_closure", "bwrap")
        )
    ):
        raise ValueError(
            "worker and standalone must select the same exact native adapter, closure and sandbox launcher"
        )
    cases = {record["case"]: record for record in reference["results"]}
    required = {
        "normal",
        "inclined",
        "back",
        "blocked",
        "double-area",
        "isotropic-s4096",
        "isotropic-s16384",
        "isotropic-s65536",
        "reflection-rho04",
        "reflection-rho08",
        "reflection-black",
    }
    if (
        set(cases) != required
        or len(reference["results"]) != len(required)
        or len(reference["rejections"]) != 11
    ):
        raise ValueError(
            "complete unchanged native angular, spectral, reflection, refinement and pre-output rejection gates required"
        )
    for name, record in cases.items():
        for filename, identity in record["original_files_sha256"].items():
            if (
                Path(filename).name != filename
                or checksum(args.native_reference / name / filename) != identity
            ):
                raise ValueError("native prerequisite original bytes changed")
    repo = Path(__file__).resolve().parents[1]
    module = importlib.util.spec_from_file_location(
        "independent_worker_spectral_checks", repo / "adapters/spectral_reference.py"
    )
    verifier = importlib.util.module_from_spec(module)
    module.loader.exec_module(verifier)
    spec = cases["normal"]["receipt"]["input"]
    normalized = verifier.normalize(spec)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact native spectral worker originals and failed attempts; preserve.\n"
    )
    before = service_resources()
    results = []
    with WorkerCampaign(
        binary, mcp, runtime, args.authority, root, timeout=300
    ) as campaign:

        async def run():
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
                schemas = campaign.command("schema")
                for interface in ("cli", "mcp"):
                    request = root / f"request-{interface}.json"
                    request.write_text(json.dumps(spec, allow_nan=False))
                    planned = campaign.command(
                        "case", "plan-spectral-reference", request
                    )
                    if interface == "mcp":
                        assert (
                            await mcp_call(client, "spectral_reference_plan", spec=spec)
                            == planned
                        )
                    assert (
                        planned["plan"]["schema_version"] == 13
                        and planned["plan"]["observation"]["retained_times_s"] == []
                    )

                    async def submit(interface, planned):
                        if interface == "mcp":
                            return await mcp_call(
                                client,
                                "job_submit",
                                plan=planned["plan"],
                                approved_digest=planned["approval_digest"],
                                idempotency_key=interface,
                            )
                        return campaign.submit(planned, interface)

                    job = await submit(interface, planned)
                    if job["unit"] not in campaign.owned:
                        campaign.owned.append(job["unit"])
                    owner = campaign.wait(job, {"running"})
                    active = retention_snapshot(campaign.state, job, str(binary))
                    reservation = admission_record(campaign.state, job)
                    assert (
                        reservation is not None
                        and active["intent"]["binding"]["sandbox_policy"]
                        == "harbor-cad-spectral-cpu-v1"
                    )
                    # Source edits and protocol-worker death cannot alter the
                    # already approved per-job source, runner or native service.
                    request.write_text("{}")
                    campaign.restart()
                    assert (await submit(interface, planned))["id"] == job["id"]
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
                    records = verify_manifest(bundle)
                    receipt = json.loads(
                        (bundle / "stages/spectral/spectral-receipt.json").read_text()
                    )
                    assert receipt["input"] == spec and receipt[
                        "request_sha256"
                    ] == checksum(bundle / "native-spectral-request.json")
                    assert (
                        receipt["variant"] == "scalar_spectral"
                        and receipt["precision"] == "Float32"
                        and receipt["reduction_precision"] == "Float64"
                    )
                    assert (
                        receipt["executed"] is True
                        and receipt["software_fallback"] is False
                        and receipt["normalized"] == normalized
                    )
                    assert (
                        receipt["sandbox"]["policy"] == "harbor-cad-spectral-cpu-v1"
                        and len(receipt["sandbox"]["checks"]) == 8
                        and all(receipt["sandbox"]["checks"].values())
                    )
                    field_checks = []
                    for observation, standalone_observation in zip(
                        receipt["observations"],
                        cases["normal"]["receipt"]["observations"],
                        strict=True,
                    ):
                        path = bundle / "stages/spectral" / observation["path"]
                        assert (
                            checksum(path)
                            == observation["sha256"]
                            == standalone_observation["sha256"]
                            and path.stat().st_size == observation["bytes"]
                        )
                        with path.open() as handle:
                            rows = list(csv.DictReader(handle))
                        assert len(rows) == spec["samples"] and [
                            int(r["sample"]) for r in rows
                        ] == list(range(spec["samples"]))
                        means = [
                            math.fsum(
                                float(r[f"emitter_weight_w_m2_nm_{i}"])
                                * float(r["native_cosine"])
                                for r in rows
                            )
                            / len(rows)
                            for i in range(len(spec["wavelengths"]))
                        ]
                        channels = {
                            name: verifier.product_integral(
                                normalized["wavelengths_nm"], means, weights
                            )
                            for name, weights in normalized["weights"].items()
                        }
                        for channel, value in channels.items():
                            assert math.isclose(
                                value,
                                observation["native_channels_w_m2"][channel],
                                rel_tol=1e-12,
                            )
                            assert math.isclose(
                                value * normalized["history_integral_s"],
                                observation["exposure_j_m2"][channel],
                                rel_tol=1e-12,
                            )
                        assert verifier.verify_channels(
                            normalized, channels, spec["relative_tolerance"]
                        )["passed"]
                        field_checks.append(
                            {
                                "seed": observation["seed"],
                                "original_sha256": checksum(path),
                                "channels": channels,
                                "samples": len(rows),
                            }
                        )
                    historical = campaign.command(
                        "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                    )["data"]
                    assert historical == await mcp_call(
                        client, "qualification_report", job_id=job["id"]
                    )
                    capability = historical["capabilities"][0]
                    assert (
                        len(historical["capabilities"]) == 1
                        and capability["runtime_execution"] == "recorded"
                        and capability["numerical_verification"] == "reported_pass"
                    )
                    assert (
                        capability["requested_device"] is None
                        and historical["physical_validation"] == "unqualified"
                        and historical["current_runtime_qualification"]
                        == "not_assessed"
                    )
                    for relative in (
                        f"stages/spectral/directional-{spec['seeds'][0]}.csv",
                        "stages/spectral/spectral-receipt.json",
                    ):
                        original = campaign.state / "artifacts" / job["id"] / relative
                        raw = original.read_bytes()
                        try:
                            original.write_bytes(raw + b"changed")
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
                            original.write_bytes(raw)
                    assert (
                        historical
                        == campaign.command(
                            "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                        )["data"]
                    )
                    controls = json.loads((bundle / "service-owner.json").read_text())[
                        "kernel_resources"
                    ]
                    resources = json.loads(
                        (bundle / "service-resources.json").read_text()
                    )["kernel_resources"]
                    assert (
                        controls["controls"]["cpu.max"] == "200000 100000"
                        and controls["controls"]["pids.max"] == "128"
                    )
                    assert int(controls["controls"]["memory.max"]) == max(
                        s["ram_bytes"] for s in planned["plan"]["stages"]
                    )
                    assert (
                        controls["controls"]["memory.swap.max"] == "0"
                        and resources["aggregate_memory_peak_bytes"] > 0
                    )
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                    results.append(
                        {
                            "interface": interface,
                            "job": outcome,
                            "original_fields": field_checks,
                            "historical": historical,
                            "records": len(records),
                            "active_retention": active,
                            "reservation": reservation,
                            "restart": "same invocation",
                            "registered_mutation_rejected": True,
                            "resources": resources,
                            "released": True,
                        }
                    )

                # Strict plan/approval rejections must not create a durable job.
                good = planned
                for field, value in (("spectral", None), ("wetting", None)):
                    invalid = copy.deepcopy(good["plan"])
                    invalid[field] = value
                    rejected = await client.call_tool(
                        "job_submit",
                        {
                            "plan": invalid,
                            "approved_digest": good["approval_digest"],
                            "idempotency_key": f"reject-{field}",
                        },
                    )
                    assert rejected.is_error
                rejected = await client.call_tool(
                    "job_submit",
                    {
                        "plan": good["plan"],
                        "approved_digest": "0" * 64,
                        "idempotency_key": "reject-approval",
                    },
                )
                assert rejected.is_error
                results.append(
                    {
                        "strict_plan_and_approval_rejections": 3,
                        "schemas": sorted(k for k in schemas if "Spectral" in k),
                    }
                )

                for action in ("forced-death", "cancel"):
                    slow = copy.deepcopy(spec)
                    slow["samples"] = 65536
                    request = root / f"request-{action}.json"
                    request.write_text(json.dumps(slow))
                    planned = campaign.command(
                        "case", "plan-spectral-reference", request
                    )
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
                            "retry": "same terminal job",
                            "failed_bundle_manifest_sha256": checksum(
                                bundle / "manifest.json"
                            ),
                        }
                    )

        asyncio.run(run())
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "native_reference": str(reference_path),
        "native_reference_sha256": checksum(reference_path),
        "authority_sha256": checksum(args.authority),
        "results": results,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "synthetic exact-package directional UV/dose, original-knot reconstruction, registered mutation, CLI/MCP immutable jobs and owned lifecycle; no atmosphere or GPU transport qualification",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "checks": len(results),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
