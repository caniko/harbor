"""Exact CLI/MCP native sampling/comparison against retained qualified fields."""

import argparse
import asyncio
import hashlib
import importlib.util
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
    parser.add_argument("--moisture", action="store_true")
    parser.add_argument("--projection", action="store_true")
    args = parser.parse_args()
    if args.moisture and not args.thermal:
        parser.error("native moisture screening requires thermal sources")
    if args.projection and not args.thermal:
        parser.error("native temperature projection requires thermal sources")
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
                                )["times"]
                                for retained_field in history:
                                    stamp = retained_field["requested_s"]
                                    snapshot = next(
                                        s
                                        for s in native["temperature"]
                                        if s["time"] == retained_field["observed_s"]
                                    )
                                    assert (
                                        stamp in plan["thermal"]["observation_times_s"]
                                    )
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
                                            "energy-only-time",
                                            {
                                                **request,
                                                "physical_time_s": native[
                                                    "temperature"
                                                ][0]["time"],
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
                            if args.projection:
                                mesh = json.loads((directory / "mesh.json").read_text())
                                spec = plan["thermal"]
                                size, n = spec["size_m"], spec["resolution"]
                                cell_capacitance = (
                                    math.prod(size)
                                    * spec["density_kg_m3"]
                                    * spec["specific_heat_j_kg_k"]
                                    / n**3
                                )
                                weights = {}
                                for node, coordinate in mesh["nodes"].items():
                                    factors = [
                                        0.5
                                        if min(abs(x), abs(x - length))
                                        < spec["geometry_tolerance_m"]
                                        else 1.0
                                        for x, length in zip(
                                            coordinate, size, strict=True
                                        )
                                    ]
                                    weights[int(node)] = cell_capacitance * math.prod(
                                        factors
                                    )
                                total = math.fsum(weights.values())
                                native_values = points[-1][1]["values"]
                                integral = math.fsum(
                                    weights[node] * native_values[node,][0]
                                    for node in weights
                                )
                                mean = integral / total
                                error = max(
                                    abs(native_values[node,][0] - mean)
                                    for node in weights
                                )
                                assert error <= 1.0
                                projection_request = {
                                    "schema_version": 1,
                                    "source_job": job["id"],
                                    "physical_time_s": points[-1][2],
                                    "destination": {
                                        "region": "lower",
                                        "size_m": size,
                                        "origin_m": [0.0, 0.0, 0.0],
                                    },
                                    "maximum_projection_error_k": 1.0,
                                    "maximum_relative_conservation_error": 1e-12,
                                }
                                projected = []
                                for region, origin in (
                                    ("lower", [0.0, 0.0, 0.0]),
                                    ("upper", [0.0, 0.0, size[2] + 0.25e-6]),
                                ):
                                    query = {
                                        **projection_request,
                                        "destination": {
                                            **projection_request["destination"],
                                            "region": region,
                                            "origin_m": origin,
                                        },
                                    }
                                    observed = cli("transfer-temperature", query)
                                    Draft202012Validator(
                                        schemas["ThermalProjectionReport"]
                                    ).validate(observed)
                                    reply = await client.call_tool(
                                        "results_transfer_temperature",
                                        {"request_spec": query},
                                    )
                                    assert (
                                        not reply.is_error
                                        and reply.structured_content == observed
                                    )
                                    assert observed["request"] == query and observed[
                                        "source_nodes"
                                    ] == len(weights)
                                    assert (
                                        abs(
                                            observed["destination_temperature_k"] - mean
                                        )
                                        <= 1e-9
                                    )
                                    assert (
                                        abs(observed["capacitance_j_k"] - total)
                                        <= total * 1e-12
                                    )
                                    assert (
                                        abs(
                                            observed["maximum_abs_projection_error_k"]
                                            - error
                                        )
                                        <= 1e-9
                                    )
                                    assert (
                                        abs(
                                            observed["receipt"]["source_integral"]
                                            - integral
                                        )
                                        <= integral * 1e-12
                                    )
                                    assert (
                                        observed["receipt"][
                                            "relative_conservation_error"
                                        ]
                                        <= query["maximum_relative_conservation_error"]
                                    )
                                    assert (
                                        observed["source"]["native_time_s"]
                                        == points[-1][1]["time"]
                                        and observed["source"]["physical_time_s"]
                                        == query["physical_time_s"]
                                    )
                                    assert observed["source"]["native_artifact"][
                                        "sha256"
                                    ] == checksum(directory / "reference.dat")
                                    projected.append(observed)
                                    results.append(
                                        {
                                            "projection": observed,
                                            "independent_capacitance_j_k": total,
                                            "independent_temperature_integral_j": integral,
                                            "independent_weighted_temperature_k": mean,
                                            "independent_projection_error_k": error,
                                            "cli_mcp_parity": True,
                                        }
                                    )
                                assert (
                                    projected[0]["projection_id"]
                                    != projected[1]["projection_id"]
                                )
                                assert (
                                    projected[0]["destination_mesh_sha256"]
                                    != projected[1]["destination_mesh_sha256"]
                                )
                                for label, changed in (
                                    (
                                        "caller-temperature",
                                        {**projection_request, "temperature_k": 293.15},
                                    ),
                                    (
                                        "caller-material",
                                        {**projection_request, "density_kg_m3": 1000.0},
                                    ),
                                    (
                                        "scaled-destination",
                                        {
                                            **projection_request,
                                            "destination": {
                                                **projection_request["destination"],
                                                "size_m": [size[0] * 2.0, *size[1:]],
                                            },
                                        },
                                    ),
                                    (
                                        "unresolved-translation",
                                        {
                                            **projection_request,
                                            "destination": {
                                                **projection_request["destination"],
                                                "origin_m": [1e15, 0.0, 0.0],
                                            },
                                        },
                                    ),
                                    (
                                        "unretained-transfer-time",
                                        {
                                            **projection_request,
                                            "physical_time_s": points[-1][2] + 1e-8,
                                        },
                                    ),
                                    (
                                        "weakened-conservation",
                                        {
                                            **projection_request,
                                            "maximum_relative_conservation_error": 0.01,
                                        },
                                    ),
                                    (
                                        "weakened-projection",
                                        {
                                            **projection_request,
                                            "maximum_projection_error_k": 10.0,
                                        },
                                    ),
                                    (
                                        "unapproved-loss",
                                        {
                                            **projection_request,
                                            "maximum_projection_error_k": error / 2.0
                                            if error > 1e-9
                                            else -1.0,
                                        },
                                    ),
                                ):
                                    error_reply = cli(
                                        "transfer-temperature", changed, False
                                    )
                                    reply = await client.call_tool(
                                        "results_transfer_temperature",
                                        {"request_spec": changed},
                                    )
                                    assert reply.is_error
                                    rejections.append(
                                        {
                                            "case": label,
                                            "job": job["id"],
                                            "error": error_reply,
                                            "mcp_rejected": True,
                                        }
                                    )
                            if args.moisture:
                                mesh = json.loads((directory / "mesh.json").read_text())
                                for surface, nodes in mesh[
                                    "boundary_node_sets"
                                ].items():
                                    minimum = min(
                                        points[-1][1]["values"][(node,)][0]
                                        for node in nodes
                                    )
                                    for risk in (
                                        {
                                            "assessment": "missing",
                                            "reason": "humidity unavailable in this controlled native reference",
                                        },
                                        {
                                            "assessment": "inapplicable",
                                            "justification": "explicit dry synthetic enclosure reference; no water exposure",
                                        },
                                        {
                                            "assessment": "dew_point_screening",
                                            "air_temperature": {
                                                "value": 20.0,
                                                "unit": "degC",
                                            },
                                            "relative_humidity": 0.5,
                                            "provenance": "controlled synthetic air reference; not measured prototype input",
                                        },
                                    ):
                                        moisture_request = {
                                            "schema_version": 1,
                                            "job_id": job["id"],
                                            "physical_time_s": points[-1][2],
                                            "surface_region": surface,
                                            "moisture_risk": risk,
                                        }
                                        observed = cli("moisture", moisture_request)
                                        Draft202012Validator(
                                            schemas["NativeMoistureReport"]
                                        ).validate(observed)
                                        reply = await client.call_tool(
                                            "results_moisture",
                                            {"request_spec": moisture_request},
                                        )
                                        assert (
                                            not reply.is_error
                                            and reply.structured_content == observed
                                        )
                                        assert observed["source"]["samples"][0][
                                            "value"
                                        ] == [minimum]
                                        node = observed["source"]["samples"][0][
                                            "location"
                                        ]["node_id"]
                                        assert node in nodes and points[-1][1][
                                            "values"
                                        ][(node,)] == [minimum]
                                        assert observed["surface_nodes"] == len(nodes)
                                        status = {
                                            "missing": "missing_inputs",
                                            "inapplicable": "inapplicable",
                                            "dew_point_screening": "unsupported_screening"
                                            if minimum < 273.15
                                            else "screening",
                                        }[risk["assessment"]]
                                        assert (
                                            observed["moisture_risk"]["status"]
                                            == status
                                        )
                                        assert (
                                            observed["physical_validation"]
                                            == "unqualified"
                                        )
                                        results.append(
                                            {
                                                "moisture": observed,
                                                "independent_surface_minimum_k": minimum,
                                                "cli_mcp_parity": True,
                                            }
                                        )
                                for label, changed in (
                                    (
                                        "caller-surface-temperature",
                                        {
                                            **moisture_request,
                                            "minimum_surface_temperature_k": 293.15,
                                        },
                                    ),
                                    (
                                        "ordinal-face",
                                        {**moisture_request, "surface_region": "face1"},
                                    ),
                                    (
                                        "missing-air",
                                        {
                                            **moisture_request,
                                            "moisture_risk": {
                                                **risk,
                                                "relative_humidity": None,
                                            },
                                        },
                                    ),
                                    (
                                        "zero-humidity",
                                        {
                                            **moisture_request,
                                            "moisture_risk": {
                                                **risk,
                                                "relative_humidity": 0,
                                            },
                                        },
                                    ),
                                    (
                                        "unsupported-air-domain",
                                        {
                                            **moisture_request,
                                            "moisture_risk": {
                                                **risk,
                                                "air_temperature": {
                                                    "value": -10.0,
                                                    "unit": "degC",
                                                },
                                            },
                                        },
                                    ),
                                ):
                                    error = cli("moisture", changed, False)
                                    reply = await client.call_tool(
                                        "results_moisture", {"request_spec": changed}
                                    )
                                    assert reply.is_error
                                    rejections.append(
                                        {
                                            "case": label,
                                            "error": error,
                                            "mcp_rejected": True,
                                        }
                                    )
                            # Mutation targets are distinct copied bytes/SQLite; original evidence is untouched.
                            target = directory / artifact
                            original = target.read_bytes()
                            document = json.loads(original)
                            field = next(iter(native))
                            if thermal:
                                node = next(iter(document["times"][0]["temperature_k"]))
                                document["times"][0]["temperature_k"][node] += 1
                            else:
                                document["fields"][field][0]["values"][0]["value"][
                                    0
                                ] += 1
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
                            if args.projection:
                                projection_error = cli(
                                    "transfer-temperature", projection_request, False
                                )
                                rejections.append(
                                    {
                                        "case": "mutated-transfer-source",
                                        "error": projection_error,
                                    }
                                )
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
                            if args.projection:
                                projection_error = cli(
                                    "transfer-temperature", projection_request, False
                                )
                                assert (
                                    "authoritative native output"
                                    in projection_error["message"]
                                )
                                reply = await client.call_tool(
                                    "results_transfer_temperature",
                                    {"request_spec": projection_request},
                                )
                                assert reply.is_error
                                rejections.append(
                                    {
                                        "case": "self-consistent-transfer-json-substitution",
                                        "error": projection_error,
                                        "mcp_rejected": True,
                                    }
                                )
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
        "native_moisture_screening": args.moisture,
        "native_temperature_projection": args.projection,
        "scope": "exact registered v6 thermal samples and explicit retained-time signed comparisons; native planar surface moisture branches enabled when requested"
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
