"""Pinned native spectral CPU reference: strict inputs and independent integrals."""

import csv
import hashlib
import importlib.metadata
import importlib.util
import json
import math
import os
import random
import sys
import time
from itertools import pairwise
from pathlib import Path

KEYS = {
    "schema_version",
    "synthetic",
    "backend",
    "variant",
    "precision",
    "wavelengths",
    "source",
    "source_provenance",
    "sensor_width",
    "sensor_height",
    "sensor_normal",
    "occlusion",
    "absorptivity",
    "optical_provenance",
    "ageing_action",
    "ageing_provenance",
    "history",
    "history_interpolation",
    "history_provenance",
    "samples",
    "seeds",
    "relative_tolerance",
}


def number(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("explicit finite numeric input required")
    return float(value)


def quantity(value, units):
    if (
        not isinstance(value, dict)
        or set(value) != {"value", "unit"}
        or value["unit"] not in units
    ):
        raise ValueError("exact explicit quantity and supported unit required")
    result = number(value["value"]) * units[value["unit"]]
    if not math.isfinite(result):
        raise ValueError("finite normalized quantity required")
    return result


def unit_vector(values):
    if not isinstance(values, list) or len(values) != 3:
        raise ValueError("explicit three-dimensional unit direction required")
    values = [number(v) for v in values]
    if abs(math.fsum(v * v for v in values) - 1.0) > 1e-12:
        raise ValueError("exact bounded unit direction required")
    return values


def product_integral(wavelengths, source, weight):
    # Simpson quadrature is exact for the product of two linear functions;
    # independent from the Rust coefficient-polynomial reduction.
    return math.fsum(
        (x1 - x0) / 6 * (a0 * b0 + (a0 + a1) * (b0 + b1) + a1 * b1)
        for x0, x1, a0, a1, b0, b1 in zip(
            wavelengths, wavelengths[1:], source, source[1:], weight, weight[1:]
        )
    )


def normalize(spec):
    if isinstance(spec, dict) and "incident" in spec:
        return normalize_reflection(spec)
    if (
        not isinstance(spec, dict)
        or set(spec) != KEYS
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["backend"] != "cpu"
        or spec["variant"] != "scalar_spectral"
        or spec["precision"] != "Float32"
    ):
        raise ValueError("strict synthetic scalar Float32 spectral CPU input required")
    wavelengths = [
        quantity(v, {"m": 1e9, "mm": 1e6, "nm": 1.0}) for v in spec["wavelengths"]
    ]
    n = len(wavelengths)
    if (
        not 2 <= n <= 64
        or any(not 200 <= v <= 2500 for v in wavelengths)
        or any(b - a < 0.001 for a, b in pairwise(wavelengths))
    ):
        raise ValueError(
            "ordered resolved full spectral range with no extrapolation required"
        )
    for name in (
        "source_provenance",
        "optical_provenance",
        "ageing_provenance",
        "history_provenance",
    ):
        if (
            not isinstance(spec[name], str)
            or not spec[name].strip()
            or len(spec[name]) > 4096
        ):
            raise ValueError("explicit bounded spectral provenance required")
    weights = {"incident": [1.0] * n}
    for field, name in (("absorptivity", "absorbed"), ("ageing_action", "ageing")):
        weights[name] = [number(v) for v in spec[field]]
        if len(weights[name]) != n or any(not 0 <= v <= 1 for v in weights[name]):
            raise ValueError(
                "complete bounded dimensionless UV optical weights required"
            )
    size = [
        quantity(spec[k], {"m": 1.0, "mm": 0.001, "nm": 1e-9})
        for k in ("sensor_width", "sensor_height")
    ]
    if any(not 1e-5 <= v <= 0.01 for v in size):
        raise ValueError("bounded explicit planar sensor dimensions required")
    normal = unit_vector(spec["sensor_normal"])
    source = spec["source"]
    if spec["occlusion"] not in ("none", "full_directional_occluder") or not isinstance(
        source, dict
    ):
        raise ValueError("allowlisted source angular model and occlusion required")
    if source.get("kind") == "directional":
        if set(source) != {"kind", "propagation_direction", "irradiance"}:
            raise ValueError(
                "directional source must preserve explicit propagation direction"
            )
        direction = unit_vector(source["propagation_direction"])
        angular = (
            0.0
            if spec["occlusion"] == "full_directional_occluder"
            else max(
                0.0, -math.fsum(a * b for a, b in zip(direction, normal, strict=True))
            )
        )
        source_values = [
            quantity(v, {"W/(m2*m)": 1e-9, "W/(m2*nm)": 1.0})
            for v in source["irradiance"]
        ]
        source_unit = "W/(m2*nm)"
    elif source.get("kind") == "isotropic":
        if set(source) != {"kind", "radiance"} or spec["occlusion"] != "none":
            raise ValueError(
                "complete isotropic radiance cannot use directional occlusion"
            )
        source_values = [
            quantity(v, {"W/(m2*sr*m)": 1e-9, "W/(m2*sr*nm)": 1.0})
            for v in source["radiance"]
        ]
        source_unit = "W/(m2*sr*nm)"
        angular = math.pi
    else:
        raise ValueError(
            "explicit directional or isotropic source required; no atmospheric inference"
        )
    if (
        len(source_values) != n
        or any(not 0 <= v <= 1e6 for v in source_values)
        or not any(source_values)
    ):
        raise ValueError("complete finite nonnegative source spectral density required")
    times, scales = [], []
    if (
        not isinstance(spec["history"], list)
        or not 2 <= len(spec["history"]) <= 32
        or spec["history_interpolation"] != "piecewise_linear_prescribed_scale"
    ):
        raise ValueError(
            "complete declared fixed-angular source amplitude history required"
        )
    for entry in spec["history"]:
        if not isinstance(entry, dict) or set(entry) != {"time", "scale"}:
            raise ValueError("strict prescribed radiant amplitude point required")
        times.append(quantity(entry["time"], {"s": 1.0, "min": 60.0, "h": 3600.0}))
        scales.append(number(entry["scale"]))
    if (
        times[0] != 0.0
        or any(b <= a for a, b in pairwise(times))
        or times[-1] > 86400 * 365
        or any(not 0 <= v <= 1e6 for v in scales)
    ):
        raise ValueError(
            "complete bounded nonnegative radiant amplitude from t=0 required"
        )
    seeds = spec["seeds"]
    samples = spec["samples"]
    tolerance = number(spec["relative_tolerance"])
    if (
        type(samples) is not int
        or not 1024 <= samples <= 65536
        or samples & (samples - 1)
        or not isinstance(seeds, list)
        or len(seeds) != 3
        or any(type(v) is not int or not 0 <= v <= 2**32 - 1 for v in seeds)
        or not seeds[0] < seeds[1] < seeds[2]
        or not 0 < tolerance <= 0.02
    ):
        raise ValueError(
            "bounded samples, distinct ordered native seeds and unchanged acceptance required"
        )
    base = {
        name: product_integral(wavelengths, source_values, weight)
        for name, weight in weights.items()
    }
    reference = {name: angular * value for name, value in base.items()}
    history_integral = math.fsum(
        (b - a) * (u + v) / 2.0
        for a, b, u, v in zip(times, times[1:], scales, scales[1:])
    )
    if any(
        not math.isfinite(v) or v < 0.0
        for v in [
            *base.values(),
            *reference.values(),
            history_integral,
            *[v * history_integral for v in reference.values()],
        ]
    ):
        raise ValueError("finite positive band and exposure integration required")
    return {
        "wavelengths_nm": wavelengths,
        "source_values_nm": source_values,
        "source_unit": source_unit,
        "sensor_size_m": size,
        "sensor_area_m2": size[0] * size[1],
        "weights": weights,
        "reference": reference,
        "base_integrals": base,
        "angular_factor": angular,
        "history_times_s": times,
        "history_scales": scales,
        "history_integral_s": history_integral,
        "relative_tolerance": tolerance,
    }


def normalize_reflection(spec):
    keys = {
        "schema_version",
        "formulation",
        "incident",
        "disk_radius",
        "sensor_height",
        "reflectance",
        "reflectance_provenance",
        "geometry_provenance",
        "maximum_model_error",
    }
    if (
        set(spec) != keys
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["formulation"] != "isotropic_lambertian_disk"
    ):
        raise ValueError("strict separate synthetic planar reflection input required")
    incident = spec["incident"]
    if not isinstance(incident, dict) or "incident" in incident:
        raise ValueError("unwrapped explicit angular UV incident spectrum required")
    normalized = normalize(incident)
    if (
        incident["source"]["kind"] != "isotropic"
        or incident["sensor_normal"] != [0, 0, -1]
        or incident["occlusion"] != "none"
    ):
        raise ValueError(
            "isotropic source and downward black sensor required for disk view-factor reference"
        )
    radius = quantity(spec["disk_radius"], {"m": 1.0, "mm": 0.001, "nm": 1e-9})
    height = quantity(spec["sensor_height"], {"m": 1.0, "mm": 0.001, "nm": 1e-9})
    reflectance = number(spec["reflectance"])
    limit = number(spec["maximum_model_error"])
    if (
        not 0.1 <= radius <= 100.0
        or not 0.1 <= height <= 10.0
        or not 0 <= reflectance <= 1.0
        or not 0 < limit <= min(1e-5, normalized["relative_tolerance"] / 8.0)
    ):
        raise ValueError(
            "bounded positive reflection geometry, UV reflectance and unchanged independent model gate required"
        )
    for name in ("reflectance_provenance", "geometry_provenance"):
        if (
            not isinstance(spec[name], str)
            or not spec[name].strip()
            or len(spec[name]) > 4096
        ):
            raise ValueError(
                "explicit bounded UV reflection and geometry provenance required"
            )
    offset = math.hypot(*normalized["sensor_size_m"]) / 2.0
    if offset >= radius:
        raise ValueError(
            "complete centred sensor footprint inside circular disk required"
        )

    # Integrate 2*cos(theta)*sin(theta) over the cone subtended by the disk.
    # At every surface position, inscribed/circumscribed cones bound the offset.
    def projected_cone(r):
        return 1.0 - height**2 / (height**2 + r**2)

    view = projected_cone(radius)
    bounds = [projected_cone(radius - offset), projected_cone(radius + offset)]
    factor = (1.0 - view) + reflectance * view
    footprint = (1.0 - reflectance) * max(view - bounds[0], bounds[1] - view) / factor
    shadow = (
        reflectance
        * bounds[1]
        * normalized["sensor_area_m2"]
        / (math.pi * height**2 * factor)
    )
    error = footprint + shadow
    if not math.isfinite(error) or error > limit:
        raise ValueError(
            "finite footprint and sensor shadow exceed the unchanged independent reflection model gate"
        )
    normalized["reference"] = {
        name: value * factor for name, value in normalized["reference"].items()
    }
    normalized["reflection"] = {
        "disk_radius_m": radius,
        "sensor_height_m": height,
        "reflectance": reflectance,
        "disk_view_factor": view,
        "reflection_factor": factor,
        "sensor_view_factor_bounds": bounds,
        "black_sensor_shadow_relative_error_bound": shadow,
        "model_relative_error_bound": error,
        "maximum_model_error": limit,
    }
    return normalized


def verify_channels(normalized, values, tolerance):
    if (
        set(values) != {"incident", "absorbed", "ageing"}
        or tolerance != normalized["relative_tolerance"]
    ):
        raise ValueError(
            "exact native spectral channel coverage and unchanged approved gate required"
        )
    checks = {}
    for name, reference in normalized["reference"].items():
        value = number(values[name])
        if value < 0.0:
            raise ValueError("nonnegative native irradiance channel required")
        if reference == 0.0:
            if value != 0.0:
                raise ValueError(
                    "zero optical response or completely occluded/back-facing reference must retain zero native channel"
                )
            error = 0.0
        else:
            error = abs(value / reference - 1.0)
        checks[name] = {
            "native_w_m2": value,
            "reference_w_m2": reference,
            "relative_error": error,
            "tolerance": tolerance,
            "passed": error <= tolerance,
        }
    if not all(v["passed"] for v in checks.values()):
        raise ValueError(
            f"native spectral irradiance exceeded unchanged analytical gate: {checks}"
        )
    return {
        "channels": checks,
        "maximum_normalized_error": max(v["relative_error"] for v in checks.values()),
        "tolerance": tolerance,
        "passed": True,
        "physical_validation": "unqualified",
    }


def native_spectrum(wavelengths, values):
    # The pinned irregular plugin builds a sampling distribution and rejects an
    # identically zero spectrum. UniformSpectrum evaluates exact zero without
    # requiring positive probability mass; retain the explicit original band.
    if all(value == 0.0 for value in values):
        return {
            "type": "uniform",
            "value": 0.0,
            "wavelength_min": wavelengths[0],
            "wavelength_max": wavelengths[-1],
        }
    return {
        "type": "irregular",
        "wavelengths": ", ".join(map(str, wavelengths)),
        "values": ", ".join(map(str, values)),
    }


def scene_definition(spec, normalized, mi):
    normal = mi.ScalarVector3f(spec["sensor_normal"])
    # The shape owns the frame and physical area; sensor transforms are forbidden.
    frame = mi.ScalarTransform4f().look_at(
        origin=[0, 0, 0],
        target=normal,
        up=[0, 1, 0] if abs(spec["sensor_normal"][1]) < 0.9 else [1, 0, 0],
    )
    width, height = normalized["sensor_size_m"]
    frame = frame.scale([width / 2, height / 2, 1])
    film = {
        "type": "specfilm",
        "width": 1,
        "height": 1,
        "component_format": "float32",
        "rfilter": {"type": "box"},
    }
    for name, weights in normalized["weights"].items():
        film[name] = native_spectrum(normalized["wavelengths_nm"], weights)
    scene = {
        "type": "scene",
        "integrator": {"type": "path", "max_depth": 3, "rr_depth": 5},
        "meter": {
            "type": "rectangle",
            "to_world": frame,
            "bsdf": {"type": "diffuse", "reflectance": 0.0},
            "sensor": {
                "type": "irradiancemeter",
                "film": film,
                "sampler": {"type": "independent", "sample_count": spec["samples"]},
            },
        },
    }
    source = spec["source"]
    if source["kind"] == "isotropic":
        scene["source"] = {
            "type": "constant",
            "radiance": native_spectrum(
                normalized["wavelengths_nm"], normalized["source_values_nm"]
            ),
        }
        if "reflection" in normalized:
            reflection = normalized["reflection"]
            scene["disk"] = {
                "type": "disk",
                "to_world": mi.ScalarTransform4f()
                .translate([0, 0, -reflection["sensor_height_m"]])
                .scale(reflection["disk_radius_m"]),
                "bsdf": {
                    "type": "diffuse",
                    "reflectance": native_spectrum(
                        normalized["wavelengths_nm"],
                        [reflection["reflectance"]] * len(normalized["wavelengths_nm"]),
                    ),
                },
            }
    else:
        scene["source"] = {
            "type": "directional",
            "direction": source["propagation_direction"],
            "irradiance": native_spectrum(
                normalized["wavelengths_nm"], normalized["source_values_nm"]
            ),
        }
        if spec["occlusion"] == "full_directional_occluder":
            toward_source = [-v for v in source["propagation_direction"]]
            # Opaque 20 cm square is perpendicular to propagation, 10 cm
            # upstream of the bounded sensor. Every collimated ray is covered.
            transform = (
                mi.ScalarTransform4f()
                .look_at(
                    origin=[v * 0.1 for v in toward_source],
                    target=[0, 0, 0],
                    up=[0, 1, 0] if abs(toward_source[1]) < 0.9 else [1, 0, 0],
                )
                .scale(0.1)
            )
            scene["screen"] = {
                "type": "rectangle",
                "to_world": transform,
                "bsdf": {"type": "diffuse", "reflectance": 0.0},
            }
    return scene


def measure_directional(scene, spec, normalized, seed, path, mi, dr):
    shape = next(shape for shape in scene.shapes() if shape.sensor() is not None)
    rng = random.Random(seed)
    wavelengths = normalized["wavelengths_nm"]
    totals = [0.0] * len(wavelengths)
    corrections = [0.0] * len(wavelengths)
    header = [
        "sample",
        "x_m",
        "y_m",
        "z_m",
        "native_cosine",
        *[f"emitter_weight_w_m2_nm_{i}" for i in range(len(wavelengths))],
    ]
    with path.open("x", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(header)
        for sample in range(spec["samples"]):
            position = shape.sample_position(0.0, [rng.random(), rng.random()])
            values = []
            original_weights = []
            for offset in range(0, len(wavelengths), 4):
                subset = wavelengths[offset : offset + 4]
                si = mi.SurfaceInteraction3f()
                si.p = position.p
                si.n = position.n
                si.sh_frame = mi.Frame3f(position.n)
                si.wavelengths = mi.Spectrum(subset + [subset[-1]] * (4 - len(subset)))
                direction, weight = scene.sample_emitter_direction(
                    si, [rng.random(), rng.random()], True
                )
                cosine = max(0.0, float(dr.dot(position.n, direction.d)))
                original_weights.extend([float(v) for v in weight][: len(subset)])
                values.extend([float(v) * cosine for v in weight][: len(subset)])
            if any(not math.isfinite(v) or v < 0 for v in values):
                raise ValueError(
                    "finite nonnegative native directional spectral weights required"
                )
            for index, value in enumerate(values):
                # Streaming compensated Float64 reduction preserves agreement
                # with independent fsum reconstruction at the maximum budget.
                adjusted = value - corrections[index]
                updated = totals[index] + adjusted
                corrections[index] = (updated - totals[index]) - adjusted
                totals[index] = updated
            writer.writerow([sample, *list(position.p), cosine, *original_weights])
    means = [value / spec["samples"] for value in totals]
    # Native direction/intersection/visibility evaluates every original knot;
    # exact polynomial quadrature preserves the declared linear spectrum.
    values = {
        name: product_integral(wavelengths, means, weights)
        for name, weights in normalized["weights"].items()
    }
    return values


def bitmap_channels(bitmap, mi):
    names = [field.name for field in bitmap.struct_()]
    if (
        list(bitmap.size()) != [1, 1]
        or len(names) != 3
        or set(names) != {"incident", "absorbed", "ageing"}
        or bitmap.component_format() != mi.Struct.Type.Float32
    ):
        raise ValueError(
            "original one-pixel Float32 spectral channels required; no RGB reinterpretation"
        )
    return dict(
        zip(names, [number(float(v)) for v in mi.TensorXf(bitmap).array], strict=True)
    )


def verify_exr_roundtrip(values, bitmap, mi):
    copied = bitmap_channels(bitmap, mi)
    # OpenEXR stores channels in name order. Compare each original Float32
    # value by its scientific channel name, without reordering its meaning or
    # adding numerical tolerance to the lossless roundtrip.
    if copied != values:
        raise ValueError("exact native Float32 EXR spectral channel roundtrip required")
    return copied


def execute(spec, root):
    normalized = normalize(spec)
    incident = spec["incident"] if "reflection" in normalized else spec
    # Native imports are isolated from MCP and never run during pure validation.
    import drjit as dr
    import mitsuba as mi

    if (
        importlib.metadata.version("mitsuba") != "3.9.1"
        or importlib.metadata.version("drjit") != "1.5.0"
    ):
        raise ValueError("exact compatible native Mitsuba/DrJit ABI required")
    mi.set_variant("scalar_spectral")
    dr.set_thread_count(2)
    mi.set_log_level(mi.LogLevel.Warn)
    if mi.variant() != "scalar_spectral":
        raise ValueError(
            "explicit native scalar spectral variant required; no backend fallback"
        )
    scene = mi.load_dict(scene_definition(incident, normalized, mi))
    observations = []
    for seed in incident["seeds"]:
        start = time.monotonic()
        if incident["source"]["kind"] == "directional":
            path = root / f"directional-{seed}.csv"
            values = measure_directional(
                scene, incident, normalized, seed, path, mi, dr
            )
            method = "native_emitter_direction_visibility_and_surface_position; exact_original_knot_product_quadrature"
        else:
            image = mi.render(scene, seed=seed, spp=incident["samples"])
            bitmap = scene.sensors()[0].film().bitmap()
            names = [field.name for field in bitmap.struct_()]
            values = dict(zip(names, [float(v) for v in image.array], strict=True))
            path = root / f"irradiance-{seed}.exr"
            bitmap.write(str(path))
            reopened = mi.Bitmap(str(path))
            verify_exr_roundtrip(values, reopened, mi)
            method = "native_irradiancemeter_cosine_hemisphere_path_and_specfilm_response_sampling"
        checks = verify_channels(normalized, values, incident["relative_tolerance"])
        observations.append(
            {
                "seed": seed,
                "samples": incident["samples"],
                "method": method,
                "native_channels_w_m2": values,
                "numerical_verification": checks,
                "path": path.name,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "bytes": path.stat().st_size,
                "runtime_s": time.monotonic() - start,
                "exposure_j_m2": {
                    name: value * normalized["history_integral_s"]
                    for name, value in values.items()
                },
            }
        )
    return {
        "schema_version": 1,
        "adapter": "Mitsuba",
        "mitsuba_version": "3.9.1",
        "drjit_version": "1.5.0",
        "variant": mi.variant(),
        "backend": "cpu",
        "precision": "Float32",
        "reduction_precision": "Float64",
        "executed": True,
        "software_fallback": False,
        "input": spec,
        "normalized": normalized,
        "observations": observations,
        "physical_validation": "unqualified",
        "reflection_model_assessment": normalized.get("reflection"),
        "scope": "synthetic planar directional/isotropic UV surface irradiance and optional bounded isotropic Lambertian disk; prescribed fixed-angular dose; no atmosphere, GPU or worker qualification",
    }


def require_new_work(root):
    # The worker opens its stage capture before spawning the native adapter.
    # Only that regular log may already exist; scientific observations/receipts
    # are create-new and a failed diagnostic tree can never be reused.
    entries = list(root.iterdir())
    if any(
        path.name != "spectral.log" or path.is_symlink() or not path.is_file()
        for path in entries
    ):
        raise ValueError(
            "new bounded native reference work directory with at most the owned capture log required"
        )


def main():
    if len(sys.argv) != 3 or sys.argv[1] not in ("reference", "inspect-exr"):
        raise ValueError(
            "usage: spectral_reference.py reference REQUEST.json | inspect-exr ORIGINAL.exr"
        )
    request = Path(sys.argv[2])
    if (
        request.is_symlink()
        or not request.is_file()
        or request.stat().st_size > 1024 * 1024
    ):
        raise ValueError("bounded closed regular spectral request required")
    if sys.argv[1] == "inspect-exr":
        import mitsuba as mi

        if (
            importlib.metadata.version("mitsuba") != "3.9.1"
            or importlib.metadata.version("drjit") != "1.5.0"
        ):
            raise ValueError("exact compatible native EXR reader required")
        mi.set_variant("scalar_spectral")
        bitmap = mi.Bitmap(str(request))
        print(json.dumps(bitmap_channels(bitmap, mi), allow_nan=False))
        return
    raw = request.read_bytes()
    spec = json.loads(raw)
    root = Path.cwd()
    # A failed attempt owns its diagnostic tree and is never overwritten.
    require_new_work(root)
    sandbox = None
    if os.environ.get("HARBOR_CAD_SPECTRAL_POLICY"):
        bridge = importlib.util.spec_from_file_location(
            "spectral_sandbox_foundation", "@fem_bridge@"
        )
        fem = importlib.util.module_from_spec(bridge)
        bridge.loader.exec_module(fem)
        sandbox = fem.cpu_sandbox(
            "HARBOR_CAD_SPECTRAL_POLICY",
            "harbor-cad-spectral-cpu-v1",
            "/spectral-runtime-closure.txt",
            str(request),
        )
    report = execute(spec, root)
    report["request_sha256"] = hashlib.sha256(raw).hexdigest()
    report["sandbox"] = sandbox
    report["sandbox_qualification"] = (
        "measured operation-closure CPU canaries; production worker remains unqualified"
        if sandbox
        else "unqualified; standalone reference does not authorize a production worker"
    )
    payload = json.dumps(report, allow_nan=False, indent=2).encode()
    temporary = root / ".spectral-receipt.partial"
    with temporary.open("xb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    temporary.rename(root / "spectral-receipt.json")
    print(payload.decode())


if __name__ == "__main__":
    main()
