"""Reconstruct conserved original fields and separate complete-history refinements.

This read-only numerical assessment cannot qualify native execution or physical
properties. It consumes preserved diagnostic originals, not native summaries.
"""

import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path


def verifier():
    path = Path(__file__).parents[1] / "adapters/retained_cooling_reference.py"
    loader = importlib.util.spec_from_file_location("retained_cooling_reference", path)
    module = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(module)
    return module, path


def history(module, spec, root):
    q, sub = spec["spatial_refinement"], spec["integration_substeps"]
    fields = {}
    for step in spec["observation_steps"]:
        scale = q * q * sub
        if step % scale:
            raise ValueError(
                "exact independent equal-time refinement observations required"
            )
        rows = module.rows(
            module.read_regular(root / f"cooling-{step}.csv", 64 * 1024**2),
            module.COLUMNS,
        )
        parents = {}
        for row in rows:
            parent = int(row["parent_i"]), int(row["parent_j"])
            parents.setdefault(parent, []).append(
                tuple(
                    float(row[key])
                    for key in (
                        "specific_enthalpy_j_kg",
                        "temperature_k",
                        "liquid_fraction",
                    )
                )
            )
        if any(len(rows) != q * q for rows in parents.values()):
            raise ValueError(
                "complete congruent subcontrols for every original parent required"
            )
        fields[step // scale] = {
            p: tuple(math.fsum(row[k] for row in rows) / (q * q) for k in range(3))
            for p, rows in parents.items()
        }
    return fields


def refinements(fields, names, span):
    records = []
    if len(names) != 3 or any(
        set(fields[name]) != set(fields[names[-1]]) for name in names
    ):
        raise ValueError(
            "three independent identical complete physical histories required"
        )
    for step, finest in fields[names[-1]].items():
        errors = []
        for name in names[:-1]:
            field = fields[name][step]
            if set(field) != set(finest):
                raise ValueError("unchanged complete original parent coverage required")
            denominator = math.fsum(finest[p][0] ** 2 for p in finest)
            if denominator <= 0:
                raise ValueError(
                    "positive finite enthalpy norm required; no invented epsilon"
                )
            errors.append(
                {
                    "enthalpy_relative_l2": math.sqrt(
                        math.fsum((field[p][0] - finest[p][0]) ** 2 for p in finest)
                        / denominator
                    ),
                    "temperature_span_l2": math.sqrt(
                        math.fsum(
                            ((field[p][1] - finest[p][1]) / span) ** 2 for p in finest
                        )
                        / len(finest)
                    ),
                    "liquid_fraction_l2": math.sqrt(
                        math.fsum((field[p][2] - finest[p][2]) ** 2 for p in finest)
                        / len(finest)
                    ),
                }
            )
        records.append(
            {
                "base_step": step,
                "errors_against_finest": errors,
                "assessment": "initial_identity" if step == 0 else "evolved_refinement",
                "tolerance": 1e-10 if step == 0 else 0.02,
                "passed": all(
                    v <= (1e-10 if step == 0 else 0.02)
                    for row in errors
                    for v in row.values()
                )
                and (step == 0 or all(errors[1][k] <= errors[0][k] for k in errors[0])),
            }
        )
    return {
        "assessments": records,
        "passed": all(r["passed"] for r in records),
        "scope": "same original parents and half-link physical boundaries; complete retained enthalpy/temperature/liquid histories, not just fully frozen final state",
    }


