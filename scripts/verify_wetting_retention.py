"""Read-only CLI/MCP retained phase extrusion against closed native originals."""

import argparse
import asyncio
import csv
import json
import math
import os
import shutil
import sqlite3
import stat
import subprocess
import time
from pathlib import Path

from verify_openlb_hip import service_resources
from verify_results import tree_identity
from verify_wetting_cpu import checksum


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "mcp", "source", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument(
        "--development",
        action="store_true",
        help="Explicitly unqualified source-built CLI and Python-module MCP diagnostic",
    )
    args = parser.parse_args()
    binary, mcp, source = (
        p.resolve(strict=True) for p in (args.binary, args.mcp, args.source)
    )
    if not args.development and not all(
        p.is_relative_to("/nix/store") for p in (binary, mcp)
    ):
        raise ValueError("exact immutable CLI/MCP packages required")
    before = service_resources()
    original_identity = tree_identity(source)
    reference = json.loads((source / "verification.json").read_text())
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    (root / ".doty-protect").write_text(
        "Active closed native phase initialization and read-only result evidence; preserve.\n"
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
    database = sqlite3.connect(state / "jobs.sqlite3")
    if database.execute(
        "select count(*) from jobs where state in ('queued','starting','running','cancelling')"
    ).fetchone()[0]:
        raise ValueError(
            "closed native wetting sources required before retained initialization"
        )
    counts = database.execute(
        "select (select count(*) from jobs),(select count(*) from events),(select count(*) from artifacts)"
    ).fetchone()
    schemas = json.loads(subprocess.check_output([str(binary), "schema"]))
    from jsonschema import Draft202012Validator

    environment = {
        **os.environ,
        "HARBOR_CAD_SOCKET": str(state / "worker.sock"),
        "PYTHONDONTWRITEBYTECODE": "1",
    }
    if args.development:
        environment["PYTHONPATH"] = str(Path(__file__).resolve().parents[1] / "python")
    reports, rejections = [], []
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
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + 15
            while not (state / "worker.sock").exists():
                if worker.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("read-only retention worker startup failed")
                time.sleep(0.02)

            def cli(request, accepted=True):
                process = subprocess.run(
                    [str(binary), "results", "retain-wetting", "/dev/stdin"],
                    input=json.dumps(request, allow_nan=False).encode(),
                    env=environment,
                    capture_output=True,
                    timeout=30,
                    check=False,
                )
                value = json.loads(process.stdout)
                if value["ok"] != accepted or bool(process.returncode) == accepted:
                    raise RuntimeError(
                        f"unexpected retained initialization reply: {process.stdout.decode()}"
                    )
                return value["data"] if accepted else value["error"]

            async def exercise():
                from mcp import Client
                from mcp.client.stdio import StdioServerParameters

                parameters = StdioServerParameters(
                    command=str(mcp),
                    args=["-m", "harbor_cad_mcp.server", "--profile", "results"]
                    if args.development
                    else ["--profile", "results"],
                    env=environment,
                )
                async with Client(parameters) as client:
                    names = {tool.name for tool in (await client.list_tools()).tools}
                    if "results_retain_wetting" not in names or "job_submit" in names:
                        raise ValueError("read-only result-profile parity required")
                    for result in reference["results"]:
                        job = result["job"]
                        if job["state"] != "succeeded":
                            continue
                        plan = json.loads(
                            database.execute(
                                "select plan from jobs where id=?", (job["id"],)
                            ).fetchone()[0]
                        )
                        spec = plan["wetting"]
                        directory = state / "artifacts" / job["id"] / "stages/wetting"
                        for step in spec["observation_steps"]:
                            registered = json.loads(
                                database.execute(
                                    "select manifest from artifacts where job=? and path=?",
                                    (job["id"], f"stages/wetting/wetting-{step}.csv"),
                                ).fetchone()[0]
                            )
                            stamp = registered["time_s"]
                            for depth_mm in (0.5, 1.0):
                                request = {
                                    "schema_version": 1,
                                    "source_job": job["id"],
                                    "physical_time_s": stamp,
                                    "extrusion": {"value": depth_mm, "unit": "mm"},
                                    "extrusion_provenance": "prescribed synthetic constant depth for original phase distribution; no measured retained volume",
                                    "destination_region": "retained_phase",
                                    "destination_origin_m": [0.001, 0.002, 0.003],
                                    "maximum_relative_conservation_error": 1e-10,
                                }
                                Draft202012Validator(
                                    schemas["WettingRetentionRequest"]
                                ).validate(request)
                                actual = cli(request)
                                Draft202012Validator(
                                    schemas["RetainedWettingReport"]
                                ).validate(actual)
                                copied = await client.call_tool(
                                    "results_retain_wetting", {"request_spec": request}
                                )
                                if (
                                    copied.is_error
                                    or copied.structured_content != actual
                                ):
                                    raise ValueError(
                                        "retained original CLI/MCP initialization parity required"
                                    )
                                field = directory / f"wetting-{step}.csv"
                                with field.open() as handle:
                                    rows = list(csv.DictReader(handle))
                                fractions = [
                                    1.0 - float(row["phi"])
                                    for row in rows
                                    if row["material"] == "1"
                                ]
                                dx = spec["diameter_m"] / spec["resolution"]
                                rho = spec["density_liquid_kg_m3"]
                                expected = math.fsum(
                                    value * rho * dx**2 * depth_mm / 1000
                                    for value in fractions
                                )
                                transferred = actual["extrusion"]
                                if (
                                    actual["original_field"]["sha256"]
                                    != checksum(field)
                                    or not math.isclose(
                                        transferred["destination_phase_mass_kg"],
                                        expected,
                                        rel_tol=1e-12,
                                    )
                                    or transferred["source_phase_fraction_range"]
                                    != [min(fractions), max(fractions)]
                                    or transferred["nonphysical_phase_cells"]
                                    != sum(not 0 <= value <= 1 for value in fractions)
                                    or transferred["native_step"] != step
                                    or transferred["destination_cells"]
                                    != len(fractions)
                                    or transferred["relative_conservation_error"]
                                    > 1e-10
                                    or actual["temperature_state"]
                                    != "missing_from_native_wetting_source"
                                    or actual["executed"] is not False
                                ):
                                    raise ValueError(
                                        "complete original signed phase distribution, exact time, no clipping and independent extruded conservation required"
                                    )
                                reports.append(actual)
                        request = reports[-1]["request"]
                        for field, value in (
                            ("physical_time_s", stamp + spec["diameter_m"]),
                            ("extrusion", {"value": 0.0, "unit": "m"}),
                            ("maximum_relative_conservation_error", 1e-3),
                            ("extrusion_provenance", ""),
                            ("destination_origin_m", [1e6, 0.0, 0.0]),
                            ("temperature_k", 273.15),
                        ):
                            rejected_request = {**request, field: value}
                            rejected = cli(rejected_request, False)
                            mcp_rejected = await client.call_tool(
                                "results_retain_wetting",
                                {"request_spec": rejected_request},
                            )
                            if (
                                rejected["code"] != "invalid_input"
                                or not mcp_rejected.is_error
                                or rejected["code"] not in str(mcp_rejected.content)
                            ):
                                raise ValueError(
                                    "unsupported retained initialization must reject identically through CLI/MCP"
                                )
                            rejections.append(
                                {"source": job["id"], "field": field, "error": rejected}
                            )
                        field = directory / f"wetting-{step}.csv"
                        original = field.read_bytes()
                        try:
                            field.write_bytes(original[:-100])
                            rejected = cli(request, False)
                            if rejected["code"] != "invalid_input":
                                raise ValueError(
                                    "registered original corruption must fail before retained initialization"
                                )
                            rejections.append(
                                {
                                    "source": job["id"],
                                    "field": "original_byte_corruption",
                                    "error": rejected,
                                }
                            )
                        finally:
                            field.write_bytes(original)
                        if cli(request) != reports[-1]:
                            raise ValueError(
                                "read-only rejection must leave original source identities recoverable"
                            )

            asyncio.run(exercise())
        finally:
            worker.terminate()
            worker.wait(timeout=5)
    if (
        tree_identity(source) != original_identity
        or database.execute(
            "select (select count(*) from jobs),(select count(*) from events),(select count(*) from artifacts)"
        ).fetchone()
        != counts
    ):
        raise ValueError(
            "retention result operations must preserve original source bytes and database records"
        )
    report = {
        "schema_version": 1,
        "scope": "source-bound original phase/velocity extrusion; no cooling execution or physical retention qualification",
        "package_qualification": "unqualified development CLI/Python module"
        if args.development
        else "exact immutable packages",
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "source_report_sha256": checksum(source / "verification.json"),
        "source_before_after_unchanged": True,
        "database_counts_before_after": counts,
        "results": reports,
        "rejections": rejections,
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "physical_validation": "unqualified",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "results": len(reports),
                "rejections": len(rejections),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
