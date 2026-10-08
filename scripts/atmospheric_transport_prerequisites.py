"""Exact-package prerequisites for registered atmospheric transport qualification."""

import json
import math
import sys
from pathlib import Path

from verify_atmosphere_cpu import assess_refinements
from verify_atmospheric_spectral_cpu import reference_cases, verify_observation
from verify_spectral_cpu import checksum


def immutable(path):
    value = Path(path).resolve(strict=True)
    if not value.is_relative_to("/nix/store") or not value.is_file():
        raise ValueError("exact immutable production package required")
    return value


def original_files(root, case, required):
    work = root / case["case"]
    if work.is_symlink() or not work.is_dir():
        raise ValueError("closed prerequisite case directory required")
    if not required <= case["original_files_sha256"].keys():
        raise ValueError("complete prerequisite original identities required")
    for filename, identity in case["original_files_sha256"].items():
        path = work / filename
        if (
            Path(filename).name != filename
            or path.is_symlink()
            or not path.is_file()
            or checksum(path) != identity
        ):
            raise ValueError("unchanged prerequisite original bytes required")


def sandbox(receipt, policy, *, original_source=False):
    checks = {
        "operation_closure_only",
        "no_gpu_nodes",
        "no_sysfs",
        "no_host_home",
        "no_session_bus",
        "no_worker_socket",
        "network_namespace_isolated",
        "descriptor_readonly",
    }
    if original_source:
        checks.add("original_source_readonly")
    observed = receipt["sandbox"]
    if (
        observed["policy"] != policy
        or set(observed["checks"]) != checks
        or not all(value is True for value in observed["checks"].values())
    ):
        raise ValueError("complete operation-specific measured sandbox checks required")


