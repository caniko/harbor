"""Synthetic native planar penalty contact, preload and uniform thermal expansion.

CalculiX 2.23 owns the contact solve. Native DAT, complete force/displacement/
stress and exact geometric gaps verify the explicit series-compliance reference.
This is neither measured gasket behaviour nor an imported-device sealing model.
"""

import hashlib
import importlib.util
import math
import stat
import subprocess
import sys
from pathlib import Path

POLICY = "harbor-cad-contact-cpu-v1"
FIELDS = {
    "schema_version",
    "synthetic",
    "backend",
    "formulation",
    "size_m",
    "resolution",
    "geometry_tolerance_m",
    "initial_gap_m",
    "preload_compression_m",
    "final_compression_m",
    "young_modulus_pa",
    "expansion_per_k",
    "reference_temperature_k",
    "final_temperatures_k",
    "contact_stiffness_pa_m",
    "numerical_tolerance",
    "material_provenance",
    "contact_provenance",
    "boundary_provenance",
}
FACES = [
    (0, 1, 2, 3),
    (4, 7, 6, 5),
    (0, 4, 5, 1),
    (1, 5, 6, 2),
    (2, 6, 7, 3),
    (3, 7, 4, 0),
]


def number(value):
    if (
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
    ):
        raise ValueError("explicit finite SI contact value required")
    return float(value)


def validate(spec):
    if (
        set(spec) != FIELDS
        or type(spec["schema_version"]) is not int
        or spec["schema_version"] != 1
        or spec["synthetic"] is not True
        or spec["backend"] != "cpu"
        or spec["formulation"] != "planar_linear_penalty_contact"
        or type(spec["resolution"]) is not int
        or not 2 <= spec["resolution"] <= 16
    ):
        raise ValueError(
            "versioned synthetic planar CPU contact with explicit refinement required"
        )
    for key, count in (
        ("size_m", 3),
        ("young_modulus_pa", 2),
        ("expansion_per_k", 2),
        ("final_temperatures_k", 2),
    ):
        if not isinstance(spec[key], list) or len(spec[key]) != count:
            raise ValueError(
                "complete explicit contact dimensions/materials/temperatures required"
            )
        for value in spec[key]:
            number(value)
    sizes = spec["size_m"]
    h = sizes[2]
    if (
        any(not 1e-6 <= x <= 0.1 for x in sizes)
        or not 1e-10 <= number(spec["geometry_tolerance_m"]) < 1e-3 * min(sizes)
        or not 0 <= number(spec["initial_gap_m"]) <= 0.01 * h
        or any(not 1e4 <= e <= 1e12 for e in spec["young_modulus_pa"])
        or any(not 0 <= a <= 1e-4 for a in spec["expansion_per_k"])
        or not 100 <= number(spec["reference_temperature_k"]) <= 1000
        or any(not 100 <= t <= 1000 for t in spec["final_temperatures_k"])
        or not 0 < number(spec["contact_stiffness_pa_m"]) <= 1e16
        or not 0 < number(spec["numerical_tolerance"]) <= 0.002
    ):
        raise ValueError(
            "resolved bounded synthetic elasticity, gap and unchanged contact gate required"
        )
    for key in ("preload_compression_m", "final_compression_m"):
        if not 0 <= number(spec[key]) <= 0.001 * h:
            raise ValueError(
                "explicit small-strain displacement-controlled contact required"
            )
    compliance = (
        sum(h / e for e in spec["young_modulus_pa"])
        + 1 / spec["contact_stiffness_pa_m"]
    )
    if not math.isfinite(compliance):
        raise ValueError("finite explicit material/interface compliance required")
    for a, t in zip(spec["expansion_per_k"], spec["final_temperatures_k"], strict=True):
        if abs(a * (t - spec["reference_temperature_k"])) > 0.001:
            raise ValueError("small-strain constant-property thermal contact required")
    for key in ("material_provenance", "contact_provenance", "boundary_provenance"):
        if (
            not isinstance(spec[key], str)
            or not spec[key].strip()
            or len(spec[key]) > 4096
        ):
            raise ValueError(
                "explicit bounded material/contact/boundary provenance required"
            )


