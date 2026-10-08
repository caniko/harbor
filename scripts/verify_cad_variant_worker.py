"""Exact patched CAD-copy edits, original preservation, reimport and owned lifecycle."""

import argparse
import copy
import json
import math
import subprocess
from pathlib import Path

from atmospheric_transport_prerequisites import immutable
from native_worker_campaign import WorkerCampaign
from verify_native_cpu import verify_manifest
from verify_spectral_cpu import checksum
from verify_systemd import (
    admission_record,
    retention_snapshot,
    wait_admission_release,
    wait_job,
    wait_retention_release,
)


def verify_geometry(spec, regions):
    """Independent named box geometry check; no native-execution claim by itself."""
    scales = {"m": 1.0, "mm": 0.001, "nm": 1e-9}
    request, source = spec["request"], spec["source"]["geometry"]
    dimensions = [q["value"] * scales[q["unit"]] for q in request["dimensions"]]
    tolerance = (
        request["geometry_tolerance"]["value"]
        * scales[request["geometry_tolerance"]["unit"]]
    )
    if (
        len(dimensions) != 3
        or any(not math.isfinite(v) or v <= 0 for v in dimensions)
        or not math.isfinite(tolerance)
        or not 0 < tolerance <= 1e-4
    ):
        raise ValueError(
            "resolved positive dimensions and original geometry tolerance required"
        )
    expected = list(source["bounds_m"])
    for axis in range(3):
        expected[2 * axis + 1] = expected[2 * axis] + dimensions[axis]
    if (
        len(regions["regions"]) != 1
        or regions["synthetic"] != source["synthetic"]
        or regions["gap_healing"]
        or regions["geometry_tolerance"]["value"]
        * scales[regions["geometry_tolerance"]["unit"]]
        != tolerance
    ):
        raise ValueError("complete original-provenance recomputed box required")
    region = regions["regions"][0]
    if (
        region["name"] != request["region_name"]
        or region["transform"] != source["source_transform"]
        or region["source_unit"] != "mm"
        or region["stl_scale_to_m"] != 0.001
        or region["triangles"] <= 0
    ):
        raise ValueError("original named region, placement and triangulation required")
    if any(
        not math.isfinite(value) or abs(value - target) > tolerance
        for value, target in zip(region["bounds_m"], expected, strict=True)
    ) or not math.isclose(
        region["volume_m3"], math.prod(dimensions), rel_tol=1e-10, abs_tol=0.0
    ):
        raise ValueError(
            "recomputed world bounds/volume differ from explicit dimensions"
        )
    return {
        "bounds_m": expected,
        "volume_m3": math.prod(dimensions),
        "placement_preserved": True,
        "complete_named_region": True,
    }


