"""Analytical plane-wall transient references and prescribed SI histories.

PDE: rho*c*dT/dt=k*d2T/dx2+P(t)/V. Robin conditions apply at both
x faces; transverse faces are adiabatic. Constant properties only. This
reference verifies a native FEM solve, not component boot reliability.
"""

import hashlib
import importlib.util
import math
import subprocess
import sys
from functools import lru_cache
from itertools import pairwise
from pathlib import Path


def finite(value):
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
    ):
        raise ValueError("finite SI numeric input required")
    return float(value)


def validate(spec):
    keys = {
        "schema_version",
        "synthetic",
        "backend",
        "formulation",
        "size_m",
        "resolution",
        "geometry_tolerance_m",
        "initial_temperature_k",
        "density_kg_m3",
        "specific_heat_j_kg_k",
        "conductivity_w_m_k",
        "material_temperature_domain_k",
        "convection_w_m2_k",
        "duration_s",
        "max_step_s",
        "integration_substeps",
        "observation_times_s",
        "ambient_history",
        "heater_history",
        "numerical_tolerance",
        "energy_tolerance",
        "geometry_provenance",
        "material_provenance",
        "history_provenance",
        "convection_provenance",
        "moisture_risk",
    }
    if (
        set(spec) != keys
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["backend"] != "cpu"
        or spec["formulation"] != "plane_wall_robin"
    ):
        raise ValueError(
            "exact synthetic constant-property CPU thermal recipe required"
        )
    lengths = [finite(v) for v in spec["size_m"]]
    if (
        len(lengths) != 3
        or min(lengths) <= 0
        or max(lengths) / min(lengths) > 1000
        or not 1e-10 <= finite(spec["geometry_tolerance_m"]) < 0.001 * min(lengths)
    ):
        raise ValueError("resolved positive three-dimensional synthetic box required")
    if type(spec["resolution"]) is not int or not 2 <= spec["resolution"] <= 32:
        raise ValueError("bounded independent spatial refinement required")
    for name in (
        "density_kg_m3",
        "specific_heat_j_kg_k",
        "conductivity_w_m_k",
        "initial_temperature_k",
        "duration_s",
        "max_step_s",
    ):
        if finite(spec[name]) <= 0:
            raise ValueError("positive SI thermal properties/time required")
    duration = spec["duration_s"]
    if duration / spec["max_step_s"] > 1024 or spec["max_step_s"] > duration:
        raise ValueError("bounded explicitly approved temporal refinement required")
    if (
        type(spec["integration_substeps"]) is not int
        or not 1 <= spec["integration_substeps"] <= 64
    ):
        raise ValueError(
            "explicit bounded solver substeps independent of energy output required"
        )
    for key in ("numerical_tolerance", "energy_tolerance"):
        if not 0 < finite(spec[key]) <= 0.02:
            raise ValueError(
                "unchanged bounded transient numerical/energy gate required"
            )
    for key in (
        "geometry_provenance",
        "material_provenance",
        "history_provenance",
        "convection_provenance",
    ):
        if (
            not isinstance(spec[key], str)
            or not spec[key].strip()
            or len(spec[key]) > 4096
        ):
            raise ValueError(
                "explicit bounded geometry/material/history/convection provenance required"
            )
    for key in ("heater_history", "ambient_history"):
        history = spec[key]
        if not isinstance(history, list) or not 2 <= len(history) <= 64:
            raise ValueError("bounded piecewise-linear prescribed history required")
        previous = -1.0
        for pair in history:
            if not isinstance(pair, list) or len(pair) != 2:
                raise ValueError("explicit SI time/value pairs required")
            stamp, value = map(finite, pair)
            if (
                stamp <= previous
                or stamp < 0
                or value < 0
                or (key == "ambient_history" and value == 0)
            ):
                raise ValueError(
                    "ordered times and positive absolute ambient/nonnegative heater inputs required"
                )
            previous = stamp
        if history[0][0] != 0 or history[-1][0] != duration:
            raise ValueError("full histories required; extrapolation forbidden")
    h = finite(spec["convection_w_m2_k"])
    biot = h * lengths[0] / (2 * spec["conductivity_w_m_k"])
    if h < 0 or (h > 0 and not 1e-5 <= biot <= 1000):
        raise ValueError(
            "prescribed bounded convection; velocity alone cannot supply h"
        )
    times = spec["observation_times_s"]
    if (
        not isinstance(times, list)
        or not 1 <= len(times) <= 64
        or times[-1] != duration
    ):
        raise ValueError("bounded explicit retained times through final state required")
    previous = 0.0
    diffusivity = spec["conductivity_w_m_k"] / (
        spec["density_kg_m3"] * spec["specific_heat_j_kg_k"]
    )
    if not math.isfinite(diffusivity) or diffusivity <= 0:
        raise ValueError("finite positive thermal diffusivity required")
    for stamp in map(finite, times):
        if (
            stamp <= previous
            or stamp > duration
            or (h > 0 and diffusivity * stamp / (lengths[0] / 2) ** 2 < 1e-4)
        ):
            raise ValueError(
                "ordered positive resolved physical observation times required"
            )
        previous = stamp
    domain = [finite(v) for v in spec["material_temperature_domain_k"]]
    if len(domain) != 2 or not 0 < domain[0] < domain[1]:
        raise ValueError("explicit valid constant-property temperature domain required")
    capacity = spec["density_kg_m3"] * spec["specific_heat_j_kg_k"] * math.prod(lengths)
    if not math.isfinite(capacity) or capacity <= 0:
        raise ValueError("finite positive thermal capacity required")
    outputs = output_times(spec)
    if len(outputs) * (spec["resolution"] + 1) ** 3 * 48 > 64 * 1024**2 or any(
        right - left < 1e-6 * duration
        for left, right in zip([0.0, *outputs[:-1]], outputs, strict=True)
    ):
        raise ValueError(
            "bounded native field output and distinct printed physical times required"
        )
    low = min(spec["initial_temperature_k"], *(v for _, v in spec["ambient_history"]))
    high = (
        max(spec["initial_temperature_k"], *(v for _, v in spec["ambient_history"]))
        + history_energy(spec["heater_history"], duration) / capacity
    )
    if not math.isfinite(high) or low < domain[0] or high > domain[1]:
        raise ValueError(
            "full thermal maximum-principle bound must stay inside material data; no extrapolation"
        )
    if history_energy(spec["heater_history"], duration) == 0 and (
        h == 0
        or all(v == spec["initial_temperature_k"] for _, v in spec["ambient_history"])
    ):
        raise ValueError("nontrivial explicit transient forcing required")
    moisture = spec["moisture_risk"]
    if not isinstance(moisture, dict) or moisture.get("assessment") not in {
        "missing",
        "inapplicable",
    }:
        raise ValueError(
            "explicit missing moisture inputs or justified inapplicability required"
        )
    key = "reason" if moisture["assessment"] == "missing" else "justification"
    if (
        set(moisture) != {"assessment", key}
        or not isinstance(moisture[key], str)
        or not moisture[key].strip()
        or len(moisture[key]) > 4096
    ):
        raise ValueError(
            "bounded moisture assessment required; no condensate or boot inference"
        )