def assess(root, minimum_refinement=1):
    module, path = verifier()
    raw = module.read_regular(root / "verification.json", 256 * 1024)
    report = module.common.strict_json(raw)
    independent = []
    fields = {}
    signature = None
    expected = {
        "uniform",
        "retained-s1-t1",
        "retained-s1-t2",
        "retained-s1-t4",
        "retained-s2-t1",
        "retained-s4-t1",
    }
    if minimum_refinement == 2:
        expected |= {"retained-s3-t1", "retained-s2-t2", "retained-s2-t4"}
    elif minimum_refinement != 1:
        raise ValueError("independently declared supported refinement series required")
    if {r["case"] for r in report["results"]} != expected or len(
        report["results"]
    ) != len(expected):
        raise ValueError(
            "exact complete independently preserved uniform/retained diagnostic cases required"
        )
    for row in report["results"]:
        name = row["case"]
        spec = row["request"]
        expected_levels = (
            (1, 1)
            if name == "uniform"
            else tuple(int(v[1:]) for v in name.split("-")[1:])
        )
        if (
            spec["spatial_refinement"],
            spec["integration_substeps"],
        ) != expected_levels:
            raise ValueError(
                "distinct declared native spatial/temporal refinement levels required"
            )
        inputs = root / ("input-" + name)
        work = root / name
        original = module.read_regular(inputs / "wetting-original.csv", 16 * 1024**2)
        native_raw = module.read_regular(
            work / "retained-cooling-receipt.json", 256 * 1024
        )
        if (
            hashlib.sha256(original).hexdigest() != row["source_sha256"]
            or hashlib.sha256(native_raw).hexdigest() != row["receipt_sha256"]
            or module.common.strict_json(
                module.read_regular(inputs / "request.json", 65536)
            )
            != spec
        ):
            raise ValueError(
                "complete unchanged source, approved diagnostic request and native receipt required"
            )
        independent.append(
            {
                "case": name,
                "reconstruction": module.verify(
                    spec, original, module.common.strict_json(native_raw), work
                ),
            }
        )
        if name == "uniform":
            continue
        current = {
            key: spec[key]
            for key in (
                "source_shape",
                "spacing_m",
                "extrusion_m",
                "destination_origin_m",
                "thermal",
                "formulation",
            )
        }
        current["original_sha256"] = hashlib.sha256(original).hexdigest()
        if signature is not None and signature != current:
            raise ValueError(
                "identical scientific model and unchanged original source across refinements required"
            )
        signature = current
        fields[name] = history(module, spec, work)
    span = (
        signature["thermal"]["melting_temperature_k"]
        - signature["thermal"]["cold_wall_temperature_k"]
    )
    spatial = (
        ["retained-s1-t1", "retained-s2-t1", "retained-s4-t1"]
        if minimum_refinement == 1
        else ["retained-s2-t1", "retained-s3-t1", "retained-s4-t1"]
    )
    temporal = (
        ["retained-s1-t1", "retained-s1-t2", "retained-s1-t4"]
        if minimum_refinement == 1
        else ["retained-s2-t1", "retained-s2-t2", "retained-s2-t4"]
    )
    return {
        "schema_version": 1,
        "native_report": str(root / "verification.json"),
        "native_report_sha256": hashlib.sha256(raw).hexdigest(),
        "verifier_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "independent_original_fields": independent,
        "minimum_spatial_refinement": minimum_refinement,
        "spatial": refinements(fields, spatial, span),
        "temporal": refinements(fields, temporal, span),
        "unrefined_spatial": refinements(
            fields, ["retained-s1-t1", "retained-s2-t1", "retained-s4-t1"], span
        ),
        "executed": False,
        "physical_validation": "unqualified",
        "scope": "independent complete-history numerical verification; native execution, analytic Stefan and physical/product qualification remain separately required",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--minimum-refinement", type=int, choices=(1, 2), default=1)
    args = parser.parse_args()
    report = assess(args.reference.resolve(strict=True), args.minimum_refinement)
    with args.output.open("x") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
    print(
        json.dumps(
            {"spatial": report["spatial"], "temporal": report["temporal"]}, indent=2
        )
    )
    if not report["spatial"]["passed"] or not report["temporal"]["passed"]:
        raise ValueError(
            "complete original-field refinement gate failed; independent report retained"
        )


if __name__ == "__main__":
    main()
