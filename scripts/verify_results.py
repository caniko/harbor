"""Exact CLI/MCP native sampling/comparison against retained qualified fields."""

import argparse
import asyncio
import hashlib
import importlib.util
import json
import os
import shutil
import sqlite3
import stat
import subprocess
import time
from pathlib import Path

from verify_openlb_hip import service_resources


def checksum(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_identity(root):
    return {
        str(p.relative_to(root)): checksum(p)
        for p in root.rglob("*")
        if stat.S_ISREG(p.lstat().st_mode) and not p.name.endswith("-shm")
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("binary", "mcp", "output"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--source", type=Path, action="append", required=True)
    parser.add_argument("--thermal", action="store_true")
    args = parser.parse_args()
    binary, mcp = args.binary.resolve(strict=True), args.mcp.resolve(strict=True)
    if not all(p.is_relative_to("/nix/store") for p in (binary, mcp)):
        raise ValueError("exact immutable packaged CLI/MCP required")
    before = service_resources()
    schemas = json.loads(subprocess.check_output([str(binary), "schema"]))
    from jsonschema import Draft202012Validator

    module = importlib.util.spec_from_file_location(
        "fem", Path(__file__).resolve().parents[1] / "adapters/fem_reference.py"
    )
    fem = importlib.util.module_from_spec(module)
    module.loader.exec_module(fem)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    reports = []
    source_versions = set()
    for index, source in enumerate(args.source):
        source = source.resolve(strict=True)
        source_report = json.loads((source / "verification.json").read_text())
        source_before = tree_identity(source)
        state = root / f"state-{index}"

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
            raise ValueError("closed qualified sources required for read-only sampling")
        start_counts = database.execute(
            "select (select count(*) from jobs),(select count(*) from events),(select count(*) from artifacts)"
        ).fetchone()
        environment = {**os.environ, "HARBOR_CAD_SOCKET": str(state / "worker.sock")}
        with (root / f"worker-{index}.log").open("xb") as log:
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
                        raise RuntimeError("read-only result worker startup failed")
                    time.sleep(0.02)

                def cli(operation, request, accepted=True, environment=environment):
                    process = subprocess.run(
                        [str(binary), "results", operation, "/dev/stdin"],
                        input=json.dumps(request).encode(),
                        env=environment,
                        capture_output=True,
                        timeout=30,
                        check=False,
                    )
                    if bool(process.returncode) == accepted:
                        raise RuntimeError(
                            f"unexpected results {operation} exit: {process.stdout.decode()}"
                        )
                    value = json.loads(process.stdout)
                    assert value["ok"] == accepted
                    return value["data"] if accepted else value["error"]

                async def check(
                    environment=environment,
                    source_report=source_report,
                    database=database,
                    state=state,
                    cli=cli,
                ):
                    from mcp import Client
                    from mcp.client.stdio import StdioServerParameters

                    parameters = StdioServerParameters(
                        command=str(mcp), args=["--profile", "results"], env=environment
                    )
                    results, rejections = [], []
                    async with Client(parameters) as client:
                        names = {
                            tool.name for tool in (await client.list_tools()).tools
                        }
                        assert {"results_sample", "results_compare"}.issubset(
                            names
                        ) and "job_submit" not in names
                        for result in source_report["results"]:
                            job = result["job"]
                            if job["state"] != "succeeded":
                                continue
                            raw_plan = database.execute(
                                "select plan from jobs where id=?", (job["id"],)
                            ).fetchone()[0]
                            plan = json.loads(raw_plan)
                            source_versions.add(plan["schema_version"])
                            thermal = "thermal" in plan
                            assert thermal == args.thermal
                            prefix = (
                                "stages/thermal"
                                if thermal
                                else "stages/fem-imported"
                                if "imported_fem" in plan
                                else "stages/fem"
                            )
                            artifact = (
                                "thermal-fields.json"
                                if thermal
                                else "imported-fields.json"
                                if "imported_fem" in plan
                                else "fields.json"
                            )
                            directory = state / "artifacts" / job["id"] / prefix
                            native = fem.read_dat(
                                (directory / "reference.dat").read_text()
                            )
                            points = []
                            if thermal:
                                history = json.loads(
                                    (directory / artifact).read_text()
                                )["fields"]["temperature"]
                                assert len(history) == len(native["temperature"])
                                for retained_field, snapshot in zip(
                                    history, native["temperature"], strict=True
                                ):
                                    stamp = retained_field["physical_time_s"]
                                    if stamp in plan["thermal"]["observation_times_s"]:
                                        points.append(("temperature", snapshot, stamp))
                                assert len(points) == len(
                                    plan["thermal"]["observation_times_s"]
                                )
                            else:
                                points = [
                                    (field, snapshots[0], None)
                                    for field, snapshots in native.items()
                                ]
                            sample_operation = "sample-thermal" if thermal else "sample"
                            compare_operation = (
                                "compare-thermal" if thermal else "compare"
                            )
                            sample_tool = (
                                "results_sample_thermal"
                                if thermal
                                else "results_sample"
                            )
                            compare_tool = (
                                "results_compare_thermal"
                                if thermal
                                else "results_compare"
                            )
                            for field, snapshot, physical_time in points:
                                ids = sorted(snapshot["values"])
                                retained = [ids[0], ids[len(ids) // 2], ids[-1]]
                                locations = [
                                    {"association": "node", "node_id": identifier[0]}
                                    if len(identifier) == 1
                                    else {
                                        "association": "integration_point",
                                        "element_id": identifier[0],
                                        "integration_point": identifier[1],
                                    }
                                    for identifier in retained
                                ]
                                request = {
                                    "schema_version": 1,
                                    "job_id": job["id"],
                                    "field": field,
                                    "locations": locations,
                                }
                                if thermal:
                                    request["physical_time_s"] = physical_time
                                observed = cli(sample_operation, request)
                                Draft202012Validator(
                                    schemas[
                                        "ThermalSampleReport"
                                        if thermal
                                        else "SampleReport"
                                    ]
                                ).validate(observed)
                                reply = await client.call_tool(
                                    sample_tool, {"request_spec": request}
                                )
                                assert (
                                    not reply.is_error
                                    and reply.structured_content == observed
                                )
                                expected = [
                                    snapshot["values"][identity]
                                    for identity in retained
                                ]
                                assert [
                                    value["value"] for value in observed["samples"]
                                ] == expected
                                assert [
                                    value["location"] for value in observed["samples"]
                                ] == locations
                                for key, path in (
                                    ("field_artifact", artifact),
                                    ("mesh_artifact", "mesh.json"),
                                    ("native_artifact", "reference.dat"),
                                ):
                                    assert observed[key]["sha256"] == checksum(
                                        directory / path
                                    )
                                assert (
                                    observed["source_samples"] == len(ids)
                                    and observed["physical_time_s"] == physical_time
                                )
                                assert (
                                    observed["coordinate_unit"] == "m"
                                    and observed["physical_validation"] == "unqualified"
                                )
                                comparison = {
                                    "schema_version": 1,
                                    "left": request,
                                    "right": request,
                                }
                                if thermal:
                                    assert observed["native_time_s"] == snapshot["time"]
                                    assert (
                                        abs(snapshot["time"] - physical_time)
                                        <= observed["time_serialization_tolerance_s"]
                                    )
                                delta = cli(compare_operation, comparison)
                                Draft202012Validator(
                                    schemas[
                                        "ThermalCompareReport"
                                        if thermal
                                        else "CompareReport"
                                    ]
                                ).validate(delta)
                                reply = await client.call_tool(
                                    compare_tool, {"request_spec": comparison}
                                )
                                assert (
                                    not reply.is_error
                                    and reply.structured_content == delta
                                )
                                assert delta["maximum_abs_difference"] == 0 and all(
                                    all(v == 0 for v in value["value"])
                                    for value in delta["differences"]
                                )
                                results.append(
                                    {
                                        "request": request,
                                        "sample": observed,
                                        "comparison": delta,
                                        "cli_mcp_parity": True,
                                        "independent_raw_values": expected,
                                    }
                                )
                                for label, changed in (
                                    (
                                        "missing-location",
                                        {
                                            **request,
                                            "locations": [
                                                {
                                                    **locations[0],
                                                    "node_id"
                                                    if len(retained[0]) == 1
                                                    else "element_id": 999999999,
                                                }
                                            ],
                                        },
                                    ),
                                    ("unmapped-time", {**request, "time_s": 1}),
                                    (
                                        "unknown-path",
                                        {**request, "artifact": "../../private.json"},
                                    ),
                                    (
                                        "duplicate-location",
                                        {
                                            **request,
                                            "locations": [locations[0], locations[0]],
                                        },
                                    ),
                                ):
                                    error = cli(sample_operation, changed, False)
                                    reply = await client.call_tool(
                                        sample_tool, {"request_spec": changed}
                                    )
                                    assert reply.is_error
                                    rejections.append(
                                        {
                                            "case": label,
                                            "job": job["id"],
                                            "field": field,
                                            "error": error,
                                            "mcp_rejected": True,
                                        }
                                    )
                                if thermal:
                                    for label, changed in (
                                        (
                                            "unretained-time",
                                            {
                                                **request,
                                                "physical_time_s": physical_time + 1e-8,
                                            },
                                        ),
                                        (
                                            "invented-initial-time",
                                            {**request, "physical_time_s": 0},
                                        ),
                                        (
                                            "unsupported-field",
                                            {**request, "field": "heat_flux"},
                                        ),
                                    ):
                                        error = cli(sample_operation, changed, False)
                                        reply = await client.call_tool(
                                            sample_tool, {"request_spec": changed}
                                        )
                                        assert reply.is_error
                                        rejections.append(
                                            {
                                                "case": label,
                                                "job": job["id"],
                                                "error": error,
                                                "mcp_rejected": True,
                                            }
                                        )
                            if thermal:
                                first, last = points[0], points[-1]
                                comparison = {
                                    "schema_version": 1,
                                    "left": {**request, "physical_time_s": first[2]},
                                    "right": {**request, "physical_time_s": last[2]},
                                }
                                delta = cli(compare_operation, comparison)
                                reply = await client.call_tool(
                                    compare_tool, {"request_spec": comparison}
                                )
                                assert (
                                    not reply.is_error
                                    and reply.structured_content == delta
                                )
                                expected_delta = [
                                    last[1]["values"][identity][0]
                                    - first[1]["values"][identity][0]
                                    for identity in retained
                                ]
                                assert [
                                    value["value"][0] for value in delta["differences"]
                                ] == expected_delta
                                results.append(
                                    {
                                        "comparison": delta,
                                        "independent_signed_difference": expected_delta,
                                        "cli_mcp_parity": True,
                                    }
                                )
                            # Mutation targets are distinct copied bytes/SQLite; original evidence is untouched.
                            target = directory / artifact
                            original = target.read_bytes()
                            document = json.loads(original)
                            field = next(iter(native))
                            document["fields"][field][0]["values"][0]["value"][0] += 1
                            target.write_text(json.dumps(document))
                            request = {
                                "schema_version": 1,
                                "job_id": job["id"],
                                "field": field,
                                "locations": [{"association": "node", "node_id": 1}],
                            }
                            if thermal:
                                request["physical_time_s"] = points[-1][2]
                            error = cli(sample_operation, request, False)
                            path = f"{prefix}/{artifact}"
                            manifest = json.loads(
                                database.execute(
                                    "select manifest from artifacts where job=? and path=?",
                                    (job["id"], path),
                                ).fetchone()[0]
                            )
                            old_manifest = json.dumps(manifest)
                            manifest.update(
                                sha256=checksum(target), bytes=target.stat().st_size
                            )
                            database.execute(
                                "update artifacts set manifest=? where job=? and path=?",
                                (json.dumps(manifest), job["id"], path),
                            )
                            database.commit()
                            bound_error = cli(sample_operation, request, False)
                            assert (
                                "authoritative native output" in bound_error["message"]
                            )
                            target.write_bytes(original)
                            database.execute(
                                "update artifacts set manifest=? where job=? and path=?",
                                (old_manifest, job["id"], path),
                            )
                            database.commit()
                            rejections.extend(
                                [
                                    {
                                        "case": "mutated-registered-bytes",
                                        "job": job["id"],
                                        "error": error,
                                    },
                                    {
                                        "case": "self-consistent-json-substitution",
                                        "job": job["id"],
                                        "error": bound_error,
                                    },
                                ]
                            )
                    return results, rejections

                results, rejections = asyncio.run(check())
                final_counts = database.execute(
                    "select (select count(*) from jobs),(select count(*) from events),(select count(*) from artifacts)"
                ).fetchone()
                assert final_counts == start_counts
                reports.append(
                    {
                        "source": str(source),
                        "source_report_sha256": checksum(source / "verification.json"),
                        "results": results,
                        "rejections": rejections,
                        "database_counts_unchanged": list(final_counts),
                    }
                )
            finally:
                worker.terminate()
                worker.wait(timeout=15)
                database.close()
        assert tree_identity(source) == source_before
    assert source_versions == ({6} if args.thermal else {5, 8})
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "sources": reports,
        "source_versions": sorted(source_versions),
        "service_resources_before": before,
        "service_resources_after": service_resources(),
        "original_sources_unchanged": True,
        "physical_validation": "unqualified",
        "scope": "exact registered v6 thermal samples and explicit retained-time signed comparisons"
        if args.thermal
        else "exact registered static v5/v8 native samples and same-mesh comparisons through CLI/results MCP; no interpolated or transient sampling",
    }
    (root / "verification.json").write_text(
        json.dumps(report, indent=2, allow_nan=False)
    )
    print(
        json.dumps(
            {
                "report_sha256": checksum(root / "verification.json"),
                "fields": sum(len(v["results"]) for v in reports),
                "rejections": sum(len(v["rejections"]) for v in reports),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