def history_value(history, stamp):
    for (left, a), (right, b) in pairwise(history):
        if left <= stamp <= right:
            return a + (b - a) * ((stamp - left) / (right - left))
    raise ValueError("history extrapolation forbidden")


def history_energy(history, stamp):
    history_value(history, stamp)
    total = 0.0
    for (left, a), (right, b) in pairwise(history):
        end = min(stamp, right)
        if end > left:
            total += (end - left) * (a + history_value(history, end)) / 2.0
    return total


@lru_cache(maxsize=16)
def eigenvalues(biot, count=256):
    if not math.isfinite(biot) or not 1e-5 <= biot <= 1000 or not 1 <= count <= 256:
        raise ValueError("bounded positive plane-wall Biot number required")
    values = []
    for index in range(count):
        left, right = index * math.pi, (index + 0.5) * math.pi
        for _ in range(64):
            midpoint = (left + right) / 2.0
            if midpoint * math.tan(midpoint) > biot:
                right = midpoint
            else:
                left = midpoint
        values.append((left + right) / 2.0)
    return tuple(values)


def decay_integral(rate, value, slope, duration):
    """Integral_0^d (value+slope*u)*exp[-rate*(d-u)] du, stable at small d."""
    z = rate * duration
    constant = -math.expm1(-z) / rate
    if abs(z) < 1e-4:
        ramp = duration**2 * (0.5 - z / 6 + z**2 / 24 - z**3 / 120)
    else:
        ramp = (duration - constant) / rate
    return value * constant + slope * ramp


