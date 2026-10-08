"""Full-sphere original atmospheric midpoints to native Mitsuba directional lights.

Exact 3.9.1 source: src/emitters/directional.cpp and src/render/scene.cpp at
https://github.com/mitsuba-renderer/mitsuba3/tree/478e193a183c21723f4a8251afc3ad29a8da4c5e
Each original uu*dOmega is a directional irradiance, not an environment-map RGB.
Native transport receipts do not themselves establish original atmosphere runtime
or worker qualification; those are separate registered-source evidence gates.
"""

import csv
import hashlib
import importlib.metadata
import json
import math
import os
import random
import sys
import time
from pathlib import Path

import atmosphere_reference as atmosphere_bridge
import fem_reference as sandbox_foundation
import spectral_reference as spectral_bridge


def cosine(normal, direction):
    return max(0.0, -math.fsum(a * b for a, b in zip(normal, direction, strict=True)))


def normalize_source(atmosphere, receiver, original):
    source = atmosphere_bridge.normalize(atmosphere)
    surface = spectral_bridge.normalize(receiver)
    if (
        receiver["source"]["kind"] != "directional"
        or receiver["occlusion"] != "none"
        or any(
            abs(a - b) > 1e-12
            for a, b in zip(
                receiver["source"]["propagation_direction"],
                source["propagation_direction"],
                strict=True,
            )
        )
        or len(surface["wavelengths_nm"]) != len(source["wavelengths_nm"])
        or any(
            abs(a - b) > 1e-9
            for a, b in zip(
                surface["wavelengths_nm"], source["wavelengths_nm"], strict=True
            )
        )
        or any(
            abs(a - b) > 1e-12 * abs(b)
            for a, b in zip(
                surface["source_values_nm"],
                source["toa_irradiance_w_m2_nm"],
                strict=True,
            )
        )
    ):
        raise ValueError(
            "unchanged original atmospheric TOA inputs and unobstructed explicit directional receiver required"
        )
    observed = atmosphere_bridge.parse_original(original, atmosphere, source)
    radiance = observed["radiance_w_m2_sr_nm"]
    cell = source["angular_cell_solid_angle_sr"]
    emitters = []
    transfer_error = 0.0
    for i, mu in enumerate(source["umu"]):
        for j, phi in enumerate(source["phi_deg"]):
            index = i * len(source["phi_deg"]) + j
            values = [row[index] * cell for row in radiance]
            for original_value, mapped in zip(
                [row[index] for row in radiance], values, strict=True
            ):
                error = (
                    abs(mapped / cell / original_value - 1.0)
                    if original_value
                    else abs(mapped)
                )
                if not math.isfinite(error):
                    raise ValueError(
                        "finite complete original angular transfer required"
                    )
                transfer_error = max(transfer_error, error)
            radius = math.sqrt(1 - mu * mu)
            emitters.append(
                {
                    "id": f"angular-{i:03}-{j:03}",
                    "umu_index": i,
                    "phi_index": j,
                    "umu": mu,
                    "phi_deg": phi,
                    "propagation_direction": [
                        radius * math.sin(math.radians(phi)),
                        radius * math.cos(math.radians(phi)),
                        mu,
                    ],
                    "irradiance_w_m2_nm": values,
                }
            )
    vertical = -source["propagation_direction"][2]
    direct_values = observed["direct_normal_w_m2_nm"]
    for original_value, mapped in zip(
        observed["direct_horizontal_w_m2_nm"], direct_values, strict=True
    ):
        transfer_error = max(
            transfer_error,
            abs(mapped * vertical / original_value - 1.0)
            if original_value
            else abs(mapped),
        )
    if transfer_error > 1e-10:
        raise ValueError(
            "unchanged original source-to-emitter conservation exceeds 1e-10"
        )
    direct = {
        "id": "solar-direct",
        "propagation_direction": source["propagation_direction"],
        "irradiance_w_m2_nm": direct_values,
    }
    direct_incident = [
        v * cosine(receiver["sensor_normal"], source["propagation_direction"])
        for v in direct_values
    ]
    diffuse_incident = [
        math.fsum(
            e["irradiance_w_m2_nm"][k]
            * cosine(receiver["sensor_normal"], e["propagation_direction"])
            for e in emitters
        )
        for k in range(len(source["wavelengths_nm"]))
    ]
    incident = [a + b for a, b in zip(direct_incident, diffuse_incident, strict=True)]
    references = {
        name: {
            channel: spectral_bridge.product_integral(
                source["wavelengths_nm"], values, weights
            )
            for channel, weights in surface["weights"].items()
        }
        for name, values in (
            ("direct", direct_incident),
            ("diffuse", diffuse_incident),
            ("incident", incident),
        )
    }
    return {
        **surface,
        "reference": references["incident"],
        "component_references": references,
        "direct_emitter": direct,
        "diffuse_emitters": emitters,
        "zero_diffuse_emitters": sum(
            not any(e["irradiance_w_m2_nm"]) for e in emitters
        ),
        "angular_shape": [
            len(source["wavelengths_nm"]),
            len(source["umu"]),
            len(source["phi_deg"]),
        ],
        "transfer_relative_conservation_error": transfer_error,
        "source_angular_verification": {
            "maximum_angular_flux_error": observed["maximum_angular_flux_error"],
            "tolerance": observed["tolerance"],
            "passed": True,
            "physical_validation": "unqualified",
        },
    }