def reference(spec, state):
    validate(spec)
    if state not in (1, 2):
        raise ValueError("explicit preload or final static state required")
    h = spec["size_m"][2]
    delta = spec["preload_compression_m" if state == 1 else "final_compression_m"]
    strains = (
        [0.0, 0.0]
        if state == 1
        else [
            a * (t - spec["reference_temperature_k"])
            for a, t in zip(
                spec["expansion_per_k"], spec["final_temperatures_k"], strict=True
            )
        ]
    )
    closure = delta + h * sum(strains) - spec["initial_gap_m"]
    compliance = (
        sum(h / e for e in spec["young_modulus_pa"])
        + 1 / spec["contact_stiffness_pa_m"]
    )
    pressure = max(0.0, closure) / compliance
    return {
        "solver_step_parameter": state,
        "physical_time_s": None,
        "compression_m": delta,
        "thermal_strains": strains,
        "pressure_pa": pressure,
        "gap_m": -pressure / spec["contact_stiffness_pa_m"] if pressure else -closure,
        "model": "p=max(0,compression+sum(alpha*dT*h)-gap)/(sum(h/E)+1/K); Poisson ratio zero",
    }


def verify(spec, nodes, cells, sets, fields):
    validate(spec)
    n, h = spec["resolution"], spec["size_m"][2]
    if (
        len(nodes) != 2 * (n + 1) ** 3
        or len(cells) != 2 * n**3
        or set(fields) != {"displacement", "reaction_force", "stress"}
        or any(
            [v["time"] for v in snapshots] != [1.0, 2.0]
            for snapshots in fields.values()
        )
        or any(
            len(sets[k]) != (n + 1) ** 2 or len(set(sets[k])) != len(sets[k])
            for k in ("bottom", "top", "lower_interface", "upper_interface")
        )
    ):
        raise ValueError(
            "complete two-state native planar contact geometry and fields required"
        )
    nodal_ids = {(tag,) for tag in nodes}
    element_ids = {(tag, ip) for tag in cells for ip in range(1, 9)}
    for name, snapshots in fields.items():
        for snapshot in snapshots:
            count = 6 if name == "stress" else 3
            if set(snapshot["values"]) != (
                element_ids if name == "stress" else nodal_ids
            ) or any(
                len(v) != count or not all(math.isfinite(number(x)) for x in v)
                for v in snapshot["values"].values()
            ):
                raise ValueError(
                    "complete unique finite native contact component coverage required"
                )
    node_offset, cell_offset = max(nodes) // 2, max(sorted(cells)[: len(cells) // 2])
    area = spec["size_m"][0] * spec["size_m"][1]
    checks = []
    for index in range(2):
        expected = reference(spec, index + 1)
        p, delta, strains = (
            expected["pressure_pa"],
            expected["compression_m"],
            expected["thermal_strains"],
        )
        displacement = fields["displacement"][index]["values"]
        force = fields["reaction_force"][index]["values"]
        stress = fields["stress"][index]["values"]
        length_scale = max(
            delta, h * max(map(abs, strains)), spec["geometry_tolerance_m"]
        )
        pressure_scale = max(p, max(spec["young_modulus_pa"]) * length_scale / h)
        errors = []
        for (tag,), value in displacement.items():
            block = int(tag > node_offset)
            z = nodes[tag][2]
            slope = strains[block] - p / spec["young_modulus_pa"][block]
            dz = (
                slope * z
                if block == 0
                else -delta - slope * (2 * h + spec["initial_gap_m"] - z)
            )
            errors.append(
                max(abs(value[0]), abs(value[1]), abs(value[2] - dz)) / length_scale
            )
        stress_error = 0.0
        for (tag, _), value in stress.items():
            block = int(tag > cell_offset)
            transverse = -spec["young_modulus_pa"][block] * strains[block]
            ideal = [transverse, transverse, -p, 0.0, 0.0, 0.0]
            stress_error = max(
                stress_error,
                max(abs(a - b) for a, b in zip(value, ideal, strict=True))
                / pressure_scale,
            )
        bottom = sum(force[tag,][2] for tag in sets["bottom"])
        top = sum(force[tag,][2] for tag in sets["top"])
        force_error = max(
            abs(bottom - p * area),
            abs(top + p * area),
            abs(sum(v[2] for v in force.values())),
        ) / (pressure_scale * area)
        lower = {
            (nodes[tag][0], nodes[tag][1]): displacement[tag,][2]
            for tag in sets["lower_interface"]
        }
        upper = {
            (nodes[tag][0], nodes[tag][1]): displacement[tag,][2]
            for tag in sets["upper_interface"]
        }
        if len(lower) != (n + 1) ** 2 or set(lower) != set(upper):
            raise ValueError(
                "corresponding complete geometric interface points required"
            )
        gap_error = (
            max(
                abs(spec["initial_gap_m"] + upper[xy] - lower[xy] - expected["gap_m"])
                for xy in lower
            )
            / length_scale
        )
        metrics = {
            "displacement": max(errors),
            "stress": stress_error,
            "reaction_force_balance": force_error,
            "gap": gap_error,
        }
        passed = all(
            math.isfinite(v) and v <= spec["numerical_tolerance"]
            for v in metrics.values()
        )
        if not passed:
            raise ValueError(
                f"unchanged contact field/force/gap gate failed: {metrics}"
            )
        checks.append(
            {
                "reference": expected,
                "normalized_errors": metrics,
                "tolerance": spec["numerical_tolerance"],
                "passed": passed,
            }
        )
    return checks


