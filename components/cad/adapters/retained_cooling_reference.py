"""Independent original-control enthalpy and boundary-ledger reconstruction."""

import csv
import hashlib
import importlib.util
import io
import math
from pathlib import Path

_common_spec = importlib.util.spec_from_file_location(
    "retained_cooling_common", Path(__file__).with_name("fem_reference.py")
)
common = importlib.util.module_from_spec(_common_spec)
_common_spec.loader.exec_module(common)

KEYS = {
    "schema_version",
    "synthetic",
    "formulation",
    "source_shape",
    "spacing_m",
    "extrusion_m",
    "destination_origin_m",
    "thermal",
    "steps",
    "observation_steps",
    "integration_substeps",
    "spatial_refinement",
}
THERMAL_KEYS = {
    "density_kg_m3",
    "specific_heat_j_kg_k",
    "conductivity_w_m_k",
    "latent_heat_j_kg",
    "melting_temperature_k",
    "initial_temperature_k",
    "cold_wall_temperature_k",
    "material_temperature_domain_k",
    "stefan_number_at_full_water_fraction",
}
COLUMNS = [
    "i",
    "j",
    "parent_i",
    "parent_j",
    "x_m",
    "y_m",
    "water_fraction",
    "specific_enthalpy_j_kg",
    "temperature_k",
    "liquid_fraction",
]


def number(value):
    if type(value) not in (float, int) or not math.isfinite(value):
        raise ValueError("finite original numeric input required")
    return float(value)


def rows(raw, columns):
    parsed = csv.DictReader(io.StringIO(raw.decode("utf-8")))
    if parsed.fieldnames != columns:
        raise ValueError("exact complete original field columns required")
    for row in parsed:
        if set(row) != set(columns) or any(value is None for value in row.values()):
            raise ValueError("complete original scientific rows required")
        yield row