def scene_definition(receiver, normalized, component, mi):
    # Reuse the exact planar geometry and sensor API, then replace only its
    # source with native spectral directional plugins at original midpoints.
    definition = spectral_bridge.scene_definition(receiver, normalized, mi)
    del definition["source"]
    emitters = (
        [normalized["direct_emitter"]]
        if component == "direct"
        else normalized["diffuse_emitters"]
    )
    active = []
    maximum = max(max(e["irradiance_w_m2_nm"]) for e in emitters)
    for emitter in emitters:
        if not any(emitter["irradiance_w_m2_nm"]):
            continue
        label = emitter["id"]
        # Source-only importance weights alter the PMF, not illumination.
        definition[label] = {
            "type": "directional",
            "id": label,
            "direction": emitter["propagation_direction"],
            "irradiance": spectral_bridge.native_spectrum(
                normalized["wavelengths_nm"], emitter["irradiance_w_m2_nm"]
            ),
            "sampling_weight": math.fsum(
                v / maximum for v in emitter["irradiance_w_m2_nm"]
            )
            / len(emitter["irradiance_w_m2_nm"]),
        }
        active.append(label)
    return definition, active


def measure(scene, receiver, normalized, seed, path, mi, dr):
    shape = next(shape for shape in scene.shapes() if shape.sensor() is not None)
    rng = random.Random(seed)
    wavelengths = normalized["wavelengths_nm"]
    totals = [0.0] * len(wavelengths)
    corrections = [0.0] * len(wavelengths)
    header = [
        "sample",
        "knot_offset",
        "x_m",
        "y_m",
        "z_m",
        "towards_source_x",
        "towards_source_y",
        "towards_source_z",
        "native_cosine",
        "native_emitter_id",
        "native_pdf",
        *[f"native_weight_w_m2_nm_{i}" for i in range(4)],
    ]
    with path.open("x", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        for sample in range(receiver["samples"]):
            position = shape.sample_position(0.0, [rng.random(), rng.random()])
            random_direction = [rng.random(), rng.random()]
            for offset in range(0, len(wavelengths), 4):
                subset = wavelengths[offset : offset + 4]
                si = mi.SurfaceInteraction3f()
                si.p = position.p
                si.n = position.n
                si.sh_frame = mi.Frame3f(position.n)
                si.wavelengths = mi.Spectrum(subset + [subset[-1]] * (4 - len(subset)))
                direction, weights = scene.sample_emitter_direction(
                    si, random_direction, True
                )
                native_cosine = max(0.0, float(dr.dot(position.n, direction.d)))
                weights = [float(v) for v in weights]
                if (
                    len(weights) != 4
                    or any(
                        not math.isfinite(v)
                        for v in [
                            *weights,
                            *list(direction.d),
                            native_cosine,
                            float(direction.pdf),
                        ]
                    )
                    or any(
                        v < 0.0 for v in [*weights, native_cosine, float(direction.pdf)]
                    )
                ):
                    raise ValueError(
                        "finite original native spectral weights and cosine required"
                    )
                for i, weight in enumerate(weights[: len(subset)]):
                    index = offset + i
                    value = weight * native_cosine
                    adjusted = value - corrections[index]
                    updated = totals[index] + adjusted
                    corrections[index] = (updated - totals[index]) - adjusted
                    totals[index] = updated
                emitter = (
                    direction.emitter.id() if direction.emitter is not None else "none"
                )
                writer.writerow(
                    [
                        sample,
                        offset,
                        *list(position.p),
                        *list(direction.d),
                        native_cosine,
                        emitter,
                        float(direction.pdf),
                        *weights,
                    ]
                )
    means = [value / receiver["samples"] for value in totals]
    return {
        name: spectral_bridge.product_integral(wavelengths, means, weights)
        for name, weights in normalized["weights"].items()
    }


def execute(atmosphere, receiver, original, root):
    normalized = normalize_source(atmosphere, receiver, original)
    spectral_bridge.require_new_work(root, capture_log="atmospheric-transport.log")
    import drjit as dr
    import mitsuba as mi

    if (
        importlib.metadata.version("mitsuba") != "3.9.1"
        or importlib.metadata.version("drjit") != "1.5.0"
    ):
        raise ValueError("exact isolated native spectral ABI required")
    mi.set_variant("scalar_spectral")
    dr.set_thread_count(2)
    mi.set_log_level(mi.LogLevel.Warn)
    if mi.variant() != "scalar_spectral":
        raise ValueError("no native transport backend fallback permitted")
    scenes = {}
    identities = {}
    native_areas = {}
    for component in ("direct", "diffuse"):
        definition, labels = scene_definition(receiver, normalized, component, mi)
        scenes[component] = mi.load_dict(definition)
        shape = next(
            shape for shape in scenes[component].shapes() if shape.sensor() is not None
        )
        native_areas[component] = float(shape.surface_area())
        if (
            not math.isfinite(native_areas[component])
            or abs(native_areas[component] / normalized["sensor_area_m2"] - 1.0) > 5e-6
        ):
            raise ValueError(
                "native Float32 planar surface area must preserve explicit SI dimensions"
            )
        identities[component] = sorted(
            emitter.id() for emitter in scenes[component].emitters()
        )
        if identities[component] != sorted(labels):
            raise ValueError(
                "every nonzero original angular cell must retain its exact native emitter identity"
            )
    source_file = root / "emitter-sources.json"
    source_file.write_text(
        json.dumps(
            {
                "schema_version": 1,
                "direct": normalized["direct_emitter"],
                "diffuse": normalized["diffuse_emitters"],
                "wavelengths_nm": normalized["wavelengths_nm"],
                "native_emitter_ids": identities,
            },
            allow_nan=False,
            indent=2,
        )
    )
    observations = []
    for seed in receiver["seeds"]:
        components = {}
        originals = []
        start = time.monotonic()
        for component, scene in scenes.items():
            path = root / f"{component}-{seed}.csv"
            values = measure(scene, receiver, normalized, seed, path, mi, dr)
            checks = spectral_bridge.verify_channels(
                {
                    **normalized,
                    "reference": normalized["component_references"][component],
                },
                values,
                receiver["relative_tolerance"],
            )
            components[component] = {
                "native_channels_w_m2": values,
                "numerical_verification": checks,
            }
            originals.append(
                {
                    "component": component,
                    "path": path.name,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    "bytes": path.stat().st_size,
                }
            )
        combined = {
            name: math.fsum(
                v["native_channels_w_m2"][name] for v in components.values()
            )
            for name in normalized["weights"]
        }
        checks = spectral_bridge.verify_channels(
            normalized, combined, receiver["relative_tolerance"]
        )
        observations.append(
            {
                "seed": seed,
                "samples": receiver["samples"],
                "components": components,
                "native_channels_w_m2": combined,
                "numerical_verification": checks,
                "exposure_j_m2": {
                    name: value * normalized["history_integral_s"]
                    for name, value in combined.items()
                },
                "native_power_w": {
                    name: value * normalized["sensor_area_m2"]
                    for name, value in combined.items()
                },
                "energy_j": {
                    name: value
                    * normalized["history_integral_s"]
                    * normalized["sensor_area_m2"]
                    for name, value in combined.items()
                },
                "originals": originals,
                "runtime_s": time.monotonic() - start,
            }
        )
    return {
        "schema_version": 1,
        "adapter": "Mitsuba",
        "mitsuba_version": "3.9.1",
        "drjit_version": "1.5.0",
        "backend": "cpu",
        "variant": mi.variant(),
        "precision": "Float32",
        "reduction_precision": "Float64",
        "executed": True,
        "software_fallback": False,
        "source_input": atmosphere,
        "receiver_input": receiver,
        "original_atmosphere_sha256": hashlib.sha256(original.encode()).hexdigest(),
        "derived_emitter_sources": {
            "path": source_file.name,
            "sha256": hashlib.sha256(source_file.read_bytes()).hexdigest(),
            "bytes": source_file.stat().st_size,
        },
        "angular_shape": normalized["angular_shape"],
        "zero_original_angular_cells": normalized["zero_diffuse_emitters"],
        "transfer_relative_conservation_error": normalized[
            "transfer_relative_conservation_error"
        ],
        "source_angular_verification": normalized["source_angular_verification"],
        "reference": normalized["reference"],
        "component_references": normalized["component_references"],
        "sensor_area_m2": normalized["sensor_area_m2"],
        "native_sensor_area_m2": native_areas,
        "history_integral_s": normalized["history_integral_s"],
        "observations": observations,
        "source_runtime_evidence": "requires_independent_registered_originals_qualification",
        "worker_execution": "not_qualified_by_standalone_adapter",
        "convergence": "not_assessed",
        "physical_validation": "unqualified",
        "scope": "native unobstructed planar CPU spectral transfer at every original angular midpoint; separate direct/diffuse transport; no angular interpolation, worker, GPU or physical qualification",
    }


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "reference":
        raise ValueError(
            "usage: harbor-cad-atmospheric-spectral reference REQUEST.json"
        )
    request = Path(sys.argv[2])
    raw = sandbox_foundation.read_regular(request, 65536)
    spec = json.loads(raw, object_pairs_hook=atmosphere_bridge.strict_object)
    if (
        set(spec)
        != {
            "schema_version",
            "atmosphere",
            "receiver",
            "original_path",
            "original_sha256",
        }
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["original_path"] != "/inputs/atmosphere-original.txt"
    ):
        raise ValueError(
            "strict source/receiver descriptor and fixed read-only original atmospheric path required"
        )
    original_path = Path(spec["original_path"])
    original = sandbox_foundation.read_regular(original_path, 32 * 1024**2)
    if hashlib.sha256(original).hexdigest() != spec["original_sha256"]:
        raise ValueError("unchanged complete original atmospheric checksum required")
    root = Path.cwd()
    spectral_bridge.require_new_work(root, capture_log="atmospheric-transport.log")
    sandbox = sandbox_foundation.cpu_sandbox(
        "HARBOR_CAD_ATMOSPHERIC_SPECTRAL_POLICY",
        "harbor-cad-atmospheric-spectral-cpu-v1",
        "/spectral-runtime-closure.txt",
        str(request),
    )
    if sandbox is None or os.statvfs(original_path).f_flag & os.ST_RDONLY == 0:
        raise ValueError(
            "exact operation sandbox and read-only authoritative atmospheric originals required"
        )
    sandbox["checks"]["original_source_readonly"] = True
    report = execute(
        spec["atmosphere"], spec["receiver"], original.decode("utf-8"), root
    )
    report.update(
        request_sha256=hashlib.sha256(raw).hexdigest(),
        sandbox=sandbox,
        sandbox_qualification="measured operation-closure CPU canaries; standalone transport does not qualify registered atmospheric source or worker execution",
    )
    payload = json.dumps(report, allow_nan=False, indent=2).encode()
    temporary = root / ".atmospheric-spectral-receipt.partial"
    with temporary.open("xb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    temporary.rename(root / "atmospheric-spectral-receipt.json")
    print(payload.decode())


if __name__ == "__main__":
    main()
