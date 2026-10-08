"""Independent original-field checks for the pinned conduction Stefan driver."""

import csv
import hashlib
import importlib.util
import io
import math
import stat
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

POLICY = "harbor-cad-freezing-cpu-v1"
COLUMNS = [
    "i",
    "j",
    "x_m",
    "y_m",
    "material",
    "specific_enthalpy_j_kg",
    "temperature_k",
    "liquid_fraction",
]
KEYS = {
    "schema_version",
    "synthetic",
    "backend",
    "formulation",
    "size_m",
    "resolution",
    "density_kg_m3",
    "specific_heat_j_kg_k",
    "conductivity_w_m_k",
    "latent_heat_j_kg",
    "melting_temperature_k",
    "initial_temperature_k",
    "cold_wall_temperature_k",
    "material_temperature_domain_k",
    "steps",
    "observation_steps",
    "front_tolerance",
    "temperature_tolerance",
    "mass_tolerance",
    "energy_tolerance",
    "material_provenance",
    "boundary_provenance",
    "geometry_provenance",
    "moisture_risk",
}


def validate(spec, fem):
    if (
        set(spec) != KEYS
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["backend"] != "cpu"
        or spec["formulation"] != "conduction_stefan_solidification_2d"
    ):
        raise ValueError(
            "strict synthetic CPU conduction solidification request required"
        )
    n, steps, retained = spec["resolution"], spec["steps"], spec["observation_steps"]
    if (
        type(n) is not int
        or not 32 <= n <= 256
        or n % 8
        or type(steps) is not int
        or not n * n // 2 <= steps <= n * n
        or not isinstance(retained, list)
        or not 2 <= len(retained) <= 16
        or any(type(i) is not int for i in retained)
        or retained[0] != 0
        or retained[-1] != steps
        or retained != sorted(set(retained))
        or any(i and not n * n // 2 <= i <= steps for i in retained)
    ):
        raise ValueError("bounded grid and original observation schedule required")
    size = [fem.number(v) for v in spec["size_m"]]
    domain = [fem.number(v) for v in spec["material_temperature_domain_k"]]
    rho, cp, k, latent = [
        fem.number(spec[key])
        for key in (
            "density_kg_m3",
            "specific_heat_j_kg_k",
            "conductivity_w_m_k",
            "latent_heat_j_kg",
        )
    ]
    tm, cold, initial = [
        fem.number(spec[key])
        for key in (
            "melting_temperature_k",
            "cold_wall_temperature_k",
            "initial_temperature_k",
        )
    ]
    if (
        len(size) != 3
        or any(not 1e-6 <= v <= 1 for v in size)
        or size[1] != size[0] / 8
        or len(domain) != 2
        or not 100 <= domain[0] < domain[1] <= 1000
        or min(rho, cp, k, latent) <= 0
        or not domain[0] <= cold < tm <= domain[1]
        or initial != tm
    ):
        raise ValueError(
            "explicit SI equal-phase properties and covered material temperatures required"
        )
    span = tm - cold
    stefan = cp * span / latent
    dx, dt = size[0] / n, (size[0] / n) ** 2 * rho * cp / (6 * k)
    cell_mass = rho * dx * dx * size[2]
    if not 0.05 <= stefan <= 0.2 or not all(
        math.isfinite(v) and v > 0
        for v in (
            dx,
            dt,
            cp * span + latent,
            steps * dt,
            cell_mass,
            cell_mass * (cp * span + latent) * (n - 1) * (n // 8),
        )
    ):
        raise ValueError(
            "finite fixed-tau SI scaling and bounded Stefan number required"
        )
    for key in (
        "front_tolerance",
        "temperature_tolerance",
        "mass_tolerance",
        "energy_tolerance",
    ):
        if (
            not 0
            < fem.number(spec[key])
            <= (0.02 if key in {"front_tolerance", "temperature_tolerance"} else 1e-10)
        ):
            raise ValueError("unchanged numerical/mass/energy gates required")
    for key in ("material_provenance", "boundary_provenance", "geometry_provenance"):
        if (
            not isinstance(spec[key], str)
            or not spec[key].strip()
            or len(spec[key]) > 4096
        ):
            raise ValueError("bounded explicit provenance required")
    moisture = spec["moisture_risk"]
    if not isinstance(moisture, dict):
        raise TypeError("explicit moisture assessment required")
    if moisture.get("assessment") in {"missing", "inapplicable"}:
        key = "reason" if moisture["assessment"] == "missing" else "justification"
        if (
            set(moisture) != {"assessment", key}
            or not isinstance(moisture[key], str)
            or not moisture[key].strip()
            or len(moisture[key]) > 4096
        ):
            raise ValueError("bounded missing-input/inapplicability record required")
    elif moisture.get("assessment") == "dew_point_screening":
        air = moisture["air_temperature"]
        if (
            set(moisture)
            != {"assessment", "air_temperature", "relative_humidity", "provenance"}
            or set(air) != {"value", "unit"}
            or air["unit"] not in {"K", "degC"}
            or not 273.15
            <= fem.number(air["value"]) + (273.15 if air["unit"] == "degC" else 0)
            <= 323.15
            or not 0 < fem.number(moisture["relative_humidity"]) <= 1
            or not isinstance(moisture["provenance"], str)
            or not moisture["provenance"].strip()
            or len(moisture["provenance"]) > 4096
        ):
            raise ValueError(
                "explicit bounded air assessment required; native fields do not imply frost transport"
            )
    else:
        raise ValueError("allowlisted moisture assessment required")
    return dx, dt, stefan


def similarity_parameter(stefan):
    lo, hi = 0.0, 2.0
    for _ in range(100):
        value = (lo + hi) / 2
        if value * math.exp(value * value) * math.erf(value) < stefan / math.sqrt(
            math.pi
        ):
            lo = value
        else:
            hi = value
    return (lo + hi) / 2


def csv_rows(raw, columns):
    reader = csv.DictReader(io.StringIO(raw.decode("ascii")))
    if reader.fieldnames != columns:
        raise ValueError("exact native scientific columns required")
    for row in reader:
        if set(row) != set(columns) or any(v is None for v in row.values()):
            raise ValueError("complete native scientific row required")
        yield row


def vtk_bytes(spec, raw):
    """Canonical Float64 point grid, including zero-measure cold-wall nodes."""
    n, dx = spec["resolution"], spec["size_m"][0] / spec["resolution"]
    extent = f"0 {n - 1} 0 {n // 8 - 1} 0 0"
    tree = ET.Element(
        "VTKFile", type="ImageData", version="1.0", byte_order="LittleEndian"
    )
    image = ET.SubElement(
        tree,
        "ImageData",
        WholeExtent=extent,
        Origin=f"0 {dx / 2:.17g} 0",
        Spacing=f"{dx:.17g} {dx:.17g} {spec['size_m'][2]:.17g}",
    )
    piece = ET.SubElement(image, "Piece", Extent=extent)
    point = ET.SubElement(piece, "PointData")
    ET.SubElement(piece, "CellData")
    rows = sorted(
        csv_rows(raw, COLUMNS), key=lambda row: (int(row["j"]), int(row["i"]))
    )
    for name, data_type, unit in (
        ("i", "Int32", "1"),
        ("j", "Int32", "1"),
        ("material", "Int32", "1"),
        ("specific_enthalpy_j_kg", "Float64", "J/kg"),
        ("temperature_k", "Float64", "K"),
        ("liquid_fraction", "Float64", "1"),
    ):
        array = ET.SubElement(
            point,
            "DataArray",
            type=data_type,
            Name=name,
            NumberOfComponents="1",
            format="ascii",
            unit=unit,
        )
        array.text = " ".join(
            str(int(row[name]))
            if data_type == "Int32"
            else format(float(row[name]), ".17g")
            for row in rows
        )
    return ET.tostring(tree, encoding="utf-8", xml_declaration=True)


def collection_bytes(observations):
    tree = ET.Element(
        "VTKFile", type="Collection", version="1.0", byte_order="LittleEndian"
    )
    collection = ET.SubElement(tree, "Collection")
    for observation in observations:
        ET.SubElement(
            collection,
            "DataSet",
            timestep=format(observation["physical_time_s"], ".17g"),
            group="",
            part="0",
            file=f"freezing-{observation['step']}.vti",
        )
    return ET.tostring(tree, encoding="utf-8", xml_declaration=True)


def scientific_exports(spec, observations, root, fem, create=False):
    """Bind portable point fields and physical-time collection to native CSV bytes."""
    records = [("freezing.pvd", collection_bytes(observations))]
    for observation in observations:
        name = f"freezing-{observation['step']}"
        records.append(
            (
                name + ".vti",
                vtk_bytes(spec, fem.read_regular(root / (name + ".csv"), 16 * 1024**2)),
            )
        )
    hashes = {}
    for name, data in records:
        if create:
            with (root / (name + ".partial")).open("xb") as stream:
                stream.write(data)
            if (root / name).exists() or (root / name).is_symlink():
                raise ValueError("new immutable scientific export required")
            (root / (name + ".partial")).rename(root / name)
        elif fem.read_regular(root / name, 16 * 1024**2) != data:
            raise ValueError(
                "portable scientific fields differ from original native CSV values"
            )
        hashes[name] = hashlib.sha256(data).hexdigest()
    return hashes


def verify(spec, receipt, root, fem):
    dx, dt, stefan = validate(spec, fem)
    n = spec["resolution"]
    cp, latent, cold, span = (
        spec["specific_heat_j_kg_k"],
        spec["latent_heat_j_kg"],
        spec["cold_wall_temperature_k"],
        spec["melting_temperature_k"] - spec["cold_wall_temperature_k"],
    )
    cell_mass = spec["density_kg_m3"] * dx * dx * spec["size_m"][2]
    mass = cell_mass * (n - 1) * (n // 8)
    expected = {
        "schema_version": 1,
        "adapter": "OpenLB",
        "backend": "cpu",
        "executed": True,
        "software_fallback": False,
        "precision": "float64",
        "source_revision": "145cd54810b468f4b6fd3ed86b10644264841578",
        "formulation": spec["formulation"],
        "dimensionality": 2,
        "synthetic": True,
        "request": spec,
        "shape": [n + 1, n // 8],
        "spacing_m": dx,
        "physical_step_s": dt,
        "stefan_number": stefan,
        "cell_mass_kg": cell_mass,
        "active_volume_m3": (n - 1) * (n // 8) * dx * dx * spec["size_m"][2],
        "active_control_bounds_m": [
            [0.5 * dx, spec["size_m"][0] - 0.5 * dx],
            [0.0, spec["size_m"][1]],
            [0.0, spec["size_m"][2]],
        ],
        "energy_zero": "solid at prescribed cold wall temperature",
        "physical_validation": "unqualified",
    }
    for key, value in expected.items():
        observed = receipt.get(key)
        if type(value) is bool:
            if observed is not value:
                raise ValueError("exact native execution booleans required")
        elif type(value) is float:
            if not math.isclose(fem.number(observed), value, rel_tol=5e-13, abs_tol=0):
                raise ValueError(
                    "native converter/geometry differs from explicit SI inputs"
                )
        elif observed != value:
            raise ValueError(
                "native receipt/source/scope differs from approved reference"
            )
    snapshots = receipt["snapshots"]
    if (
        len(snapshots) != len(spec["observation_steps"])
        or [s["step"] for s in snapshots] != spec["observation_steps"]
    ):
        raise ValueError("exact retained original snapshots required")
    raw_ledger = fem.read_regular(root / "heat-exchange.csv", 32 * 1024**2)
    hashes = {"heat-exchange.csv": hashlib.sha256(raw_ledger).hexdigest()}
    exchange = {0: (0.0, 0.0)}
    cumulative = [0.0, 0.0]
    count = 0
    for count, row in enumerate(
        csv_rows(
            raw_ledger, ["step", "time_s", "cold_exchange_j", "reflecting_exchange_j"]
        ),
        1,
    ):
        if (
            row["step"] != str(count)
            or not math.isclose(float(row["time_s"]), count * dt, rel_tol=5e-13)
            or count > spec["steps"]
        ):
            raise ValueError(
                "complete native per-step boundary-exchange ledger required"
            )
        for i, key in enumerate(("cold_exchange_j", "reflecting_exchange_j")):
            value = float(row[key])
            if not math.isfinite(value):
                raise ValueError("finite native boundary flux required")
            cumulative[i] += value
        if count in spec["observation_steps"]:
            exchange[count] = tuple(cumulative)
    if count != spec["steps"]:
        raise ValueError("native boundary exchange ended before final physical time")
    parameter = similarity_parameter(stefan)
    maximum = dict.fromkeys(("front", "temperature", "mass", "energy"), 0.0)
    observations = []
    initial_energy = mass * (cp * span + latent)
    for snapshot in snapshots:
        step = snapshot["step"]
        name = f"freezing-{step}.csv"
        if snapshot["path"] != name or not math.isclose(
            fem.number(snapshot["time_s"]), step * dt, rel_tol=5e-13, abs_tol=0
        ):
            raise ValueError("exact observation path/physical time required")
        raw = fem.read_regular(root / name, 16 * 1024**2)
        hashes[name] = hashlib.sha256(raw).hexdigest()
        seen, energies, liquid_masses, phases = set(), [], [], {}
        reference_front = 2 * parameter * math.sqrt(step / (6 * n * n)) if step else 0.0
        for row in csv_rows(raw, COLUMNS):
            i, j, material = int(row["i"]), int(row["j"]), int(row["material"])
            if (
                not 0 <= i < n
                or not 0 <= j < n // 8
                or (i, j) in seen
                or material != (3 if i == 0 else 1)
            ):
                raise ValueError(
                    "complete nonduplicated original native grid/material identities required"
                )
            seen.add((i, j))
            x, y, h, t, phase = [
                float(row[key])
                for key in (
                    "x_m",
                    "y_m",
                    "specific_enthalpy_j_kg",
                    "temperature_k",
                    "liquid_fraction",
                )
            ]
            if (
                not all(math.isfinite(v) for v in (x, y, h, t, phase))
                or not math.isclose(x, i * dx, rel_tol=5e-13, abs_tol=1e-15 * dx)
                or not math.isclose(y, (j + 0.5) * dx, rel_tol=5e-13)
                or not 0 <= phase <= 1
            ):
                raise ValueError(
                    "finite SI original fields and exact cell-center coordinates required"
                )
            theta = (t - cold) / span
            expected_phase = max(0.0, min(1.0, (h - cp * span) / latent))
            if (
                abs(h - (cp * (t - cold) + latent * phase))
                > 5e-12 * (cp * span + latent)
                or abs(phase - expected_phase) > 5e-13
                or not spec["material_temperature_domain_k"][0]
                <= t
                <= spec["material_temperature_domain_k"][1]
            ):
                raise ValueError(
                    "native enthalpy/temperature/phase relation and material domain required"
                )
            if i == 0:
                if (
                    abs(theta) > 1e-12
                    or phase != 0
                    or abs(h) > 1e-12 * (cp * span + latent)
                ):
                    raise ValueError("prescribed cold wall failed")
            else:
                energies.append(h * cell_mass)
                liquid_masses.append(phase * cell_mass)
                if step == 0 and (
                    abs(theta - 1) > 1e-12
                    or phase != 1
                    or abs(h / (cp * span + latent) - 1) > 1e-12
                ):
                    raise ValueError("fully liquid original initial state required")
            if step:
                reference_temperature = (
                    math.erf(
                        (x / spec["size_m"][0]) / (2 * math.sqrt(step / (6 * n * n)))
                    )
                    / math.erf(parameter)
                    if x / spec["size_m"][0] < reference_front
                    else 1.0
                )
                maximum["temperature"] = max(
                    maximum["temperature"], abs(theta - reference_temperature)
                )
            phases[i, j] = phase
        if len(seen) != n * (n // 8):
            raise ValueError(
                "every authoritative native phase/temperature sample required"
            )
        energy, liquid_mass = math.fsum(energies), math.fsum(liquid_masses)
        balance = abs(energy - initial_energy - sum(exchange[step])) / initial_energy
        reflecting_loss = abs(exchange[step][1]) / initial_energy
        maximum["energy"] = max(maximum["energy"], balance, reflecting_loss)
        measured_mass = len(energies) * cell_mass
        maximum["mass"] = max(
            maximum["mass"],
            abs(measured_mass / mass - 1),
            abs((liquid_mass + (measured_mass - liquid_mass)) / mass - 1),
        )
        for key, value in {
            "energy_j": energy,
            "mass_kg": measured_mass,
            "liquid_mass_kg": liquid_mass,
            "cold_exchange_j": exchange[step][0],
            "reflecting_exchange_j": exchange[step][1],
        }.items():
            scale = initial_energy if key.endswith("_j") else mass
            if abs(fem.number(snapshot[key]) - value) > 5e-12 * scale:
                raise ValueError(
                    "native observation summary differs from original fields/flux"
                )
        if step:
            for j in range(n // 8):
                crossings = [
                    i for i in range(n - 1) if phases[i, j] <= 0.5 < phases[i + 1, j]
                ]
                if len(crossings) != 1:
                    raise ValueError(
                        "one complete planar solidification front per row required"
                    )
                i = crossings[0]
                front = (
                    i + (0.5 - phases[i, j]) / (phases[i + 1, j] - phases[i, j])
                ) / n
                maximum["front"] = max(maximum["front"], abs(front - reference_front))
        observations.append(
            {
                "step": step,
                "physical_time_s": step * dt,
                "reference_front_m": reference_front * spec["size_m"][0],
                "mass_kg": measured_mass,
                "liquid_mass_kg": liquid_mass,
                "solid_mass_kg": measured_mass - liquid_mass,
                "energy_j": energy,
                "energy_relative_error": balance,
            }
        )
    checks = {}
    for key, error in maximum.items():
        tolerance = spec[key + "_tolerance"]
        if not math.isfinite(error) or error > tolerance:
            raise ValueError(
                f"unchanged {key} gate failed: {error} > {tolerance}; original fields retained"
            )
        checks[key] = {"error": error, "tolerance": tolerance, "passed": True}
    if "original_files_sha256" in receipt:
        hashes.update(scientific_exports(spec, observations, root, fem))
        raw_native = fem.read_regular(root / "native-freezing-receipt.json", 256 * 1024)
        hashes["native-freezing-receipt.json"] = hashlib.sha256(raw_native).hexdigest()
        original = fem.strict_json(raw_native)
        if original.get(
            "numerical_verification"
        ) != "independent complete-field Stefan/mass/energy gate required" or any(
            receipt.get(key) != value
            for key, value in original.items()
            if key != "numerical_verification"
        ):
            raise ValueError(
                "checked receipt must retain the exact native metadata and snapshots"
            )
        if (
            hashes != receipt["original_files_sha256"]
            or receipt["numerical_verification"] != checks
            or receipt["independent_observations"] != observations
        ):
            raise ValueError(
                "checked receipt differs from closed authoritative originals"
            )
    return checks, observations, hashes


def fresh_output(root):
    for entry in root.iterdir():
        info = entry.lstat()
        if (
            entry.name != "freezing.log"
            or not stat.S_ISREG(info.st_mode)
            or info.st_nlink != 1
            or info.st_size
        ):
            raise ValueError(
                "fresh stage with only its empty regular worker log required"
            )


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "reference":
        raise ValueError("usage: harbor-cad-freezing reference request.json")
    module = importlib.util.spec_from_file_location("freezing_common", "@fem_bridge@")
    fem = importlib.util.module_from_spec(module)
    module.loader.exec_module(fem)
    raw_request = fem.read_regular(sys.argv[2], 65536)
    spec = fem.strict_json(raw_request)
    validate(spec, fem)
    sandbox = fem.cpu_sandbox(
        "HARBOR_CAD_FREEZING_POLICY",
        POLICY,
        "/freezing-runtime-closure.txt",
        sys.argv[2],
    )
    if sandbox is None:
        raise ValueError("operation-specific CPU freezing sandbox required")
    root = Path.cwd()
    fresh_output(root)
    with Path("process.log").open("xb") as log:
        process = subprocess.run(
            ["@native@", "reference", sys.argv[2]],
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=180,
            env={"HOME": "/home/worker", "LC_ALL": "C"},
            check=False,
        )
    if process.returncode:
        raise ValueError(
            "native solidification failed; all closed/partial originals retained"
        )
    raw = fem.read_regular("freezing-receipt.json", 256 * 1024)
    receipt = fem.strict_json(raw)
    checks, observations, hashes = verify(spec, receipt, root, fem)
    hashes.update(scientific_exports(spec, observations, root, fem, create=True))
    Path("freezing-receipt.json").rename("native-freezing-receipt.json")
    hashes["native-freezing-receipt.json"] = hashlib.sha256(raw).hexdigest()
    receipt.update(
        numerical_verification=checks,
        independent_observations=observations,
        original_files_sha256=hashes,
        request_sha256=hashlib.sha256(raw_request).hexdigest(),
        sandbox=sandbox,
        moisture_risk=spec["moisture_risk"],
    )
    fem.atomic_json("freezing-receipt.json", receipt)


if __name__ == "__main__":
    main()
