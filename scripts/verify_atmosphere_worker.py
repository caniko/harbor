"""Exact atmospheric CLI/MCP originals, immutable job approvals and owned lifecycle."""

import argparse
import asyncio
import copy
import importlib.util
import json
import math
import subprocess
from pathlib import Path

from native_worker_campaign import WorkerCampaign
from verify_atmosphere_cpu import assess_refinements
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
            "exact immutable packaged atmospheric CLI, MCP and worker runtime required"
        )
    native = json.loads(runtime.read_text())
    reference_file = args.native_reference.resolve(strict=True) / "verification.json"
    reference = json.loads(reference_file.read_text())
    standalone = Path(reference["runtime"]).resolve(strict=True)
    if (
        not standalone.is_relative_to("/nix/store")
        or checksum(standalone) != reference["runtime_sha256"]
    ):
        raise ValueError(
            "unchanged exact packaged standalone atmosphere qualification required"
        )
    standalone_runtime = json.loads(standalone.read_text())
    if (
        native["atmosphere"] != standalone_runtime["atmosphere"]
        or native["atmosphere_closure"] != standalone_runtime["atmosphere_closure"]
        or reference["binary"] != str(binary)
        or reference["binary_sha256"] != checksum(binary)
    ):
        raise ValueError(
            "matching exact atmospheric standalone/worker planner, ABI, adapter and closure required"
        )
    required = {
        "transparent-z0-a0",
        "transparent-z30-a0",
        "transparent-z70-a90",
        "clear-streams16",
        "clear-streams32",
        "clear-streams64",
        "clear-angular16",
        "clear-angular32",
        "clear-rho06",
        "clear-wavelength4",
        "clear-wavelength2",
        "clear-wavelength1",
    }
    cases = {r["case"]: r for r in reference["results"]}
    if (
        set(cases) != required
        or len(reference["results"]) != 12
        or len(reference["rejections"]) != 8
        or set(reference["refinements"])
        != {"streams", "angular_shape", "wavelength_integrals"}
        or not all(
            r["passed"] and r["tolerance"] <= 0.02
            for r in reference["refinements"].values()
        )
        or not reference["runtime_qualification"].startswith(
            "exact immutable standalone"
        )
    ):
        raise ValueError(
            "complete unchanged standalone transparent, source/boundary and three separate refinement gates required"
        )
    for name, record in cases.items():
        for filename, identity in record["original_files_sha256"].items():
            if (
                Path(filename).name != filename
                or checksum(reference_file.parent / name / filename) != identity
            ):
                raise ValueError(
                    "native atmospheric prerequisite original bytes changed"
                )
    repo = Path(__file__).resolve().parents[1]
    module = importlib.util.spec_from_file_location(
        "independent_atmospheric_worker_originals",
        repo / "adapters/atmosphere_reference.py",
    )
    bridge = importlib.util.module_from_spec(module)
    module.loader.exec_module(bridge)
    if reference["source_sha256"] != bridge.SOURCE_SHA256 or reference[
        "adapter_source_sha256"
    ] != checksum(repo / "adapters/atmosphere_reference.py"):
        raise ValueError(
            "exact official atmospheric source and matching independent adapter identity required"
        )
    for name, record in cases.items():
        normalized = bridge.normalize(record["request"])
        decoded = bridge.parse_original(
            (reference_file.parent / name / "uvspec-original.txt").read_text(),
            record["request"],
            normalized,
        )
        if record["prepared"] != normalized or record["observations"] != {
            k: v for k, v in decoded.items() if k != "radiance_w_m2_sr_nm"
        }:
            raise ValueError(
                "standalone reference differs from independently reconstructed atmospheric originals"
            )
    refinements = assess_refinements(
        cases,
        reference_file.parent,
        cases["clear-streams64"]["request"]["relative_tolerance"],
    )
    if refinements != reference["refinements"] or not all(
        r["passed"] for r in refinements.values()
    ):
        raise ValueError(
            "standalone refinement flags differ from retained original atmospheric errors"
        )
    # The high-order narrow-band native case also keeps enough real work for
    # observing owned running services and full-tree cancellation/death tests.
    case = cases["clear-wavelength1"]
    spec = case["request"]
    normalized = bridge.normalize(spec)
    before = service_resources()
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Exact atmospheric worker original fields and failed attempts; preserve.\n"
    )
    results = []
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
                planned = None
                for interface in ("cli", "mcp"):
                    request = root / f"request-{interface}.json"
                    request.write_text(json.dumps(spec, allow_nan=False))
                    planned = campaign.command(
                        "case", "plan-atmospheric-reference", request
                    )
                    assert planned == await mcp_call(
                        client, "atmospheric_reference_plan", spec=spec
                    )
                    assert (
                        planned["plan"]["schema_version"] == 14
                        and planned["plan"]["observation"]["retained_times_s"] == []
                    )

                    async def submit(interface, planned):
                        if interface == "cli":
                            return campaign.submit(planned, interface)
                        return await mcp_call(
                            client,
                            "job_submit",
                            plan=planned["plan"],
                            approved_digest=planned["approval_digest"],
                            idempotency_key=interface,
                        )

                    job = await submit(interface, planned)
                    if job["unit"] not in campaign.owned:
                        campaign.owned.append(job["unit"])
                    running = campaign.wait(job, {"running"})
                    active = retention_snapshot(campaign.state, job, str(binary))
                    reservation = admission_record(campaign.state, job)
                    assert (
                        reservation is not None
                        and active["intent"]["binding"]["sandbox_policy"]
                        == bridge.POLICY
                    )
                    request.write_text("{}")
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
                    manifests = verify_manifest(bundle)
                    receipt = json.loads(
                        (
                            bundle / "stages/atmosphere/atmosphere-receipt.json"
                        ).read_text()
                    )
                    original = bundle / "stages/atmosphere/uvspec-original.txt"
                    assert receipt["input"] == spec and receipt[
                        "request_sha256"
                    ] == checksum(bundle / "native-atmosphere-request.json")
                    assert (
                        receipt["prepared"] == normalized
                        and receipt["source_sha256"] == bridge.SOURCE_SHA256
                        and receipt["profile_sha256"]
                        == bridge.PROFILE_SHA256[spec["profile"]]
                    )
                    assert (
                        receipt["executed"] is True
                        and receipt["software_fallback"] is False
                        and receipt["precision"] == "Float32"
                        and receipt["reduction_precision"] == "Float64"
                    )
                    assert (
                        receipt["sandbox"]["policy"] == bridge.POLICY
                        and len(receipt["sandbox"]["checks"]) == 8
                        and all(receipt["sandbox"]["checks"].values())
                    )
                    assert (
                        checksum(original)
                        == receipt["original"]["sha256"]
                        == case["original_files_sha256"]["uvspec-original.txt"]
                    )
                    decoded = bridge.parse_original(
                        original.read_text(), spec, normalized
                    )
                    assert (
                        {k: v for k, v in decoded.items() if k != "radiance_w_m2_sr_nm"}
                        == receipt["observations"]
                        == case["observations"]
                    )
                    field = [
                        m
                        for m in manifests
                        if m["path"] == "stages/atmosphere/uvspec-original.txt"
                    ]
                    assert (
                        len(field) == 1
                        and field[0]["association"]
                        == "wavelength_propagation_solid_angle_sample"
                        and field[0]["time_s"] is None
                    )
                    assert (
                        field[0]["units"]
                        == "lambda:nm,edir:W/(m2*nm),edn:W/(m2*nm),eup:W/(m2*nm),uu:W/(m2*sr*nm)"
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
                        "not_assessed" in capability["convergence"]
                        and historical["current_runtime_qualification"]
                        == "not_assessed"
                        and historical["physical_validation"] == "unqualified"
                    )
                    receiver = json.loads(
                        (repo / "examples/spectral-reference.json").read_text()
                    )
                    receiver.update(
                        wavelengths=spec["wavelengths"],
                        absorptivity=[0.5] * len(spec["wavelengths"]),
                        ageing_action=[0.25] * len(spec["wavelengths"]),
                    )
                    receiver["source"] = {
                        "kind": "directional",
                        "propagation_direction": normalized["propagation_direction"],
                        "irradiance": spec["toa_irradiance"],
                    }
                    receiver["source_provenance"] = (
                        "unchanged prescribed original native atmospheric TOA source; ground attenuation/radiance only from registered originals"
                    )
                    transfers = []
                    for normal in ([0, 0, 1], [0, 0, -1], [0, 1, 0], [0, -1, 0]):
                        receiver["sensor_normal"] = normal
                        transfer_request = {
                            "schema_version": 1,
                            "source_job": job["id"],
                            "receiver": receiver,
                            "angular_mapping": "native_midpoint_solid_angle_quadrature",
                            "maximum_relative_conservation_error": 1e-10,
                        }
                        transfer_file = (
                            root / f"transfer-{interface}-{len(transfers)}.json"
                        )
                        transfer_file.write_text(json.dumps(transfer_request))
                        transfer = campaign.command(
                            "--socket",
                            campaign.endpoint,
                            "results",
                            "transfer-atmosphere",
                            transfer_file,
                        )["data"]
                        assert transfer == await mcp_call(
                            client,
                            "results_transfer_atmosphere",
                            request_spec=transfer_request,
                        )
                        assert (
                            transfer["executed"] is False
                            and transfer["native_transport"] == "not_executed"
                            and transfer["preserved_original_distribution"] is True
                        )
                        assert (
                            transfer["original_field"]["sha256"] == checksum(original)
                            and transfer["transfer_relative_conservation_error"]
                            <= 1e-10
                        )
                        towards_direct = max(
                            0.0,
                            -math.fsum(
                                a * b
                                for a, b in zip(
                                    normal,
                                    normalized["propagation_direction"],
                                    strict=True,
                                )
                            ),
                        )
                        expected = []
                        for direct, rad in zip(
                            decoded["direct_normal_w_m2_nm"],
                            decoded["radiance_w_m2_sr_nm"],
                            strict=True,
                        ):
                            diffuse = []
                            for i, mu in enumerate(normalized["umu"]):
                                for j, phi in enumerate(normalized["phi_deg"]):
                                    direction = [
                                        math.sqrt(1 - mu * mu)
                                        * math.sin(math.radians(phi)),
                                        math.sqrt(1 - mu * mu)
                                        * math.cos(math.radians(phi)),
                                        mu,
                                    ]
                                    diffuse.append(
                                        rad[i * len(normalized["phi_deg"]) + j]
                                        * normalized["angular_cell_solid_angle_sr"]
                                        * max(
                                            0.0,
                                            -math.fsum(
                                                a * b
                                                for a, b in zip(
                                                    normal, direction, strict=True
                                                )
                                            ),
                                        )
                                    )
                            expected.append(
                                direct * towards_direct + math.fsum(diffuse)
                            )
                        assert all(
                            math.isclose(a, b, rel_tol=1e-11, abs_tol=1e-14)
                            for a, b in zip(
                                transfer["reference"]["incident_w_m2_nm"],
                                expected,
                                strict=True,
                            )
                        )
                        assert (
                            transfer["reference"]["absorbed_irradiance_w_m2"]
                            == 0.5 * transfer["reference"]["incident_irradiance_w_m2"]
                        )
                        assert (
                            transfer["reference"]["ageing_weighted_irradiance_w_m2"]
                            == 0.25 * transfer["reference"]["incident_irradiance_w_m2"]
                        )
                        transfers.append(transfer)
                    controls = json.loads((bundle / "service-owner.json").read_text())[
                        "kernel_resources"
                    ]
                    assert (
                        controls["MemoryMax"]
                        == str(planned["plan"]["stages"][0]["ram_bytes"])
                        and controls["MemorySwapMax"] == "0"
                        and controls["TasksMax"] == "128"
                        and controls["KillMode"] == "control-group"
                        and controls["NoNewPrivileges"] == "yes"
                    )
                    for relative in (
                        "stages/atmosphere/uvspec-original.txt",
                        "stages/atmosphere/atmosphere-receipt.json",
                    ):
                        retained = campaign.state / "artifacts" / job["id"] / relative
                        raw = retained.read_bytes()
                        try:
                            retained.write_bytes(raw + b"changed")
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
                            rejected = await client.call_tool(
                                "results_transfer_atmosphere",
                                {"request_spec": transfer_request},
                            )
                            assert rejected.is_error
                        finally:
                            retained.write_bytes(raw)
                    assert (
                        historical
                        == campaign.command(
                            "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                        )["data"]
                    )
                    wait_admission_release(campaign.state, job)
                    wait_retention_release(campaign.state, job)
                    results.append(
                        {
                            "interface": interface,
                            "job": outcome,
                            "historical": historical,
                            "active_retention": active,
                            "active_reservation": reservation,
                            "released": True,
                            "manifest_sha256": checksum(bundle / "manifest.json"),
                            "original_sha256": checksum(original),
                            "controls": controls,
                            "anisotropic_original_transfers": transfers,
                        }
                    )
                for key, value in (
                    ("atmosphere", None),
                    ("spectral", None),
                    ("schema_version", 13),
                ):
                    invalid = copy.deepcopy(planned["plan"])
                    invalid[key] = value
                    rejected = await client.call_tool(
                        "job_submit",
                        {
                            "plan": invalid,
                            "approved_digest": planned["approval_digest"],
                            "idempotency_key": f"reject-{key}",
                        },
                    )
                    assert rejected.is_error
                rejected = await client.call_tool(
                    "job_submit",
                    {
                        "plan": planned["plan"],
                        "approved_digest": "0" * 64,
                        "idempotency_key": "reject-approval",
                    },
                )
                assert rejected.is_error
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
                            "retry": "same terminal job",
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
        "native_reference": str(reference_file),
        "native_reference_sha256": checksum(reference_file),
        "authority_sha256": checksum(args.authority),
        "results": results,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "scope": "exact immutable synthetic CPU atmosphere CLI/MCP originals, source/energy/isolation, registered mutation and complete owned lifecycle; no spectral transport or physical qualification",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "results": len(results),
                "strict_plan_and_approval_rejections": 4,
                "report_sha256": checksum(root / "verification.json"),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