def plane_wall_temperature(
    x, stamp, half_length, diffusivity, capacity, h_over_k, initial, ambient, power
):
    if h_over_k == 0:
        return initial + history_energy(power, stamp) / capacity
    fourier = diffusivity * stamp / half_length**2
    if fourier < 1e-4 or abs(x) > half_length:
        raise ValueError(
            "bounded spatial coordinate and resolved modal-series time required"
        )
    breaks = sorted(
        {
            0.0,
            stamp,
            *(t for t, _ in ambient if 0 < t < stamp),
            *(t for t, _ in power if 0 < t < stamp),
        }
    )
    observed = history_value(ambient, stamp)
    for root in eigenvalues(h_over_k * half_length):
        rate = diffusivity * (root / half_length) ** 2
        coefficient = 4 * math.sin(root) / (2 * root + math.sin(2 * root))
        state = (initial - history_value(ambient, 0.0)) * math.exp(-rate * stamp)
        for left, right in pairwise(breaks):
            duration = right - left
            ambient_rate = (
                history_value(ambient, right) - history_value(ambient, left)
            ) / duration
            a = history_value(power, left) / capacity - ambient_rate
            slope = (history_value(power, right) - history_value(power, left)) / (
                capacity * duration
            )
            state += math.exp(-rate * (stamp - right)) * decay_integral(
                rate, a, slope, duration
            )
        observed += coefficient * math.cos(root * x / half_length) * state
    if not math.isfinite(observed):
        raise ValueError("analytical reference overflow")
    return observed


# CalculiX 2.23 ccx.tex *DFLUX/*FILM hexahedron local face-node table.
# https://www.dhondt.de/ccx_2.23.doc.tar.bz2
FACES = [
    (0, 1, 2, 3),
    (4, 7, 6, 5),
    (0, 4, 5, 1),
    (1, 5, 6, 2),
    (2, 6, 7, 3),
    (3, 7, 4, 0),
]


def plane_wall_faces(spec, nodes, cells):
    sides = {0.0: [], spec["size_m"][0]: []}
    for tag, cell in cells.items():
        for face, local in enumerate(FACES, 1):
            ids = [cell[index] for index in local]
            for boundary, faces in sides.items():
                if all(
                    abs(nodes[n][0] - boundary) <= spec["geometry_tolerance_m"]
                    for n in ids
                ):
                    faces.append((tag, face, ids))
    if any(len(faces) != spec["resolution"] ** 2 for faces in sides.values()):
        raise ValueError(
            "unambiguous complete semantic x-normal convection surfaces required"
        )
    return [face for faces in sides.values() for face in faces]


def output_times(spec):
    count = math.ceil(spec["duration_s"] / spec["max_step_s"])
    return sorted(
        {
            *(spec["duration_s"] * i / count for i in range(1, count + 1)),
            *spec["observation_times_s"],
            *(
                t
                for history in (spec["ambient_history"], spec["heater_history"])
                for t, _ in history
                if t > 0
            ),
        }
    )


def integration_step(spec):
    return spec["max_step_s"] / spec["integration_substeps"]


def heat_transfer_time_card(spec):
    # heattransfers.f reads each numeric textpart using a fixed (f20.0)
    # field. Full .17g text can truncate an exponent outside those 20 columns.
    step = integration_step(spec)
    fields = [f"{v:.13e}" for v in (step, spec["duration_s"], step * 1e-4, step)]
    if any(len(v) > 20 for v in fields):
        raise ValueError("thermal time values exceed the native CCX numeric card width")
    return ",".join(fields)