def normalize(spec, original):
    if (
        set(spec) != KEYS
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["formulation"] != "stationary_equal_property_retained_phase_conduction"
    ):
        raise ValueError(
            "strict synthetic stationary native original-control request required"
        )
    shape = spec["source_shape"]
    if (
        not isinstance(shape, list)
        or len(shape) != 2
        or any(type(v) is not int for v in shape)
        or not 3 <= shape[0] <= 241
        or not 4 <= shape[1] <= 145
    ):
        raise ValueError("bounded complete original source shape required")
    q, sub = spec["spatial_refinement"], spec["integration_substeps"]
    if (
        type(q) is not int
        or q not in (1, 2, 3, 4)
        or type(sub) is not int
        or sub not in (1, 2, 4)
    ):
        raise ValueError(
            "explicit conservative spatial and temporal refinement required"
        )
    steps, retained = spec["steps"], spec["observation_steps"]
    if (
        type(steps) is not int
        or not 1 <= steps <= 1000000
        or not isinstance(retained, list)
        or not 2 <= len(retained) <= 16
        or any(type(v) is not int for v in retained)
        or retained != sorted(set(retained))
        or retained[0] != 0
        or retained[-1] != steps
    ):
        raise ValueError(
            "bounded complete original physical observation sequence required"
        )
    dx, depth = number(spec["spacing_m"]), number(spec["extrusion_m"])
    origin = spec["destination_origin_m"]
    if (
        not 0 < dx <= 1
        or not 0 < depth <= 1
        or not isinstance(origin, list)
        or len(origin) != 3
        or any(abs(number(v)) > 1e6 for v in origin)
    ):
        raise ValueError(
            "explicit bounded original spacing, extrusion and translation required"
        )
    material = spec["thermal"]
    if set(material) != THERMAL_KEYS:
        raise ValueError("complete independently supplied thermal state required")
    rho, cp, k, latent, tm, initial, cold = [
        number(material[key])
        for key in (
            "density_kg_m3",
            "specific_heat_j_kg_k",
            "conductivity_w_m_k",
            "latent_heat_j_kg",
            "melting_temperature_k",
            "initial_temperature_k",
            "cold_wall_temperature_k",
        )
    ]
    domain = material["material_temperature_domain_k"]
    if (
        min(rho, cp, k, latent) <= 0
        or not isinstance(domain, list)
        or len(domain) != 2
        or not 100 <= number(domain[0]) <= cold < tm <= number(domain[1]) <= 1000
        or initial != tm
    ):
        raise ValueError(
            "explicit equal material properties/domain and initial melting state required"
        )
    stefan = cp * (tm - cold) / latent
    if (
        not 0.05 <= stefan <= 0.2
        or number(material["stefan_number_at_full_water_fraction"]) != stefan
    ):
        raise ValueError("original full-water Stefan applicability required")
    nx, ny = shape
    parents = {}
    count = 0
    for count, row in enumerate(
        rows(original, ["x_m", "y_m", "material", "phi", "u_lattice", "v_lattice"]), 1
    ):
        index = count - 1
        i, j = index % nx, index // nx
        x, y, kind, phase, u, v = [
            float(row[key])
            for key in ("x_m", "y_m", "material", "phi", "u_lattice", "v_lattice")
        ]
        if (
            count > nx * ny
            or not all(math.isfinite(value) for value in (x, y, kind, phase, u, v))
            or abs(x - i * dx) > dx * 1e-10
            or abs(y - j * dx) > dx * 1e-10
            or kind != (2 if j in (0, ny - 1) else 1)
            or u != 0
            or v != 0
        ):
            raise ValueError(
                "unchanged complete ordered original geometry/material and exactly stationary source required"
            )
        if kind == 1:
            fraction = 1 - phase
            if not 0 <= fraction <= 1:
                raise ValueError(
                    "original bounded water fraction required without clipping"
                )
            parents[i, j] = (x, y, fraction)
    if count != nx * ny:
        raise ValueError("complete original source controls required")
    h = dx / q
    mass = rho * h * h * depth
    dt = h * h * rho * cp / (6 * k * sub)
    thermal_mass = mass * q * q * len(parents)
    water_mass = mass * q * q * math.fsum(row[2] for row in parents.values())
    energy = (
        mass
        * q
        * q
        * math.fsum(cp * (tm - cold) + row[2] * latent for row in parents.values())
    )
    if not all(
        math.isfinite(v) and v > 0 for v in (mass, dt, thermal_mass, water_mass, energy)
    ):
        raise ValueError(
            "finite positive original-control mass and initial enthalpy required"
        )
    return {
        "parents": parents,
        "spacing_m": h,
        "physical_step_s": dt,
        "cell_mass_kg": mass,
        "thermal_mass_kg": thermal_mass,
        "water_mass_kg": water_mass,
        "initial_energy_j": energy,
    }


def read_regular(path, limit):
    try:
        return common.read_regular(path, limit)
    except OSError as error:
        raise ValueError(
            "bounded closed nonsymlink regular original artifact required"
        ) from error


