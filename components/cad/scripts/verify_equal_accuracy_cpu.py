"""Paired exact native CPU timings with identical physics, originals and gates."""

import argparse
import json
import math
import statistics
import time
from pathlib import Path

from atmospheric_transport_prerequisites import immutable, sandbox
from native_worker_campaign import WorkerCampaign
from verify_native_cpu import verify_manifest
from verify_spectral_cpu import checksum
from verify_study_worker import prerequisites
from verify_systemd import wait_admission_release, wait_retention_release


def compare_originals(reference, observed):
    """Compare complete original native nodal values at identical physical times."""
    if not reference or len(reference) != len(observed):
        raise ValueError("complete equal-accuracy original observations required")
    differences = []
    for left, right in zip(reference, observed, strict=True):
        if (
            left["requested_s"] != right["requested_s"]
            or left["observed_s"] != right["observed_s"]
            or not left["temperature_k"]
            or left["temperature_k"].keys() != right["temperature_k"].keys()
        ):
            raise ValueError(
                "same complete native physical times and point IDs required"
            )
        for node, value in left["temperature_k"].items():
            current = right["temperature_k"][node]
            if any(
                isinstance(v, bool) or not math.isfinite(v) for v in (value, current)
            ):
                raise ValueError("finite original thermal values required")
            differences.append(abs(current - value))
    maximum = max(differences)
    if maximum > 1e-12:
        raise ValueError(
            "equal-accuracy benchmark changed original native temperatures"
        )
    return maximum


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
    parser.add_argument("--repetitions", type=int, default=5)
    args = parser.parse_args()
    if not 5 <= args.repetitions <= 10:
        parser.error("five to ten recorded paired repetitions required")
    binary, mcp, runtime = (
        immutable(p) for p in (args.executable, args.mcp, args.runtime)
    )
    repo = Path(__file__).resolve().parents[1]
    source, spec, fem, thermal = prerequisites(
        runtime, args.native_reference.resolve(strict=True), repo
    )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "All equal-accuracy native originals, warmups and independent failures; preserve.\n"
    )
    records, reference, reference_mesh, science = [], None, None, None
    with WorkerCampaign(
        binary, mcp, runtime, args.authority, root, timeout=300
    ) as campaign:
        planned = campaign.planned("plan-thermal-reference", spec, "fixed-science")
        plan_file = root / "approved-plan.json"
        plan_file.write_text(json.dumps(planned["plan"]))
        for repetition in range(args.repetitions + 1):
            # Alternate the within-pair order; retain every sample and warmup.
            for threads in (1, 2) if repetition % 2 == 0 else (2, 1):
                label = f"threads-{threads}-repeat-{repetition}"
                profile = json.loads(campaign.profile.read_text())
                profile["threads"] = threads
                campaign.profile.write_text(json.dumps(profile))
                campaign.restart()
                begin = time.perf_counter()
                job = campaign.command(
                    "--socket",
                    campaign.endpoint,
                    "job",
                    "submit",
                    plan_file,
                    "--approve",
                    planned["approval_digest"],
                    "--idempotency-key",
                    label,
                )["data"]
                campaign.owned.append(job["unit"])
                outcome = campaign.wait(job, {"succeeded"})
                solved = time.perf_counter()
                bundle = root / ("bundle-" + label)
                campaign.command(
                    "artifact", "export", "--state", campaign.state, job["id"], bundle
                )
                manifests = verify_manifest(bundle)
                exported = time.perf_counter()
                work = bundle / "stages/thermal"
                mesh = json.loads((work / "mesh.json").read_text())
                nodes, cells = (
                    {int(k): v for k, v in mesh[name].items()}
                    for name in ("nodes", "elements")
                )
                checks, _, original = thermal.verify(
                    spec,
                    nodes,
                    cells,
                    fem.read_dat((work / "reference.dat").read_text()),
                )
                original = json.loads(json.dumps(original))
                receipt = json.loads((work / "thermal-receipt.json").read_text())
                if (
                    checks != receipt["numerical_verification"]
                    or original
                    != json.loads((work / "thermal-fields.json").read_text())["times"]
                ):
                    raise ValueError(
                        "unchanged independent original-field numerical gates required"
                    )
                sandbox(receipt, "harbor-cad-thermal-cpu-v1")
                historical = campaign.command(
                    "--socket", campaign.endpoint, "qualify", "--job", job["id"]
                )["data"]
                if (
                    historical["capabilities"][0]["numerical_verification"]
                    != "reported_pass"
                ):
                    raise ValueError(
                        "registered complete original numerical evidence required"
                    )
                current_science = historical["science_id"]
                if reference is None:
                    reference, reference_mesh, science = original, mesh, current_science
                if current_science != science or mesh != reference_mesh:
                    raise ValueError(
                        "same immutable science and original mesh required across execution profiles"
                    )
                difference = compare_originals(reference, original)
                resources = json.loads((bundle / "service-resources.json").read_text())
                kernel = resources["kernel_resources"]
                if (
                    kernel["controls"]["cpu.max"] != f"{threads * 100000} 100000"
                    or kernel["controls"]["memory.swap.max"] != "0"
                ):
                    raise ValueError(
                        "effective requested CPU quota and zero-swap controls required"
                    )
                recorded_profile = json.loads(
                    (bundle / "host-profile.json").read_text()
                )
                if recorded_profile != profile:
                    raise ValueError(
                        "each effective profile must be bound before execution"
                    )
                wait_admission_release(campaign.state, job)
                wait_retention_release(campaign.state, job)
                records.append(
                    {
                        "label": label,
                        "warmup": repetition == 0,
                        "repetition": repetition,
                        "threads": threads,
                        "job": outcome,
                        "science_id": current_science,
                        "submission_through_terminal_s": solved - begin,
                        "submission_through_checksum_export_s": exported - begin,
                        "checksum_export_s": exported - solved,
                        "maximum_original_difference_k": difference,
                        "independent_checks": checks,
                        "resources": resources,
                        "original_output_bytes": sum(
                            p.stat().st_size for p in work.iterdir() if p.is_file()
                        ),
                        "export_bytes": sum(item["bytes"] for item in manifests),
                        "original_csv_json_dat_sha256": {
                            p.name: checksum(p) for p in work.iterdir() if p.is_file()
                        },
                        "manifest_sha256": checksum(bundle / "manifest.json"),
                        "released": True,
                    }
                )
                (root / "observations.json").write_text(
                    json.dumps(records, indent=2, allow_nan=False)
                )
                print(
                    label, "solve/export", solved - begin, exported - begin, flush=True
                )
    summaries = {}
    for threads in (1, 2):
        samples = [
            row for row in records if not row["warmup"] and row["threads"] == threads
        ]
        elapsed = [row["submission_through_checksum_export_s"] for row in samples]
        summaries[str(threads)] = {
            "samples": len(samples),
            "median_end_to_end_s": statistics.median(elapsed),
            "minimum_end_to_end_s": min(elapsed),
            "maximum_end_to_end_s": max(elapsed),
            "median_cpu_usage_usec": statistics.median(
                row["resources"]["kernel_resources"][
                    "cpu_stat_microseconds_and_counts"
                ]["usage_usec"]
                for row in samples
            ),
            "maximum_job_cgroup_peak_bytes": max(
                row["resources"]["kernel_resources"]["aggregate_memory_peak_bytes"]
                for row in samples
            ),
        }
    fastest = min(summaries, key=lambda k: summaries[k]["median_end_to_end_s"])
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "campaign_sha256": checksum(Path(__file__)),
        "native_reference": str(source),
        "native_reference_sha256": checksum(source),
        "authority_sha256": checksum(args.authority),
        "approval_digest": planned["approval_digest"],
        "science_id": science,
        "request": spec,
        "records": records,
        "summary_by_threads": summaries,
        "fastest_observed_median_threads": int(fastest),
        "two_core_over_one_core_median_time": summaries["2"]["median_end_to_end_s"]
        / summaries["1"]["median_end_to_end_s"],
        "equal_accuracy_gate_k": 1e-12,
        "timing_scope": "wall clock before ordinary approved submission through atomic offline export/checksum verification; independent replay timing excluded and separately executed for every sample",
        "ordering": "paired alternating one/two-core order; one retained warmup per configuration; no samples discarded or outlier trimming",
        "claim_scope": "measured CPU execution-profile choice on this fixed synthetic case and exact runtime; no fleet-wide, GPU or physical-performance generalization",
        "jit": "not applicable to this precompiled native CPU solver; warmups do not claim OS cache flushing",
        "vram": "not requested or measured; CPU-only",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "summary_by_threads": summaries,
                "fastest_observed_median_threads": int(fastest),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