def mesh(spec, fem):
    lower_nodes, lower_cells, lower_sets = fem.mesh(spec, exact_planar_vertices=True)
    Path("reference.msh").rename("lower-reference.msh")
    Path("mesh.json").rename("lower-mesh.json")
    offset, element_offset = max(lower_nodes), max(lower_cells)
    h, gap = spec["size_m"][2], spec["initial_gap_m"]
    nodes = {
        **lower_nodes,
        **{
            tag + offset: [*xyz[:2], xyz[2] + h + gap]
            for tag, xyz in lower_nodes.items()
        },
    }
    cells = {
        **lower_cells,
        **{
            tag + element_offset: [n + offset for n in ids]
            for tag, ids in lower_cells.items()
        },
    }
    sets = {
        "bottom": lower_sets["zmin"],
        "top": [n + offset for n in lower_sets["zmax"]],
        "lower_interface": lower_sets["zmax"],
        "upper_interface": [n + offset for n in lower_sets["zmin"]],
        "lower_all": sorted(lower_nodes),
        "upper_all": [n + offset for n in sorted(lower_nodes)],
    }
    fem.atomic_json(
        "mesh.json",
        {
            "schema_version": 1,
            "synthetic": True,
            "coordinate_unit": "m",
            "nodes": nodes,
            "elements": cells,
            "element_type": "C3D8",
            "boundary_node_sets": sets,
            "initial_gap_m": gap,
            "block_node_offset": offset,
            "block_element_offset": element_offset,
            "positive_gauss_jacobians": True,
            "integrated_volume_m3": 2 * math.prod(spec["size_m"]),
            "geometry": "two separate identical Gmsh boxes; exact translation preserves Jacobians/volume and explicit gap; no healing or ADJUST",
            "lower_mesh_sha256": hashlib.sha256(
                Path("lower-mesh.json").read_bytes()
            ).hexdigest(),
        },
    )
    return nodes, cells, sets, element_offset


def ccx_number(value):
    # Pinned 2.23 expansions.f and boundarys.f read numeric fields using
    # (f20.0). Long Float64 exponent strings can truncate mid-exponent.
    value = number(value)
    for digits in range(17, 12, -1):
        text = format(value, f".{digits}g")
        if len(text) <= 20 and abs(float(text) - value) <= abs(value) * 5e-13:
            return text
    raise ValueError(
        "native 20-character numeric field cannot preserve approved SI value"
    )