def verify_isolation(bundle):
    isolation = json.loads((bundle / "import-isolation.json").read_text())
    for key in (
        "package_mounts_read_only",
        "plan_read_only",
        "input_read_only",
        "gpu_devices_absent",
        "host_session_environment_absent",
        "no_new_privileges",
    ):
        if isolation[key] is not True:
            raise ValueError("original patched-importer isolation checks required")
    if (
        isolation["policy"] != "harbor-cad-importer-v1"
        or isolation["network_interfaces"] != ["lo"]
        or isolation["net_namespace"] == isolation["host_net_namespace"]
        or int(isolation["effective_capabilities"], 16) != 0
    ):
        raise ValueError(
            "closure-only unprivileged isolated native CAD execution required"
        )
    return isolation


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in (
        "executable",
        "mcp",
        "runtime",
        "fixture-runtime",
        "authority",
        "output",
    ):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    binary, mcp, runtime, fixture_runtime = map(
        immutable, (args.executable, args.mcp, args.runtime, args.fixture_runtime)
    )
    native, generator = (
        json.loads(runtime.read_text()),
        json.loads(fixture_runtime.read_text()),
    )
    if (
        not native.get("cad")
        or not native.get("cad_closure")
        or any(
            native.get(key) is not None
            for key in ("openlb", "render", "video", "filter", "fem", "thermal")
        )
    ):
        raise ValueError("independent patched importer-only runtime required")
    for path in (
        native["cad"],
        native["cad_closure"],
        native["bwrap"],
        generator["fixture"],
        generator["bwrap"],
    ):
        immutable(path)
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    fixtures = root / "fixtures"
    fixtures.mkdir(mode=0o700)
    campaign = WorkerCampaign(binary, mcp, runtime, args.authority, root, timeout=180)
    # Trusted fixed package generator takes no input documents or source code;
    # all subsequent document opens use the worker's closure-only importer.
    fixture_command = [
        generator["bwrap"],
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        "/nix/store",
        "/nix/store",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/home",
        "--dir",
        "/home/native",
        "--bind",
        str(fixtures),
        "/work",
        "--chdir",
        "/work",
        "--clearenv",
        "--setenv",
        "HOME",
        "/home/native",
        "--setenv",
        "LC_ALL",
        "C",
        generator["fixture"],
    ]
    process = subprocess.run(
        fixture_command,
        env=campaign.environment,
        capture_output=True,
        timeout=120,
        check=False,
    )
    (root / "fixture-command.json").write_text(json.dumps(fixture_command, indent=2))
    (root / "fixture-process.log").write_bytes(process.stdout + process.stderr)
    if process.returncode:
        raise RuntimeError(
            "fixed native CAD fixture generation failed; originals retained"
        )
    fixture_manifest = json.loads((fixtures / "fixture-manifest.json").read_text())
    if fixture_manifest["freecad_version"] != "1.1.4" or {
        f["label"] for f in fixture_manifest["fixtures"]
    } != {"origin", "translated"}:
        raise ValueError("exact controlled native box fixtures required")
    results, rejections, sources = [], [], {}

    def offline(*argv):
        process = subprocess.run(
            [str(binary), *map(str, argv)],
            env=campaign.environment,
            capture_output=True,
            timeout=30,
            check=True,
        )
        return json.loads(process.stdout)

    def released(job):
        wait_admission_release(campaign.state, job)
        wait_retention_release(campaign.state, job)

    def export(job, label):
        bundle = root / ("bundle-" + label)
        campaign.command(
            "artifact", "export", "--state", campaign.state, job["id"], bundle
        )
        verify_manifest(bundle)
        return bundle

    with campaign:
        for fixture in fixture_manifest["fixtures"]:
            label, source = fixture["label"], fixtures / fixture["source"]
            if source.is_symlink() or checksum(source) != fixture["sha256"]:
                raise ValueError("unchanged controlled original CAD bytes required")
            case = offline("case", "init")
            case["geometry"] = {
                "source": str(source.relative_to(root)),
                "sha256": fixture["sha256"],
                "synthetic": True,
            }
            case["regions"], case["geometry_tolerance"] = (
                ["solid"],
                {"value": 1e-6, "unit": "m"},
            )
            path = root / ("case-" + label + ".json")
            path.write_text(json.dumps(case))
            plan = offline("cad", "inspect", path)
            if (
                campaign.mcp("cad_plan_inspection", {"case": case}, profile="cad")
                != plan
            ):
                raise ValueError("CLI/MCP original inspection planning drift")
            job = campaign.submit(plan, "source-" + label)
            wait_job(
                str(binary), campaign.endpoint, job["id"], {"succeeded"}, timeout=180
            )
            released(job)
            original_bundle = export(job, "source-" + label)
            verify_isolation(original_bundle)
            if checksum(original_bundle / "input.FCStd") != fixture["sha256"]:
                raise ValueError("original approved document changed")
            sources[label] = job["id"]
            request = {
                "schema_version": 1,
                "source_job": job["id"],
                "region_name": "solid",
                "dimensions": [{"value": v, "unit": "mm"} for v in (30.0, 20.0, 10.0)],
                "geometry_tolerance": {"value": 1e-6, "unit": "m"},
                "provenance": "explicit synthetic native primitive dimensions; preserved original placement",
            }
            path = root / ("request-" + label + ".json")
            path.write_text(json.dumps(request))
            prepared = campaign.command(
                "--socket", campaign.endpoint, "cad", "variant", path
            )["data"]
            if (
                prepared["plan"]["schema_version"] != 16
                or campaign.mcp("cad_plan_variant", {"variant": request}, profile="cad")
                != prepared
            ):
                raise ValueError("CLI/MCP controlled-copy approval drift")
            original = campaign.state / "artifacts" / job["id"] / "input.FCStd"
            data = original.read_bytes()
            original.write_bytes(data + b"pre-submission mutation")
            try:
                for reply in (
                    campaign.command(
                        "--socket",
                        campaign.endpoint,
                        "cad",
                        "variant",
                        path,
                        allow_error=True,
                    ),
                    campaign.submit(
                        prepared, "reject-stale-" + label, allow_error=True
                    ),
                ):
                    if reply["ok"]:
                        raise ValueError("stale original document was accepted")
                    rejections.append(reply["error"])
                campaign.mcp(
                    "cad_plan_variant",
                    {"variant": request},
                    profile="cad",
                    expect_error=True,
                )
            finally:
                original.write_bytes(data)
            changed = copy.deepcopy(prepared)
            changed["plan"]["cad_variant"]["request"]["dimensions"][0]["value"] += 1.0
            reply = campaign.submit(
                changed, "reject-old-approval-" + label, allow_error=True
            )
            if reply["ok"]:
                raise ValueError("changed geometry reused original approval")
            rejections.append(reply["error"])
            variant = (
                campaign.submit(prepared, "variant-" + label)
                if label == "origin"
                else campaign.mcp_submit(
                    prepared, "variant-" + label, profile="cad", tool="cad_submit"
                )
            )
            copy_path = (
                campaign.state / "artifacts" / variant["id"] / "source-document.FCStd"
            )
            if (
                copy_path.read_bytes() != data
                or copy_path.stat().st_ino == original.stat().st_ino
            ):
                raise ValueError(
                    "distinct original document copy required before acknowledgment"
                )
            original.write_bytes(data + b"post-acknowledgment mutation")
            try:
                running = wait_job(
                    str(binary),
                    campaign.endpoint,
                    variant["id"],
                    {"running"},
                    timeout=30,
                )
                retained = retention_snapshot(campaign.state, variant, str(binary))
                reservation = admission_record(campaign.state, variant)
                if reservation is None or reservation["cards"] != {}:
                    raise ValueError("independent shared CPU reservation required")
                campaign.restart()
                if campaign.submit(prepared, "variant-" + label)["id"] != variant["id"]:
                    raise ValueError(
                        "acknowledged native copy duplicated after worker restart"
                    )
                outcome = wait_job(
                    str(binary),
                    campaign.endpoint,
                    variant["id"],
                    {"succeeded"},
                    timeout=180,
                )
                if outcome["invocation_id"] != running["invocation_id"]:
                    raise ValueError(
                        "native CAD service changed invocation across worker crash"
                    )
                released(variant)
            finally:
                original.write_bytes(data)
            bundle = export(variant, "variant-" + label)
            isolation = verify_isolation(bundle)
            regions = json.loads((bundle / "regions.json").read_text())
            verified = verify_geometry(prepared["plan"]["cad_variant"], regions)
            recompute = json.loads((bundle / "cad-variant-recompute.json").read_text())
            if (
                not recompute["source_preserved"]
                or recompute["document"]["sha256"] != checksum(bundle / "variant.FCStd")
                or recompute["approved_variant"] != prepared["plan"]["cad_variant"]
            ):
                raise ValueError("complete original native recompute identity required")
            evidence = campaign.command(
                "--socket", campaign.endpoint, "qualify", "--job", variant["id"]
            )["data"]
            if (
                evidence["capabilities"][0]["formulation"]
                != "source_bound_controlled_box_variant"
                or evidence["capabilities"][0]["runtime_execution"] != "recorded"
                or evidence["capabilities"][0]["numerical_verification"]
                != "not_assessed"
            ):
                raise ValueError(
                    "geometric execution must stay separate from numerical physics"
                )
            # Re-open only the exported copy in a second independent patched
            # import and compare its complete geometry to the first recompute.
            case["geometry"]["source"] = str(
                (
                    campaign.state / "artifacts" / variant["id"] / "variant.FCStd"
                ).relative_to(root)
            )
            case["geometry"]["sha256"] = checksum(bundle / "variant.FCStd")
            path = root / ("reimport-" + label + ".json")
            path.write_text(json.dumps(case))
            reimport = campaign.submit(
                offline("cad", "inspect", path), "reimport-" + label
            )
            wait_job(
                str(binary),
                campaign.endpoint,
                reimport["id"],
                {"succeeded"},
                timeout=180,
            )
            released(reimport)
            roundtrip = export(reimport, "reimport-" + label)
            if json.loads((roundtrip / "regions.json").read_text()) != regions:
                raise ValueError(
                    "saved native variant lost geometry, units or placement on reimport"
                )
            results.append(
                {
                    "source": label,
                    "job": outcome,
                    "approval_digest": prepared["approval_digest"],
                    "independent_geometry": verified,
                    "reimport": reimport,
                    "isolation": isolation,
                    "active_retention": retained,
                    "active_reservation": reservation,
                    "resources": json.loads(
                        (bundle / "service-resources.json").read_text()
                    ),
                    "source_mutation": "pre-submission refused; acknowledged distinct original survived",
                    "worker_restart": "same invocation",
                    "roots_and_reservation_released": True,
                }
            )

        for action in ("cancel", "forced-death"):
            request["source_job"] = sources["translated"]
            path = root / ("request-" + action + ".json")
            path.write_text(json.dumps(request))
            prepared = campaign.command(
                "--socket", campaign.endpoint, "cad", "variant", path
            )["data"]
            job = campaign.submit(prepared, action)
            wait_job(str(binary), campaign.endpoint, job["id"], {"running"}, timeout=30)
            if action == "cancel":
                campaign.command(
                    "--socket", campaign.endpoint, "job", "cancel", job["id"]
                )
            else:
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
                    capture_output=True,
                    timeout=20,
                )
                campaign.start()
            outcome = wait_job(
                str(binary),
                campaign.endpoint,
                job["id"],
                {"cancelled" if action == "cancel" else "failed"},
                timeout=30,
            )
            if campaign.submit(prepared, action)["id"] != job["id"]:
                raise ValueError("terminal native variant was relaunched")
            released(job)
            export(job, action)
            results.append(
                {
                    "action": action,
                    "job": outcome,
                    "roots_and_reservation_released": True,
                    "retry": "same terminal identity",
                }
            )
    report = {
        "schema_version": 1,
        "binary": str(binary),
        "binary_sha256": checksum(binary),
        "mcp": str(mcp),
        "mcp_sha256": checksum(mcp),
        "runtime": str(runtime),
        "runtime_sha256": checksum(runtime),
        "fixture_runtime_sha256": checksum(fixture_runtime),
        "authority_sha256": checksum(args.authority),
        "campaign_sha256": checksum(Path(__file__)),
        "results": results,
        "rejections": rejections,
        "physical_validation": "unqualified",
        "scope": "controlled synthetic native box-copy variants, preserved originals, reimported world geometry, CLI/MCP and complete owned service lifecycle; no physics solve",
    }
    (root / "verification.json").write_text(
        json.dumps(report, allow_nan=False, indent=2)
    )
    print(
        json.dumps(
            {
                "results": len(results),
                "rejections": len(rejections),
                "scope": report["scope"],
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