def prerequisites(
    binary, source_runtime, transport_runtime, source_root, transport_root
):
    repo = Path(__file__).resolve().parents[1]
    sys.path.insert(0, str(repo / "adapters"))
    import atmospheric_spectral as verifier

    source_file, transport_file = (
        Path(root).resolve(strict=True) / "verification.json"
        for root in (source_root, transport_root)
    )
    source_report, transport_report = (
        json.loads(path.read_text()) for path in (source_file, transport_file)
    )
    # Refuse development/source-overlay reports before selecting any source job.
    if (
        not source_report.get("runtime_qualification", "").startswith(
            "exact immutable standalone"
        )
        or transport_report.get("package_qualification")
        != "exact immutable renderer and operation sandbox"
    ):
        raise ValueError("both complete exact-package native prerequisites required")
    source_descriptor, transport_descriptor = (
        json.loads(immutable(path).read_text())
        for path in (source_runtime, transport_runtime)
    )
    for report, worker, adapter, closure in (
        (source_report, source_descriptor, "atmosphere", "atmosphere_closure"),
        (
            transport_report,
            transport_descriptor,
            "atmospheric_spectral",
            "atmospheric_spectral_closure",
        ),
    ):
        standalone = immutable(report["runtime"])
        descriptor = json.loads(standalone.read_text())
        original_closure = (
            "spectral_closure" if adapter == "atmospheric_spectral" else closure
        )
        if (
            checksum(standalone) != report["runtime_sha256"]
            or worker[adapter] != descriptor[adapter]
            or worker[closure] != descriptor[original_closure]
            or worker["bwrap"] != descriptor["bwrap"]
        ):
            raise ValueError(
                "matching exact native adapter, ABI closure and launcher required"
            )
    if (
        source_report["binary"] != str(binary)
        or source_report["binary_sha256"] != checksum(binary)
        or source_report["source_sha256"] != verifier.atmosphere_bridge.SOURCE_SHA256
        or source_report["adapter_source_sha256"]
        != checksum(repo / "adapters/atmosphere_reference.py")
        or transport_report["adapter_source_sha256"]
        != checksum(repo / "adapters/atmospheric_spectral.py")
    ):
        raise ValueError(
            "matching production planner and unchanged official bridge sources required"
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
    sources = {case["case"]: case for case in source_report["results"]}
    if (
        set(sources) != required
        or len(source_report["results"]) != 12
        or len(source_report["rejections"]) != 8
    ):
        raise ValueError(
            "complete atmospheric source and rejection case coverage required"
        )
    for case in sources.values():
        original_files(
            source_file.parent, case, {"uvspec-original.txt", "atmosphere-receipt.json"}
        )
        prepared = verifier.atmosphere_bridge.normalize(case["request"])
        work = source_file.parent / case["case"]
        original = work / "uvspec-original.txt"
        request = source_file.parent / f"input-{case['case']}" / "request.json"
        receipt = json.loads((work / "atmosphere-receipt.json").read_text())
        sandbox(receipt, verifier.atmosphere_bridge.POLICY)
        if (
            request.is_symlink()
            or json.loads(request.read_text()) != case["request"]
            or receipt["input"] != case["request"]
            or receipt["request_sha256"] != checksum(request)
            or receipt["source_sha256"] != verifier.atmosphere_bridge.SOURCE_SHA256
            or receipt["profile_sha256"]
            != verifier.atmosphere_bridge.PROFILE_SHA256[case["request"]["profile"]]
            or receipt["native_executable_sha256"]
            != checksum(immutable(receipt["native_executable"]))
            or receipt["executed"] is not True
            or receipt["software_fallback"] is not False
            or receipt["adapter"] != "libRadtran"
            or receipt["version"] != "2.0.6"
            or receipt["backend"] != "cpu"
            or receipt["solver"] != "disort"
            or receipt["precision"] != "Float32"
            or receipt["reduction_precision"] != "Float64"
            or receipt["prepared"] != prepared
            or receipt["observations"] != case["observations"]
            or receipt["original"]
            != {
                "path": original.name,
                "bytes": original.stat().st_size,
                "sha256": checksum(original),
            }
        ):
            raise ValueError(
                "unchanged exact source execution, prescription and original receipt required"
            )
        observed = verifier.atmosphere_bridge.parse_original(
            (source_file.parent / case["case"] / "uvspec-original.txt").read_text(),
            case["request"],
            prepared,
        )
        if case["prepared"] != prepared or case["observations"] != {
            key: value
            for key, value in observed.items()
            if key != "radiance_w_m2_sr_nm"
        }:
            raise ValueError(
                "independent complete original atmospheric reconstruction required"
            )
    refinements = assess_refinements(
        sources,
        source_file.parent,
        sources["clear-streams64"]["request"]["relative_tolerance"],
    )
    if source_report["refinements"] != refinements or not all(
        v["passed"] for v in refinements.values()
    ):
        raise ValueError("three unchanged independent source refinement gates required")
    transports = {case["case"]: case for case in transport_report["results"]}
    expected = list(reference_cases(repo))
    if len(transports) != len(expected) or len(transport_report["results"]) != len(
        expected
    ):
        raise ValueError(
            "complete nine-case native atmospheric transport prerequisite required"
        )
    for name, atmosphere, receiver, original in expected:
        case = transports.get(name)
        if case is None or case["exit_code"] != 0:
            raise ValueError("every atmospheric native transport case must succeed")
        original_files(
            transport_file.parent,
            case,
            {
                "atmospheric-spectral-receipt.json",
                "emitter-sources.json",
                *(
                    f"{component}-{seed}.csv"
                    for seed in receiver["seeds"]
                    for component in ("direct", "diffuse")
                ),
            },
        )
        receipt = case["receipt"]
        original_path = (
            transport_file.parent / f"inputs-{name}" / "atmosphere-original.txt"
        )
        original_request = original_path.parent / "request.json"
        if (
            original_path.is_symlink()
            or original_path.read_text() != original
            or receipt["source_input"] != atmosphere
            or receipt["receiver_input"] != receiver
            or receipt["original_atmosphere_sha256"] != checksum(original_path)
            or original_request.is_symlink()
            or receipt["request_sha256"] != checksum(original_request)
            or json.loads(original_request.read_text()) != case["request"]
            or json.loads(
                (
                    transport_file.parent / name / "atmospheric-spectral-receipt.json"
                ).read_text()
            )
            != receipt
            or receipt["executed"] is not True
            or receipt["software_fallback"] is not False
            or receipt["adapter"] != "Mitsuba"
            or receipt["mitsuba_version"] != "3.9.1"
            or receipt["drjit_version"] != "1.5.0"
            or receipt["backend"] != "cpu"
            or receipt["variant"] != "scalar_spectral"
            or receipt["precision"] != "Float32"
            or receipt["reduction_precision"] != "Float64"
            or [v["seed"] for v in receipt["observations"]] != receiver["seeds"]
        ):
            raise ValueError(
                "unchanged original inputs, ordered seeds and exact sandbox prerequisite required"
            )
        normalized = verifier.normalize_source(atmosphere, receiver, original)
        sandbox(receipt, "harbor-cad-atmospheric-spectral-cpu-v1", original_source=True)
        derived = receipt["derived_emitter_sources"]
        mapping = transport_file.parent / name / "emitter-sources.json"
        mapped = json.loads(mapping.read_text())
        error = receipt["transfer_relative_conservation_error"]
        if (
            derived
            != {
                "path": mapping.name,
                "bytes": mapping.stat().st_size,
                "sha256": checksum(mapping),
            }
            or mapped["direct"] != normalized["direct_emitter"]
            or mapped["diffuse"] != normalized["diffuse_emitters"]
            or mapped["wavelengths_nm"] != normalized["wavelengths_nm"]
            or receipt["reference"] != normalized["reference"]
            or receipt["component_references"] != normalized["component_references"]
            or not math.isfinite(error)
            or not 0 <= error <= 1e-10
            or set(receipt["native_sensor_area_m2"]) != {"direct", "diffuse"}
            or any(
                not math.isfinite(area)
                or abs(area / normalized["sensor_area_m2"] - 1) > 5e-6
                for area in receipt["native_sensor_area_m2"].values()
            )
        ):
            raise ValueError(
                "unchanged complete original emitter mapping, quadrature and area required"
            )
        for observation in receipt["observations"]:
            verify_observation(
                transport_file.parent / name,
                observation,
                receiver,
                normalized,
                verifier,
            )
    return (
        sources["clear-streams64"],
        verifier,
        {
            "source": str(source_file),
            "source_sha256": checksum(source_file),
            "transport": str(transport_file),
            "transport_sha256": checksum(transport_file),
            "source_refinements": refinements,
        },
    )