def deck(spec, fem, nodes, cells, sets, element_offset):
    lines = fem.mesh_deck(nodes, cells, sets)
    for block, young, alpha in zip(
        ("LOWER", "UPPER"),
        spec["young_modulus_pa"],
        spec["expansion_per_k"],
        strict=True,
    ):
        ids = sorted(
            tag for tag in cells if (tag <= element_offset) == (block == "LOWER")
        )
        lines.append(f"*ELSET,ELSET={block}")
        lines += [",".join(map(str, ids[i : i + 16])) for i in range(0, len(ids), 16)]
        lines += [
            f"*MATERIAL,NAME={block}",
            "*ELASTIC",
            f"{young:.17g},0.",
            f"*EXPANSION,ZERO={ccx_number(spec['reference_temperature_k'])}",
            f"{alpha:.17g}",
            f"*SOLID SECTION,ELSET={block},MATERIAL={block}",
        ]
    h, gap = spec["size_m"][2], spec["initial_gap_m"]
    for name, lower, z in (("MASTER", True, h), ("SLAVE", False, h + gap)):
        lines.append(f"*SURFACE,NAME={name},TYPE=ELEMENT")
        count = 0
        for tag, ids in sorted(cells.items()):
            if (tag <= element_offset) != lower:
                continue
            for face, local in enumerate(FACES, 1):
                if all(
                    abs(nodes[ids[i]][2] - z) <= spec["geometry_tolerance_m"]
                    for i in local
                ):
                    lines.append(f"{tag},S{face}")
                    count += 1
        if count != spec["resolution"] ** 2:
            raise ValueError("complete exact native planar contact faces required")
    lines += [
        "*SURFACE INTERACTION,NAME=CONTACT",
        "*SURFACE BEHAVIOR,PRESSURE-OVERCLOSURE=LINEAR",
        f"{spec['contact_stiffness_pa_m']:.17g}",
        "*CONTACT PAIR,INTERACTION=CONTACT,TYPE=SURFACE TO SURFACE",
        "SLAVE,MASTER",
        "*INITIAL CONDITIONS,TYPE=TEMPERATURE",
        f"NALL,{spec['reference_temperature_k']:.17g}",
        "*BOUNDARY",
        "NALL,1,2,0.",
        "BOTTOM,3,3,0.",
    ]
    for state in (1, 2):
        expected = reference(spec, state)
        lines += [
            "*STEP,NLGEOM,INC=100",
            "*STATIC,SOLVER=SPOOLES",
            "0.1,1.,1e-6,0.1",
            "*BOUNDARY",
            f"TOP,3,3,{-expected['compression_m']:.17g}",
        ]
        if state == 2:
            lines += [
                "*TEMPERATURE",
                f"LOWER_ALL,{spec['final_temperatures_k'][0]:.17g}",
                f"UPPER_ALL,{spec['final_temperatures_k'][1]:.17g}",
            ]
        lines += [
            "*NODE PRINT,NSET=NALL,FREQUENCY=100000",
            "U,RF",
            "*EL PRINT,ELSET=EALL,FREQUENCY=100000",
            "S",
            "*END STEP",
        ]
    compact = []
    for line in lines:
        if line.startswith("*"):
            compact.append(line)
            continue
        parts = []
        for field in line.split(","):
            try:
                value = float(field)
            except ValueError:
                parts.append(field)
            else:
                parts.append(ccx_number(value))
        compact.append(",".join(parts))
    return "\n".join(compact) + "\n"


