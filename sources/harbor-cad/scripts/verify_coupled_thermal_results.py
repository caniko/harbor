"""Read-only coupled-history samples and full-surface moisture CLI/MCP parity."""

import argparse
import json
import math
import os
import shutil
import sqlite3
import stat
import subprocess
import time
from pathlib import Path

from native_worker_campaign import WorkerCampaign
from verify_results import tree_identity
from verify_spectral_cpu import checksum


def surface_reference(mesh, snapshot, region, size):
    """Select every geometrical face node independently of stored node sets."""
    axis = "xyz".index(region[0])
    position = size[axis] if region.endswith("max") else 0.0
    nodes = {
        int(key): value
        for key, value in mesh["nodes"].items()
        if math.isclose(value[axis], position, rel_tol=0.0, abs_tol=1e-10)
    }
    if not nodes:
        raise ValueError("complete native surface geometry required")
    minimum = min((snapshot["temperature_k"][str(node)], node) for node in nodes)
    return minimum[0], len(nodes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "mcp", "source", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument(
        "--development",
        action="store_true",
        help="Explicitly package-unqualified source diagnostic",
    )
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    binary = args.binary.resolve(strict=True)
    mcp = args.mcp.absolute() if args.development else args.mcp.resolve(strict=True)
    if not args.development and any(
        not p.is_relative_to("/nix/store") or not p.is_file() for p in (binary, mcp)
    ):
        raise ValueError("exact immutable CLI/MCP packages required")
    original = tree_identity(source)
    reference = json.loads((source / "verification.json").read_text())
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Preserve native coupled thermal originals, read-only result attempts and scoped evidence.\n"
    )
    state = root / "state"

    def ignored(directory, names):
        return [
            name
            for name in names
            if stat.S_ISSOCK((Path(directory) / name).lstat().st_mode)
            or name.endswith("-shm")
        ]

    shutil.copytree(source / "state", state, ignore=ignored)
    with sqlite3.connect(state / "jobs.sqlite3") as db:
        if db.execute(
            "SELECT count(*) FROM jobs WHERE state NOT IN ('succeeded','failed','cancelled','interrupted')"
        ).fetchone()[0]:
            raise ValueError("closed native sources required before read-only queries")
        counts = db.execute(
            "SELECT (SELECT count(*) FROM jobs),(SELECT count(*) FROM events),(SELECT count(*) FROM artifacts)"
        ).fetchone()
        cases = {
            result["job"]["id"]
            for result in reference["results"]
            if result["job"]["state"] == "succeeded"
        }
        if len(cases) != 2:
            raise ValueError(
                "both independently qualified CLI/MCP coupled originals required"
            )
        campaign = object.__new__(WorkerCampaign)
        campaign.binary = binary
        campaign.mcp = mcp
        campaign.state = state
        campaign.endpoint = state / "worker.sock"
        campaign.environment = {**os.environ, "PYTHONDONTWRITEBYTECODE": "1"}
        if args.development:
            campaign.environment["PYTHONPATH"] = str(
                Path(__file__).resolve().parents[1] / "python"
            )
        samples, screenings, rejections = [], [], []
        with (root / "worker.log").open("xb") as log:
            worker = subprocess.Popen(
                [
                    str(binary),
                    "worker",
                    "--state",
                    str(state),
                    "--profile",
                    str(source / "profile.json"),
                ],
                env=campaign.environment,
                stdout=log,
                stderr=subprocess.STDOUT,
            )
            try:
                deadline = time.monotonic() + 15
                while not campaign.endpoint.exists():
                    if worker.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(
                            "read-only coupled-results worker failed to start"
                        )
                    time.sleep(0.02)

                def cli(operation, request, accepted=True):
                    process = subprocess.run(
                        [
                            str(binary),
                            "--socket",
                            str(campaign.endpoint),
                            "results",
                            operation,
                            "/dev/stdin",
                        ],
                        input=json.dumps(request, allow_nan=False).encode(),
                        env=campaign.environment,
                        capture_output=True,
                        timeout=30,
                        check=False,
                    )
                    response = json.loads(process.stdout)
                    if (
                        response["ok"] != accepted
                        or bool(process.returncode) == accepted
                    ):
                        raise RuntimeError(
                            f"unexpected coupled-results reply: {response}"
                        )
                    return response["data"] if accepted else response["error"]

                for job in sorted(cases):
                    plan = json.loads(
                        db.execute(
                            "SELECT plan FROM jobs WHERE id=?", (job,)
                        ).fetchone()[0]
                    )
                    if plan["schema_version"] != 11:
                        raise ValueError("source-bound v11 coupling required")
                    for index, stage in enumerate(("thermal-lower", "thermal-upper")):
                        spec = plan["thermal_contact"]["thermal"][index]
                        prefix = state / "artifacts" / job / "stages" / stage
                        mesh = json.loads((prefix / "mesh.json").read_text())
                        fields = json.loads(
                            (prefix / "thermal-fields.json").read_text()
                        )
                        for stamp in spec["observation_times_s"]:
                            snapshot = next(
                                row
                                for row in fields["times"]
                                if row["requested_s"] == stamp
                            )
                            nodes = [int(key) for key in mesh["nodes"]]
                            request = {
                                "schema_version": 1,
                                "job_id": job,
                                "thermal_stage": index,
                                "field": "temperature",
                                "physical_time_s": stamp,
                                "locations": [
                                    {"association": "node", "node_id": node}
                                    for node in (min(nodes), max(nodes))
                                ],
                            }
                            observed = cli("sample-thermal", request)
                            if (
                                campaign.mcp_call(
                                    "results_sample_thermal",
                                    {"request_spec": request},
                                    profile="results",
                                )
                                != observed
                                or observed["thermal_stage"] != index
                                or observed["field_artifact"]["sha256"]
                                != checksum(prefix / "thermal-fields.json")
                                or observed["native_artifact"]["sha256"]
                                != checksum(prefix / "reference.dat")
                                or [row["value"][0] for row in observed["samples"]]
                                != [
                                    snapshot["temperature_k"][str(node)]
                                    for node in (min(nodes), max(nodes))
                                ]
                            ):
                                raise ValueError(
                                    "exact original stage, DAT, native node IDs, retained times and CLI/MCP sample parity required"
                                )
                            samples.append(observed)
                            for region in (
                                "xmin",
                                "xmax",
                                "ymin",
                                "ymax",
                                "zmin",
                                "zmax",
                            ):
                                minimum, count = surface_reference(
                                    mesh, snapshot, region, spec["size_m"]
                                )
                                for assessment in (
                                    {
                                        "assessment": "missing",
                                        "reason": "no air inputs supplied",
                                    },
                                    {
                                        "assessment": "inapplicable",
                                        "justification": "explicit synthetic dry conduction reference",
                                    },
                                    {
                                        "assessment": "dew_point_screening",
                                        "air_temperature": {
                                            "value": 20.0,
                                            "unit": "degC",
                                        },
                                        "relative_humidity": 0.5,
                                        "provenance": "manufactured constant indoor air; no measured frost transport",
                                    },
                                ):
                                    moisture = {
                                        "schema_version": 1,
                                        "job_id": job,
                                        "thermal_stage": index,
                                        "physical_time_s": stamp,
                                        "surface_region": region,
                                        "moisture_risk": assessment,
                                    }
                                    observed = cli("moisture", moisture)
                                    if (
                                        campaign.mcp_call(
                                            "results_moisture",
                                            {"request_spec": moisture},
                                            profile="results",
                                        )
                                        != observed
                                        or observed["source"]["thermal_stage"] != index
                                        or observed["source"]["samples"][0]["value"]
                                        != [minimum]
                                        or observed["surface_nodes"] != count
                                        or observed["request"] != moisture
                                        or observed["physical_validation"]
                                        != "unqualified"
                                    ):
                                        raise ValueError(
                                            "complete geometrical surface minimum, explicit air assessment and CLI/MCP moisture parity required"
                                        )
                                    screenings.append(observed)
                        for field, value in (
                            ("thermal_stage", 2),
                            ("thermal_stage", None),
                            ("thermal_stage", "../thermal-upper"),
                            ("physical_time_s", stamp - 0.001),
                            ("field", "stress"),
                            ("artifact_path", "stages/contact/reference.dat"),
                        ):
                            changed = {**request, field: value}
                            error = cli("sample-thermal", changed, False)
                            refused = campaign.mcp_call(
                                "results_sample_thermal",
                                {"request_spec": changed},
                                profile="results",
                                expect_error=True,
                            )
                            if error["code"] + ":" not in str(refused):
                                raise ValueError(
                                    "coupled thermal refusal parity required"
                                )
                            rejections.append(error)
                        data = (prefix / "reference.dat").read_bytes()
                        try:
                            (prefix / "reference.dat").write_bytes(
                                data + b"changed original native field"
                            )
                            rejections.append(cli("sample-thermal", request, False))
                            campaign.mcp_call(
                                "results_moisture",
                                {"request_spec": moisture},
                                profile="results",
                                expect_error=True,
                            )
                        finally:
                            (prefix / "reference.dat").write_bytes(data)
                        cli("sample-thermal", request)
            finally:
                worker.terminate()
                worker.wait(timeout=5)
        if (
            db.execute(
                "SELECT (SELECT count(*) FROM jobs),(SELECT count(*) FROM events),(SELECT count(*) FROM artifacts)"
            ).fetchone()
            != counts
        ):
            raise ValueError(
                "read-only queries changed native job/event/artifact records"
            )
    if tree_identity(source) != original:
        raise ValueError("independent native source originals changed")
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "source": str(source),
        "source_verification_sha256": checksum(source / "verification.json"),
        "source_unchanged": True,
        "samples": samples,
        "moisture": screenings,
        "rejections": rejections,
        "package_qualification": "unqualified source diagnostic"
        if args.development
        else "exact immutable query packages against independently qualified native originals",
        "physical_validation": "unqualified",
        "scope": "explicit lower/upper complete original thermal histories and full geometrical surface moisture assessment; no new solve or physical reliability inference",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "samples": len(samples),
                "moisture": len(screenings),
                "rejections": len(rejections),
                "package_qualification": report["package_qualification"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