def deck(spec, fem, nodes, cells, sets):
    lines = fem.mesh_deck(nodes, cells, sets)
    lines += [
        "*MATERIAL,NAME=SOLID",
        "*DENSITY",
        f"{spec['density_kg_m3']:.17g}",
        "*SPECIFIC HEAT",
        f"{spec['specific_heat_j_kg_k']:.17g}",
        "*CONDUCTIVITY",
        f"{spec['conductivity_w_m_k']:.17g}",
        "*SOLID SECTION,ELSET=EALL,MATERIAL=SOLID",
        "*INITIAL CONDITIONS,TYPE=TEMPERATURE",
        f"NALL,{spec['initial_temperature_k']:.17g}",
    ]
    for name, history in [
        ("AMBIENT", spec["ambient_history"]),
        ("HEATER", spec["heater_history"]),
    ]:
        lines.append(f"*AMPLITUDE,NAME={name},TIME=TOTAL TIME")
        lines += [f"{t:.17g},{v:.17g}" for t, v in history]
    lines += ["*TIME POINTS,NAME=OUTPUT,TIME=TOTAL TIME"]
    times = output_times(spec)
    lines += [
        ",".join(f"{t:.17g}" for t in times[i : i + 8]) for i in range(0, len(times), 8)
    ]
    lines += [
        "*STEP,INC=131072",
        "*HEAT TRANSFER,SOLVER=SPOOLES",
        heat_transfer_time_card(spec),
        "*DFLUX,AMPLITUDE=HEATER",
        f"EALL,BF,{1.0 / math.prod(spec['size_m']):.17g}",
    ]
    if spec["convection_w_m2_k"]:
        lines += ["*FILM,AMPLITUDE=AMBIENT"]
        lines += [
            f"{tag},F{face},1.,{spec['convection_w_m2_k']:.17g}"
            for tag, face, _ in plane_wall_faces(spec, nodes, cells)
        ]
    lines += [
        "*NODE PRINT,NSET=NALL,TIME POINTS=OUTPUT,GLOBAL=YES",
        "NT",
        "*NODE FILE,TIME POINTS=OUTPUT",
        "NT",
        "*EL FILE,TIME POINTS=OUTPUT",
        "HFL",
        "*END STEP",
    ]
    return "\n".join(lines) + "\n"