def verify(spec, original, receipt, root, conservation_tolerance=1e-10):
    normalized = normalize(spec, original)
    if not 0 < number(conservation_tolerance) <= 1e-10:
        raise ValueError("unchanged original mass/energy conservation gate required")
    q = spec["spatial_refinement"]
    nx, ny = spec["source_shape"]
    expected = {
        "schema_version": 1,
        "adapter": "OpenLB",
        "backend": "cpu",
        "precision": "float64",
        "source_revision": "145cd54810b468f4b6fd3ed86b10644264841578",
        "collision": "native_total_enthalpy_trt",
        "trt_magic": 0.25,
        "executed": True,
        "software_fallback": False,
        "request": spec,
        "source_spacing_m": spec["spacing_m"],
        "spacing_m": normalized["spacing_m"],
        "physical_step_s": normalized["physical_step_s"],
        "cell_mass_kg": normalized["cell_mass_kg"],
        "source_shape": spec["source_shape"],
        "native_shape": [nx * q, (ny - 2) * q + 2],
        "boundary": "native_half_link_cold_ymin; native_half_link_insulated_ymax; periodic_x",
        "physical_validation": "unqualified",
    }
    for key, value in expected.items():
        observed = receipt.get(key)
        if type(value) is bool:
            same = observed is value
        elif type(value) is int:
            same = type(observed) is int and observed == value
        elif type(value) is float:
            same = math.isclose(number(observed), value, rel_tol=5e-13, abs_tol=0)
        else:
            same = observed == value
        if not same:
            raise ValueError(
                "original native identity, SI conversion or boundary differs from approval"
            )
    snapshots = receipt["snapshots"]
    if [s["step"] for s in snapshots] != spec["observation_steps"]:
        raise ValueError("every exact retained observation required")
    ledger = read_regular(root / "heat-exchange.csv", 128 * 1024**2)
    hashes = {"heat-exchange.csv": hashlib.sha256(ledger).hexdigest()}
    accumulated = [[], []]
    exchange = {0: (0.0, 0.0)}
    count = 0
    dt = normalized["physical_step_s"]
    for count, row in enumerate(
        rows(ledger, ["step", "time_s", "cold_exchange_j", "reflecting_exchange_j"]), 1
    ):
        if (
            row["step"] != str(count)
            or count > spec["steps"]
            or not math.isclose(
                float(row["time_s"]), count * dt, rel_tol=5e-13, abs_tol=0
            )
        ):
            raise ValueError(
                "complete native per-step population-boundary energy exchange required"
            )
        for index, key in enumerate(("cold_exchange_j", "reflecting_exchange_j")):
            accumulated[index].append(number(float(row[key])))
        if count in spec["observation_steps"]:
            exchange[count] = tuple(math.fsum(v) for v in accumulated)
    if count != spec["steps"]:
        raise ValueError("original native boundary ledger ended before final time")
    material = spec["thermal"]
    cp, latent, cold, tm = [
        material[key]
        for key in (
            "specific_heat_j_kg_k",
            "latent_heat_j_kg",
            "cold_wall_temperature_k",
            "melting_temperature_k",
        )
    ]
    scale = cp * (tm - cold) + latent
    origin = spec["destination_origin_m"]
    mass, water, initial = (
        normalized["cell_mass_kg"],
        normalized["water_mass_kg"],
        normalized["initial_energy_j"],
    )
    checks = dict.fromkeys(
        (
            "water_mass_relative_error",
            "initial_enthalpy_relative_error",
            "energy_balance_relative_error",
            "reflecting_exchange_relative_error",
            "phase_enthalpy_normalized_error",
        ),
        0.0,
    )
    observations = []
    for snapshot in snapshots:
        step = snapshot["step"]
        if snapshot["path"] != f"cooling-{step}.csv" or not math.isclose(
            number(snapshot["time_s"]), step * dt, rel_tol=5e-13, abs_tol=0
        ):
            raise ValueError("exact original field path and physical time required")
        raw = read_regular(root / snapshot["path"], 64 * 1024**2)
        hashes[snapshot["path"]] = hashlib.sha256(raw).hexdigest()
        seen = set()
        energies = []
        water_masses = []
        liquid_masses = []
        temperatures = []
        for row in rows(raw, COLUMNS):
            i, j, pi, pj = [int(row[key]) for key in ("i", "j", "parent_i", "parent_j")]
            if (
                not 0 <= i < nx * q
                or not 1 <= j <= (ny - 2) * q
                or (i, j) in seen
                or (pi, pj) != (i // q, (j - 1) // q + 1)
            ):
                raise ValueError(
                    "complete unique native child and original parent identities required"
                )
            seen.add((i, j))
            px, py, fraction = normalized["parents"][pi, pj]
            x, y, f, h, t, phase = [
                number(float(row[key]))
                for key in (
                    "x_m",
                    "y_m",
                    "water_fraction",
                    "specific_enthalpy_j_kg",
                    "temperature_k",
                    "liquid_fraction",
                )
            ]
            wanted_x = origin[0] + px + ((i % q + 0.5) / q - 0.5) * spec["spacing_m"]
            wanted_y = (
                origin[1] + py + (((j - 1) % q + 0.5) / q - 0.5) * spec["spacing_m"]
            )
            if (
                x != wanted_x
                or y != wanted_y
                or f != fraction
                or not 0 <= phase <= 1
                or not cold - 1e-10 <= t <= tm + 1e-10
            ):
                raise ValueError(
                    "unchanged parent phase, explicit SI subcontrol coordinates and bounded native thermal fields required"
                )
            expected_h = cp * (t - cold) + f * latent * phase
            checks["phase_enthalpy_normalized_error"] = max(
                checks["phase_enthalpy_normalized_error"], abs(h - expected_h) / scale
            )
            if step == 0 and (t != tm or (f > 0 and phase != 1)):
                raise ValueError(
                    "explicit original-control initial melting state required"
                )
            energies.append(mass * h)
            water_masses.append(mass * f)
            liquid_masses.append(mass * f * phase)
            temperatures.append(t)
        if len(seen) != len(normalized["parents"]) * q * q:
            raise ValueError("every authoritative original/control/subcontrol required")
        energy, measured_water, liquid = (
            math.fsum(energies),
            math.fsum(water_masses),
            math.fsum(liquid_masses),
        )
        checks["water_mass_relative_error"] = max(
            checks["water_mass_relative_error"], abs(measured_water / water - 1)
        )
        checks["energy_balance_relative_error"] = max(
            checks["energy_balance_relative_error"],
            abs(energy - initial - math.fsum(exchange[step])) / initial,
        )
        checks["reflecting_exchange_relative_error"] = max(
            checks["reflecting_exchange_relative_error"],
            abs(exchange[step][1]) / initial,
        )
        if step == 0:
            checks["initial_enthalpy_relative_error"] = abs(energy / initial - 1)
        for key, value in {
            "energy_j": energy,
            "water_mass_kg": measured_water,
            "liquid_water_mass_kg": liquid,
            "cold_exchange_j": exchange[step][0],
            "reflecting_exchange_j": exchange[step][1],
        }.items():
            denominator = initial if key.endswith("_j") else water
            if abs(number(snapshot[key]) - value) > 5e-12 * denominator:
                raise ValueError(
                    "native summary differs from complete original fields and boundary exchange"
                )
        observations.append(
            {
                "step": step,
                "physical_time_s": step * dt,
                "energy_j": energy,
                "water_mass_kg": measured_water,
                "liquid_water_mass_kg": liquid,
                "solid_water_mass_kg": measured_water - liquid,
                "minimum_temperature_k": min(temperatures),
                "maximum_temperature_k": max(temperatures),
            }
        )
    if any(not math.isfinite(v) or v > conservation_tolerance for v in checks.values()):
        raise ValueError(
            "unchanged independent mass/enthalpy/energy conservation gate failed; original fields retained"
        )
    report = {
        "checks": {
            key: {"error": value, "tolerance": conservation_tolerance, "passed": True}
            for key, value in checks.items()
        },
        "observations": observations,
        "original_files_sha256": hashes,
        "original_source_sha256": hashlib.sha256(original).hexdigest(),
        "executed": False,
        "scope": "independent original-field reconstruction; native execution and physical qualification separate",
    }
    if "independent_verification" in receipt:
        raw_native = read_regular(
            root / "native-retained-cooling-receipt.json", 256 * 1024
        )
        original_native = common.strict_json(raw_native)
        if (
            receipt["independent_verification"] != report
            or receipt.get("original_source_sha256") != report["original_source_sha256"]
            or receipt.get("native_receipt_sha256")
            != hashlib.sha256(raw_native).hexdigest()
            or any(receipt.get(key) != value for key, value in original_native.items())
        ):
            raise ValueError(
                "published reconstruction must preserve the exact original native receipt, source and complete fields"
            )
    return report