def main():
    if len(sys.argv) != 3 or sys.argv[1] != "reference":
        raise ValueError("usage: harbor-cad-contact reference request.json")
    source = importlib.util.spec_from_file_location("fem", "@fem_bridge@")
    fem = importlib.util.module_from_spec(source)
    source.loader.exec_module(fem)
    raw = fem.read_regular(sys.argv[2], 65536)
    spec = fem.strict_json(raw)
    validate(spec)
    sandbox = fem.cpu_sandbox(
        "HARBOR_CAD_CONTACT_POLICY", POLICY, "/contact-runtime-closure.txt", sys.argv[2]
    )
    if sandbox is None:
        raise ValueError("operation-specific closure-only CPU contact sandbox required")
    for entry in Path.cwd().iterdir():
        metadata = entry.lstat()
        if (
            entry.name != "contact.log"
            or not stat.S_ISREG(metadata.st_mode)
            or metadata.st_size
            or metadata.st_nlink != 1
        ):
            raise ValueError("fresh native contact stage required")
    nodes, cells, sets, offset = mesh(spec, fem)
    Path("reference.inp").write_text(deck(spec, fem, nodes, cells, sets, offset))
    environment = {
        "HOME": "/nonexistent",
        "LC_ALL": "C",
        "OMP_NUM_THREADS": "1",
        "OPENBLAS_NUM_THREADS": "1",
        "MKL_NUM_THREADS": "1",
        "CCX_NPROC_RESULTS": "1",
        "CCX_NPROC_EQUATION_SOLVER": "1",
    }
    with Path("calculix.log").open("xb") as log:
        process = subprocess.run(
            ["@calculix@", "-i", "reference"],
            stdout=log,
            stderr=subprocess.STDOUT,
            env=environment,
            check=False,
            timeout=180,
        )
    output = fem.read_regular("calculix.log", 16 * 1024**2).decode()
    if (
        process.returncode
        or "*ERROR" in output
        or "Version @ccx_version@" not in output
        or "Job finished" not in output
    ):
        raise ValueError(
            "exact native CalculiX contact solve failed; original fields retained"
        )
    fields = fem.read_dat(
        fem.read_regular("reference.dat", 32 * 1024**2).decode(), reaction_forces=True
    )
    checks = verify(spec, nodes, cells, sets, fields)
    serialized = {
        name: [
            {
                "solver_step_parameter": s["time"],
                "physical_time_s": None,
                "values": [
                    {"id": list(identity), "value": value}
                    for identity, value in sorted(s["values"].items())
                ],
            }
            for s in snapshots
        ]
        for name, snapshots in fields.items()
    }
    fem.atomic_json(
        "fields.json",
        {
            "schema_version": 1,
            "static": True,
            "coordinate_unit": "m",
            "fields": serialized,
            "units": {"displacement": "m", "reaction_force": "N", "stress": "Pa"},
        },
    )
    fem.atomic_json(
        "contact-receipt.json",
        {
            "schema_version": 1,
            "adapter": "CalculiX",
            "backend": "cpu",
            "factorization": "SPOOLES",
            "precision": "float64",
            "input_serialization": {
                "native_numeric_field_characters": 20,
                "maximum_relative_error": 5e-13,
                "source": "CalculiX 2.23 expansions.f/boundarys.f f20.0",
                "original_approved_si_request_preserved": True,
            },
            "executed": True,
            "software_fallback": False,
            "synthetic": True,
            "formulation": spec["formulation"],
            "request": spec,
            "request_sha256": hashlib.sha256(raw).hexdigest(),
            "calculix_version": "@ccx_version@",
            "gmsh_version": "@gmsh_version@",
            "calculix_source_sha256": "9c88385c10fb04f5dc6c4e98027a51bebdd8aee3920e05190d6c1dd08357d6e7",
            "sandbox": sandbox,
            "numerical_verification": checks,
            "physical_validation": "unqualified",
            "outputs": {
                name: hashlib.sha256(fem.read_regular(name, 32 * 1024**2)).hexdigest()
                for name in (
                    "mesh.json",
                    "fields.json",
                    "reference.inp",
                    "reference.dat",
                    "lower-reference.msh",
                )
            },
            "limitations": [
                "synthetic constant properties and uniform prescribed block temperatures",
                "Poisson ratio zero with constrained transverse motion",
                "linear penalty interface is not measured gasket behaviour",
                "no sealing, friction, adhesion, lifetime, imported-device or hybrid GPU inference",
            ],
        },
    )


if __name__ == "__main__":
    main()