def verify(spec, nodes, cells, fields):
    snapshots = fields.get("temperature", [])
    requested = output_times(spec)
    if set(fields) != {"temperature"} or len(snapshots) != len(requested):
        raise ValueError("complete bounded native thermal history required")
    capacity = (
        spec["density_kg_m3"] * spec["specific_heat_j_kg_k"] * math.prod(spec["size_m"])
    )
    diffusivity = spec["conductivity_w_m_k"] / (
        spec["density_kg_m3"] * spec["specific_heat_j_kg_k"]
    )
    volumes = {
        tag: math.prod(
            max(nodes[n][c] for n in cell) - min(nodes[n][c] for n in cell)
            for c in range(3)
        )
        for tag, cell in cells.items()
    }
    if not math.isclose(
        sum(volumes.values()), math.prod(spec["size_m"]), rel_tol=1e-10
    ):
        raise ValueError("thermal mesh-volume integration does not preserve the solid")
    surfaces = plane_wall_faces(spec, nodes, cells)
    areas = {
        (tag, face): math.prod(
            max(nodes[n][c] for n in ids) - min(nodes[n][c] for n in ids)
            for c in (1, 2)
        )
        for tag, face, ids in surfaces
    }
    if not math.isclose(
        sum(areas.values()), 2 * math.prod(spec["size_m"][1:]), rel_tol=1e-10
    ):
        raise ValueError("thermal convection surfaces omit or duplicate area")
    scale = (
        0.0
        if spec["convection_w_m2_k"] == 0
        else max(
            abs(spec["initial_temperature_k"] - v) for _, v in spec["ambient_history"]
        )
    )
    scale = max(
        scale, history_energy(spec["heater_history"], spec["duration_s"]) / capacity
    )
    if scale == 0:
        raise ValueError(
            "nontrivial prescribed transient required for normalized reference"
        )
    loss_rate = (
        spec["convection_w_m2_k"]
        * sum(areas.values())
        * (spec["initial_temperature_k"] - spec["ambient_history"][0][1])
    )
    last_time, integrated_loss, maximum_error, maximum_balance = 0.0, 0.0, 0.0, 0.0
    rows, retained = [], []
    for expected, snapshot in zip(requested, snapshots, strict=True):
        stamp = snapshot["time"]
        if (
            not math.isclose(
                stamp, expected, rel_tol=1e-7, abs_tol=1e-7 * spec["duration_s"]
            )
            or stamp <= last_time
            or set(snapshot["values"]) != {(tag,) for tag in nodes}
        ):
            raise ValueError("native times/IDs changed, collapsed or omitted")
        values = {tag[0]: numbers[0] for tag, numbers in snapshot["values"].items()}
        if any(
            not spec["material_temperature_domain_k"][0]
            <= t
            <= spec["material_temperature_domain_k"][1]
            for t in values.values()
        ):
            raise ValueError("native temperature leaves declared material range")
        ambient = history_value(spec["ambient_history"], expected)
        next_rate = spec["convection_w_m2_k"] * sum(
            areas[tag, face] * (sum(values[n] for n in ids) / 4.0 - ambient)
            for tag, face, ids in surfaces
        )
        integrated_loss += (expected - last_time) * (loss_rate + next_rate) / 2.0
        heater = history_energy(spec["heater_history"], expected)
        stored = (
            spec["density_kg_m3"]
            * spec["specific_heat_j_kg_k"]
            * sum(
                volumes[tag]
                * (sum(values[n] for n in cell) / 8.0 - spec["initial_temperature_k"])
                for tag, cell in cells.items()
            )
        )
        balance = abs(stored + integrated_loss - heater) / max(
            abs(stored), abs(integrated_loss), heater, 1e-30
        )
        maximum_balance = max(maximum_balance, balance)
        rows.append(
            {
                "time_s": expected,
                "mean_temperature_k": spec["initial_temperature_k"] + stored / capacity,
                "stored_energy_j": stored,
                "prescribed_heater_energy_j": heater,
                "outward_convection_energy_j": integrated_loss,
                "relative_balance_error": balance,
            }
        )
        if expected in spec["observation_times_s"]:
            error = (
                max(
                    abs(
                        values[tag]
                        - plane_wall_temperature(
                            xyz[0] - spec["size_m"][0] / 2.0,
                            expected,
                            spec["size_m"][0] / 2.0,
                            diffusivity,
                            capacity,
                            spec["convection_w_m2_k"] / spec["conductivity_w_m_k"],
                            spec["initial_temperature_k"],
                            spec["ambient_history"],
                            spec["heater_history"],
                        )
                    )
                    for tag, xyz in nodes.items()
                )
                / scale
            )
            maximum_error = max(maximum_error, error)
            retained.append(
                {"requested_s": expected, "observed_s": stamp, "temperature_k": values}
            )
        last_time, loss_rate = expected, next_rate
    if (
        maximum_error > spec["numerical_tolerance"]
        or maximum_balance > spec["energy_tolerance"]
    ):
        raise ValueError(
            f"unchanged thermal reference/energy gate failed: {maximum_error}, {maximum_balance}"
        )
    return (
        {
            "temperature": {
                "reference": "256-mode Robin plane-wall series; exact convolution of prescribed piecewise-linear ambient/heater",
                "normalized_max_abs_error": maximum_error,
                "tolerance": spec["numerical_tolerance"],
                "unit": "K",
                "samples": len(nodes) * len(spec["observation_times_s"]),
                "passed": True,
            },
            "energy": {
                "reference": "volume-integrated rho*c*(T-T0) = prescribed heater minus trapezoid-integrated outward Robin flux",
                "maximum_relative_balance_error": maximum_balance,
                "tolerance": spec["energy_tolerance"],
                "unit": "J",
                "samples": len(requested),
                "passed": True,
            },
        },
        rows,
        retained,
    )


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "run":
        raise ValueError("usage: harbor-cad-thermal run request.json")
    source = importlib.util.spec_from_file_location("harbor_cad_fem", "@fem_bridge@")
    fem = importlib.util.module_from_spec(source)
    source.loader.exec_module(fem)
    raw = fem.read_regular(sys.argv[2], 1024**2)
    spec = fem.strict_json(raw)
    validate(spec)
    sandbox = fem.cpu_sandbox(
        "HARBOR_CAD_THERMAL_POLICY",
        "harbor-cad-thermal-cpu-v1",
        "/thermal-runtime-closure.txt",
        sys.argv[2],
    )
    if any(Path.cwd().glob("reference.*")) or any(Path.cwd().glob("mesh.json*")):
        raise ValueError("new stage-local thermal directory required")
    nodes, cells, sets = fem.mesh(spec)
    Path("reference.inp").write_text(deck(spec, fem, nodes, cells, sets))
    environment = {
        "OMP_NUM_THREADS": "1",
        "CCX_NPROC_RESULTS": "1",
        "CCX_NPROC_EQUATION_SOLVER": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "HOME": "/nonexistent",
        "LC_ALL": "C",
    }
    with Path("calculix.log").open("xb") as log:
        solver = subprocess.run(
            ["@calculix@", "-i", "reference"],
            env=environment,
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=180,
            check=False,
        )
    if (
        solver.returncode
        or "Job finished" not in fem.read_regular("calculix.log", 32 * 1024**2).decode()
    ):
        raise ValueError("native CalculiX transient execution did not finish")
    data = fem.read_regular("reference.dat", 64 * 1024**2)
    checks, metrics, retained = verify(spec, nodes, cells, fem.read_dat(data.decode()))
    fem.atomic_json(
        "thermal-fields.json",
        {
            "schema_version": 1,
            "field": "temperature",
            "unit": "K",
            "association": "point",
            "coordinate_unit": "m",
            "initial_condition": {
                "time_s": 0.0,
                "temperature_k": spec["initial_temperature_k"],
                "provenance": "prescribed uniform initial condition, not a solved snapshot",
            },
            "times": retained,
        },
    )
    fem.atomic_json(
        "thermal-metrics.json",
        {
            "schema_version": 1,
            "metrics": metrics,
            "convection_quadrature": "trapezoid on declared output times; temporal refinement required",
        },
    )
    fem.atomic_json(
        "thermal-receipt.json",
        {
            "schema_version": 1,
            "adapter": "CalculiX",
            "backend": "cpu",
            "factorization": "SPOOLES",
            "executed": True,
            "software_fallback": False,
            "synthetic": True,
            "precision": "float64",
            "formulation": spec["formulation"],
            "request_sha256": hashlib.sha256(raw).hexdigest(),
            "mesh_sha256": hashlib.sha256(
                fem.read_regular("mesh.json", 32 * 1024**2)
            ).hexdigest(),
            "native_field_sha256": hashlib.sha256(data).hexdigest(),
            "calculix_version": "@ccx_version@",
            "gmsh_version": "@gmsh_version@",
            "gmsh_source_sha256": "be3f66f225d27ba9fa014f07e83169285da8a051b0e8ab7103d88066b39bdd3e",
            "calculix_source_sha256": "9c88385c10fb04f5dc6c4e98027a51bebdd8aee3920e05190d6c1dd08357d6e7",
            "temperature_serialization": "E23.15; 16 significant decimal digits from native real*8",
            "temperature_serialization_patch_sha256": "@temperature_patch_sha256@",
            "nodes": len(nodes),
            "elements": len(cells),
            "physical_times_s": spec["observation_times_s"],
            "integration_substeps": spec["integration_substeps"],
            "maximum_native_step_s": integration_step(spec),
            "energy_output_times_s": output_times(spec),
            "numerical_verification": checks,
            "moisture_risk": spec["moisture_risk"],
            "physical_validation": "unqualified",
            "sandbox": sandbox,
            "limitations": [
                "synthetic box and constant-property material domain only",
                "prescribed planar convection; h is not inferred from velocity",
                "no electronic boot, sealing, condensate-mass or contact inference",
                "no checkpoint/resume qualification",
            ],
        },
    )


if __name__ == "__main__":
    main()
