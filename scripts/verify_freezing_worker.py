"""Exact freezing CLI/MCP native originals, immutable jobs and lifecycle gate."""

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from native_worker_campaign import WorkerCampaign
from verify_contact_cpu import load
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_retention_release,
)
from verify_wetting_cpu import checksum


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "runtime",
        "mcp",
        "authority",
        "native-reference",
        "output",
    ):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args = parser.parse_args()
    binary, runtime, mcp = (
        path.resolve(strict=True) for path in (args.executable, args.runtime, args.mcp)
    )
    if any(
        not path.is_relative_to("/nix/store") or not path.is_file()
        for path in (binary, runtime, mcp)
    ):
        raise ValueError(
            "exact immutable packaged CLI, MCP and freezing worker runtime required"
        )
    native = json.loads(runtime.read_text())
    reference_file = args.native_reference / "verification.json"
    reference = json.loads(reference_file.read_text())
    standalone = Path(reference["runtime"]).resolve(strict=True)
    standalone_runtime = json.loads(standalone.read_text())
    if (
        checksum(standalone) != reference["runtime_sha256"]
        or native["freezing"] != standalone_runtime["freezing"]
        or native["freezing_closure"] != standalone_runtime["freezing_closure"]
        or len(reference["results"]) != 6
        or len(reference["rejections"]) != 15
        or reference["refinement"]["passed"] is not True
    ):
        raise ValueError(
            "exact complete native Stefan/temperature/mass/energy/refinement qualification required"
        )
    if any(
        native.get(key) is not None
        for key in (
            "cad",
            "openlb",
            "render",
            "video",
            "filter",
            "fem",
            "thermal",
            "contact",
            "wetting",
            "cad_mesh",
            "fem_imported",
        )
    ):
        raise ValueError("operation-specific CPU freezing runtime required")
    repo = Path(__file__).resolve().parents[1]
    bridge = load("freezing_worker_verifier", repo / "adapters/freezing_reference.py")
    fem = load("freezing_worker_common", repo / "adapters/fem_reference.py")
    for result in reference["results"]:
        name = f"stefan{result['stefan_number']}-n{result['resolution']}"
        receipt = json.loads(
            (args.native_reference / name / "freezing-receipt.json").read_text()
        )
        checks, _, hashes = bridge.verify(
            result["request"], receipt, args.native_reference / name, fem
        )
        if (
            checks != result["independent_checks"]
            or hashes != result["original_files_sha256"]
        ):
            raise ValueError(
                "intact original native qualification prerequisite required"
            )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    before = service_resources()
    results = []

    def fixture(stefan, resolution):
        candidates = [
            entry["request"]
            for entry in reference["results"]
            if entry["stefan_number"] == stefan and entry["resolution"] == resolution
        ]
        if len(candidates) != 1:
            raise ValueError("unique exact native-qualified freezing fixture required")
        return candidates[0]

    with WorkerCampaign(binary, mcp, runtime, args.authority, root) as campaign:
        for interface, stefan, resolution in (("cli", 0.1, 128), ("mcp", 0.2, 256)):
            spec = fixture(stefan, resolution)
            key = f"freezing-{interface}"
            plan = campaign.planned("plan-freezing-reference", spec, key)
            if (
                plan["plan"]["schema_version"] != 12
                or plan["plan"]["freezing"] != spec
                or "case" in plan["plan"]
            ):
                raise ValueError("independent version-12 freezing plan required")
            for rejected_plan, approval, rejected_key in (
                (plan, "a" * 64, f"wrong-{key}"),
                (
                    campaign.planned(
                        "plan-freezing-reference",
                        {**spec, "latent_heat_j_kg": spec["latent_heat_j_kg"] * 1.1},
                        f"changed-{key}",
                    ),
                    plan["approval_digest"],
                    f"changed-{key}",
                ),
            ):
                rejected = campaign.submit(
                    rejected_plan, rejected_key, approval, allow_error=True
                )
                if (
                    rejected["ok"] is not False
                    or rejected["error"]["code"] != "invalid_input"
                ):
                    raise ValueError(
                        "changed inputs and wrong approval must reject before submission"
                    )
            job = (
                campaign.submit(plan, key)
                if interface == "cli"
                else campaign.mcp_submit(
                    "case_plan_freezing_reference", spec, plan, key
                )
            )
            running = campaign.wait(job, {"running"})
            retention = retention_snapshot(campaign.state, job, str(binary))
            reservation = admission_record(campaign.state, job)
            if (
                retention["intent"]["binding"]["sandbox_policy"] != bridge.POLICY
                or reservation is None
                or reservation["cards"] != {}
                or reservation["ram"]
                != max(stage["ram_bytes"] for stage in plan["plan"]["stages"])
            ):
                raise ValueError(
                    "exact CPU policy, immutable roots and shared reservation required"
                )
            campaign.restart()
            if campaign.submit(plan, key)["id"] != job["id"]:
                raise ValueError("restart must preserve durable job idempotency")
            outcome = campaign.wait(job, {"succeeded"})
            if outcome["invocation_id"] != running["invocation_id"]:
                raise ValueError(
                    "worker restart must retain the native service invocation"
                )
            wait_admission_release(campaign.state, job)
            wait_retention_release(campaign.state, job)
            bundle = root / f"bundle-{interface}"
            campaign.command(
                "artifact", "export", "--state", campaign.state, job["id"], bundle
            )
            records = verify_manifest(bundle)
            data = bundle / "stages/freezing"
            receipt = json.loads((data / "freezing-receipt.json").read_text())
            raw_request = (bundle / "native-freezing-request.json").read_bytes()
            if (
                json.loads(raw_request) != spec
                or receipt["request_sha256"] != hashlib.sha256(raw_request).hexdigest()
                or (bundle / "native-runtime.json").read_bytes() != runtime.read_bytes()
            ):
                raise ValueError("exact retained request and runtime bytes required")
            checks, observations, hashes = bridge.verify(spec, receipt, data, fem)
            if (
                receipt["sandbox"]["policy"] != bridge.POLICY
                or not all(receipt["sandbox"]["checks"].values())
                or len(receipt["sandbox"]["checks"]) != 8
            ):
                raise ValueError("complete closure-only CPU sandbox canaries required")
            for observation in observations:
                for extension in ("csv", "vti"):
                    path = f"stages/freezing/freezing-{observation['step']}.{extension}"
                    matching = [record for record in records if record["path"] == path]
                    if (
                        len(matching) != 1
                        or matching[0]["time_s"] != observation["physical_time_s"]
                        or matching[0]["association"] != "native_lattice_point"
                        or "temperature_k:K" not in matching[0]["units"]
                    ):
                        raise ValueError(
                            "complete native time/units/point registration for both original formats required"
                        )
            historical = campaign.command(
                "--socket", campaign.endpoint, "qualify", "--job", job["id"]
            )["data"]
            capability = historical["capabilities"][0]
            if (
                capability["runtime_execution"] != "recorded"
                or capability["numerical_verification"] != "reported_pass"
                or capability["dimensions"] != 2
                or historical["current_runtime_qualification"] != "not_assessed"
                or historical["physical_validation"] != "unqualified"
            ):
                raise ValueError(
                    "separate original-field numerical, runtime and physical evidence states required"
                )
            # Changing closed source bytes must reject a read-only historical
            # query. Restoration preserves the same registered job and approval.
            registered = (
                campaign.state
                / "artifacts"
                / job["id"]
                / "stages/freezing/freezing.pvd"
            )
            original = registered.read_bytes()
            mutated = original.replace(b'part="0"', b'part="1"')
            if mutated == original:
                raise ValueError("explicit portable collection mutation required")
            try:
                registered.write_bytes(mutated)
                rejected = campaign.command(
                    "--socket",
                    campaign.endpoint,
                    "qualify",
                    "--job",
                    job["id"],
                    allow_error=True,
                )
                if rejected["ok"] is not False:
                    raise ValueError("historical original-byte mutation must reject")
            finally:
                registered.write_bytes(original)
            if (
                campaign.command(
                    "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                )["data"]
                != historical
            ):
                raise ValueError(
                    "read-only query must preserve registered historical identity"
                )
            resources = json.loads((bundle / "service-resources.json").read_text())
            if (
                resources["job_id"] != job["id"]
                or resources["invocation"] != running["invocation_id"]
                or resources["kernel_resources"]["controls"]["cpu.max"]
                != "200000 100000"
                or resources["kernel_resources"]["aggregate_memory_peak_bytes"] <= 0
            ):
                raise ValueError(
                    "exact owned service and aggregate resource evidence required"
                )
            results.append(
                {
                    "interface": interface,
                    "job": outcome,
                    "receipt": receipt,
                    "independent_checks": checks,
                    "observations": observations,
                    "original_files_sha256": hashes,
                    "historical_evidence": historical,
                    "admission_record": reservation,
                    "service_resources": resources,
                    "restart": "same invocation",
                    "source_mutation_rejected": True,
                    "reservation_and_roots_released": True,
                }
            )
        for action in ("forced-death", "cancel"):
            plan = campaign.planned(
                "plan-freezing-reference", fixture(0.05, 256), action
            )
            job = campaign.submit(plan, action)
            campaign.wait(job, {"running"})
            retained = retention_snapshot(campaign.state, job, str(binary))
            if action == "forced-death":
                campaign.worker.kill()
                campaign.worker.wait(timeout=5)
                subprocess.run(
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
                )
                campaign.start()
                outcome = campaign.wait(job, {"failed"})
            else:
                campaign.command(
                    "--socket", campaign.endpoint, "job", "cancel", job["id"]
                )
                outcome = campaign.wait(job, {"cancelled"})
            if campaign.submit(plan, action)["id"] != job["id"]:
                raise ValueError("terminal native retries must retain the original job")
            wait_admission_release(campaign.state, job)
            wait_retention_release(campaign.state, job)
            results.append(
                {
                    "action": action,
                    "job": outcome,
                    "active_retention": retained,
                    "retry": "same terminal job",
                    "reservation_and_roots_released": True,
                }
            )
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "native_reference_sha256": checksum(reference_file),
        "results": results,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
        "scope": "exact approved CPU conduction solidification CLI/MCP jobs, original Float64 fields and conservative energy, portable formats, restart/idempotency, owned complete-tree death/cancellation and final release; no retained-water transfer or expansion",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(results),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
