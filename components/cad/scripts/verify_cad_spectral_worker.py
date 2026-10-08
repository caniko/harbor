"""Registered original-CAD direct optics, independent power/dose and owned lifecycle."""

import argparse
import copy
import fcntl
import json
import math
import shutil
import subprocess
from pathlib import Path

from atmospheric_transport_prerequisites import immutable
from native_worker_campaign import WorkerCampaign
from verify_atmospheric_transport_worker import tree_identity
from verify_cad_spectral_cpu import module
from verify_native_cpu import verify_manifest
from verify_openlb_hip import service_resources
from verify_spectral_cpu import checksum
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_retention_release,
)


def source_records(report):
    all_records = report["results"]
    records = [row for row in all_records if "source" in row]
    if (
        len(records) != 2
        or [row["source"] for row in records] != ["origin", "translated"]
        or any(row["roots_and_reservation_released"] is not True for row in all_records)
        or any(
            row["job"]["state"] != "succeeded"
            or row["job"]["exit_code"] != 0
            or "reimport" not in row
            for row in records
        )
    ):
        raise ValueError(
            "two independently qualified original CAD source inspections/reimports required"
        )
    return records


def copy_closed_source_state(original, destination):
    # The worker owns this advisory lock for its entire lifetime. Read-only
    # exclusive acquisition proves closure even when its Unix socket remains.
    # Never open the original SQLite connection or alter its WAL shared index.
    with (original / "worker.lock").open("rb") as lock:
        try:
            fcntl.flock(lock.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError(
                "closed original source worker required before database snapshot"
            ) from error
        shutil.copytree(original / "artifacts", destination / "artifacts")
        for name in ("jobs.sqlite3", "jobs.sqlite3-wal", "jobs.sqlite3-shm"):
            path = original / name
            if path.exists():
                shutil.copyfile(path, destination / name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("executable", "mcp", "runtime", "authority", "source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    binary, mcp, runtime = map(immutable, (args.executable, args.mcp, args.runtime))
    native = json.loads(runtime.read_text())
    if (
        not native.get("spectral")
        or not native["spectral"].endswith("/bin/harbor-cad-cad-spectral-direct")
        or any(
            native.get(k)
            for k in ("cad", "openlb", "render", "video", "fem", "filter", "thermal")
        )
    ):
        raise ValueError(
            "exact independently scoped direct-only optical worker runtime required"
        )
    for key in ("spectral", "spectral_closure", "bwrap"):
        immutable(native[key])
    original = args.source.resolve(strict=True)
    report = json.loads((original / "verification.json").read_text())
    sources = source_records(report)
    baseline = tree_identity(original)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Preserve registered-CAD optical originals, independent scope and failed attempts.\n"
    )
    (root / "source-reference.json").write_text(
        json.dumps(
            {
                "path": str(original),
                "report_sha256": checksum(original / "verification.json"),
                "original_tree": baseline,
            },
            indent=2,
        )
    )
    # The qualified source campaign has stopped its worker. Copy its complete
    # SQLite/WAL snapshot without opening it: even mode=ro can alter the original
    # shared-memory WAL index. Only the private state is opened or mutated.
    state = root / "state"
    state.mkdir(mode=0o700)
    copy_closed_source_state(original / "state", state)
    verifier = module(
        Path(__file__).resolve().parents[1] / "adapters/cad_spectral_transport.py"
    )
    request = json.loads(
        (
            Path(__file__).resolve().parents[1] / "examples/cad-spectral-transport.json"
        ).read_text()
    )
    request["samples_per_triangle"] = 1024
    before = service_resources()
    results, rejections, lifecycle = [], [], []
    with WorkerCampaign(
        binary, mcp, runtime, args.authority, root, timeout=600
    ) as campaign:
        for index, row in enumerate(sources):
            current = copy.deepcopy(request)
            current["scene"]["source_job"] = row["reimport"]["id"]
            if index:
                current["scene"]["materials"][0]["ageing_action"] = {
                    "availability": "known",
                    "value": [0.25, 0.75],
                    "provenance": "explicit manufactured action; no material lifetime calibration",
                    "synthetic": True,
                }
            path = root / f"request-{index}.json"
            path.write_text(json.dumps(current))
            if row["source"] == "translated":
                refusal = campaign.command(
                    "--socket",
                    campaign.endpoint,
                    "cad",
                    "plan-spectral-transport",
                    path,
                    allow_error=True,
                )
                assert not refusal["ok"] and "Float32" in refusal["error"]["message"]
                mcp_refusal = campaign.mcp_call(
                    "cad_plan_spectral_transport",
                    {"request_spec": current},
                    expect_error=True,
                )
                assert "Float32" in str(mcp_refusal)
                rejections.append(
                    {
                        "source": "translated",
                        "native_geometry_refusal": refusal,
                        "scope": "original far translation exceeds fixed native geometry limits; no rebasing or tolerance increase",
                    }
                )
                # Separately approve known material action on the admissible
                # origin source. This is an explicit second case, not a repair
                # or scientific substitution of the refused translated source.
                row = {**sources[0], "source": "origin-prescribed-action"}
                current["scene"]["source_job"] = row["reimport"]["id"]
                path.write_text(json.dumps(current))
            planned = campaign.command(
                "--socket", campaign.endpoint, "cad", "plan-spectral-transport", path
            )["data"]
            assert planned == campaign.mcp_call(
                "cad_plan_spectral_transport", {"request_spec": current}
            )
            assert planned["plan"]["schema_version"] == 17
            source_tree = state / "artifacts" / current["scene"]["source_job"]
            unchanged = tree_identity(source_tree)
            original_stl = source_tree / "solid.stl"
            raw = original_stl.read_bytes()
            try:
                original_stl.write_bytes(raw + b"changed before acknowledgement")
                assert not campaign.submit(
                    planned, f"reject-source-{index}", allow_error=True
                )["ok"]
                rejections.append({"changed_original_before_ack": index})
            finally:
                original_stl.write_bytes(raw)
            job = campaign.submit(planned, f"optical-{index}")
            acknowledged = state / "artifacts" / job["id"] / "source-cad/solid.stl"
            assert (
                acknowledged.read_bytes() == raw
                and acknowledged.stat().st_ino != original_stl.stat().st_ino
            )
            owner = campaign.wait(job, {"running"})
            active = retention_snapshot(state, job, str(binary))
            reservation = admission_record(state, job)
            assert (
                reservation is not None
                and active["intent"]["binding"]["sandbox_policy"]
                == "harbor-cad-cad-spectral-direct-cpu-v1"
            )
            try:
                original_stl.write_bytes(raw + b"changed after acknowledgement")
                path.write_text("{}")
                campaign.restart()
                assert (
                    campaign.mcp_call(
                        "job_submit",
                        {
                            "plan": planned["plan"],
                            "approved_digest": planned["approval_digest"],
                            "idempotency_key": f"optical-{index}",
                        },
                    )["id"]
                    == job["id"]
                )
                outcome = campaign.wait(job, {"succeeded"})
                assert outcome["invocation_id"] == owner["invocation_id"]
            finally:
                original_stl.write_bytes(raw)
            bundle = root / f"bundle-{index}"
            campaign.command("artifact", "export", "--state", state, job["id"], bundle)
            manifests = verify_manifest(bundle)
            work = bundle / "stages/cad-spectral"
            receipt = json.loads((work / "cad-spectral-receipt.json").read_text())
            spec = json.loads((bundle / "native-cad-spectral-request.json").read_text())
            assert receipt["request_sha256"] == checksum(
                bundle / "native-cad-spectral-request.json"
            )
            assert (
                receipt["sandbox"]["policy"] == "harbor-cad-cad-spectral-direct-cpu-v1"
                and len(receipt["sandbox"]["checks"]) == 9
                and all(receipt["sandbox"]["checks"].values())
            )
            normalized = verifier.normalize(spec, bundle / "source-cad")
            totals = []
            for observation in receipt["observations"]:
                facets = verifier.reconstruct(
                    spec,
                    normalized,
                    receipt["geometry_conversions"],
                    work / observation["original"]["path"],
                )
                assert facets == observation["facets"]
                power = math.fsum(facet["power_w"]["incident"] for facet in facets)
                widths = normalized["regions"][0]["bounds_m"]
                area = (widths[1] - widths[0]) * (widths[3] - widths[2])
                expected = 150.0 * area
                assert math.isclose(power, expected, rel_tol=2e-6, abs_tol=0.0)
                assert all(
                    ("ageing" in f["channels_w_m2"]) == bool(index) for f in facets
                )
                assert all(
                    math.isclose(
                        f["energy_j"]["incident"],
                        f["power_w"]["incident"] * 7200.0,
                        rel_tol=1e-12,
                        abs_tol=0.0,
                    )
                    for f in facets
                )
                totals.append(
                    {
                        "seed": observation["seed"],
                        "incident_power_w": power,
                        "independent_projected_power_w": expected,
                    }
                )
            fields = [record for record in manifests if record["path"].endswith(".csv")]
            assert len(fields) == 3 and all(
                record["association"] == "native_original_facet_spectral_packet"
                and record["time_s"] is None
                for record in fields
            )
            qualification = campaign.command(
                "--socket", campaign.endpoint, "qualify", "--job", job["id"]
            )["data"]
            assert qualification == campaign.mcp_call(
                "qualification_report", {"job_id": job["id"]}, profile="results"
            )
            capability = next(
                c
                for c in qualification["capabilities"]
                if c["stage_id"] == "cad-spectral"
            )
            assert (
                capability["runtime_execution"] == "recorded"
                and capability["numerical_verification"] == "reported_pass"
                and capability["physical_validation"] == "unqualified"
            )
            queries = []
            for observation in receipt["observations"]:
                query = {
                    "schema_version": 1,
                    "job_id": job["id"],
                    "seed": observation["seed"],
                    "region_name": "solid",
                }
                path = root / f"query-{index}-{observation['seed']}.json"
                path.write_text(json.dumps(query))
                view = campaign.command(
                    "--socket", campaign.endpoint, "results", "cad-optical", path
                )["data"]
                assert view == campaign.mcp_call(
                    "results_cad_optical", {"request_spec": query}, profile="results"
                )
                assert (
                    view["facets"] == observation["facets"]
                    and view["physical_time_s"] is None
                    and view["history"] == current["history"]
                    and view["physical_validation"] == "unqualified"
                )
                assert view["original_packets"]["sha256"] == observation["original"][
                    "sha256"
                ] and view["source_triangles"]["sha256"] == checksum(
                    bundle / "source-cad/solid.stl"
                )
                assert (
                    view["field_units"]["power_w"] == "W"
                    and view["field_units"]["energy_j"] == "J"
                )
                queries.append(view)
                assert view["wavelengths"] == current["scene"]["wavelengths"]
            assert not campaign.submit(
                planned, f"approval-drift-{index}", approval="0" * 64, allow_error=True
            )["ok"]
            wait_admission_release(state, job)
            wait_retention_release(state, job)
            assert tree_identity(source_tree) == unchanged
            resources = json.loads((bundle / "service-resources.json").read_text())
            results.append(
                {
                    "source": row["source"],
                    "source_job": current["scene"]["source_job"],
                    "job": outcome,
                    "approval_digest": planned["approval_digest"],
                    "original_distinct_inodes": True,
                    "source_unchanged": True,
                    "restart_preserved_invocation": True,
                    "roots_and_reservation_released": True,
                    "power": totals,
                    "qualification": qualification,
                    "queries": queries,
                    "resources": resources,
                }
            )
        for name, target in (("cancel", "cancelled"), ("service-death", "failed")):
            key = "cad-optical-" + name
            current = copy.deepcopy(request)
            current["scene"]["source_job"] = sources[0]["reimport"]["id"]
            current["samples_per_triangle"] = 4096
            planned = campaign.mcp_call(
                "cad_plan_spectral_transport", {"request_spec": current}
            )
            job = campaign.submit(planned, key)
            owner = campaign.wait(job, {"running"})
            active = retention_snapshot(state, job, str(binary))
            assert admission_record(state, job) is not None
            if name == "cancel":
                campaign.mcp_call("job_cancel", {"job_id": job["id"]})
            else:
                subprocess.run(
                    [
                        "systemctl",
                        "--user",
                        "kill",
                        "--kill-whom=all",
                        "--signal=SIGKILL",
                        owner["unit"],
                    ],
                    check=True,
                    timeout=30,
                )
            campaign.restart()
            outcome = campaign.wait(job, {target})
            assert campaign.submit(planned, key)["id"] == job["id"]
            wait_admission_release(state, job)
            wait_retention_release(state, job)
            lifecycle.append(
                {
                    "kind": name,
                    "job": outcome,
                    "active_retention": active,
                    "roots_and_reservation_released": True,
                    "terminal_idempotency": True,
                }
            )
    assert tree_identity(original) == baseline
    verification = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "campaign_sha256": checksum(Path(__file__)),
        "source": str(original),
        "source_verification_sha256": checksum(original / "verification.json"),
        "source_unchanged": True,
        "results": results,
        "rejections": rejections,
        "lifecycle": lifecycle,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "scope": "independently approved registered original-CAD direct-only CPU optical power/dose; no interreflection, atmosphere, GPU, sampling convergence or physical qualification",
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(verification, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "jobs": len(results),
                "seeds": sum(len(row["power"]) for row in results),
                "lifecycle": len(lifecycle),
                "report_sha256": checksum(root / "verification.json"),
            }
        )
    )


if __name__ == "__main__":
    main()
