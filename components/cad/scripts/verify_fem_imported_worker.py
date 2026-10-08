"""Source-bound imported static FEM through exact packaged CLI/MCP and lifecycle."""

import importlib.util
import json
from pathlib import Path

from verify_cad_mesh_worker import MeshRecipe, arguments, checksum, run_gate


class ImportedFemRecipe(MeshRecipe):
    schema_version = 8
    closure_key = "fem_imported_closure"
    native_key = "fem_imported"
    cli = ("fem-imported",)
    profile = "simulation"
    plan_tool = "fem_plan_imported"
    submit_tool = "job_submit"
    policy = "harbor-cad-fem-imported-cpu-v1"
    stage_directory = "stages/fem-imported"
    receipt_file = "fem-imported-receipt.json"
    descriptor_file = "native-fem-imported-request.json"
    scope = "registered original CAD to durable isolated synthetic CPU static conduction/free expansion references; no contact or physical validation"

    def __init__(self, args):
        if args.native_reference is None:
            raise ValueError(
                "qualified exact standalone imported FEM prerequisite required"
            )
        report = json.loads((args.native_reference / "verification.json").read_text())
        if len(report["results"]) != 12 or len(report["rejections"]) != 8:
            raise ValueError(
                "complete native origin/translated static references required"
            )
        standalone = json.loads(Path(report["runtime"]).read_text())
        worker = json.loads(args.runtime.resolve(strict=True).read_text())
        if (
            checksum(Path(report["runtime"])) != report["runtime_sha256"]
            or worker["fem_imported"] != standalone["fem_imported"]
        ):
            raise ValueError(
                "worker must use exact native-qualified imported FEM executable"
            )
        self.references = {
            label: next(
                r["request"]
                for r in report["results"]
                if r["fixture"] == label and r["resolution"] == 8 and r["mode"] == mode
            )
            for label, mode in (
                ("origin", "thermal_boundary"),
                ("translated", "free_expansion"),
            )
        }
        path = Path(__file__).resolve().parents[1] / "adapters/fem_reference.py"
        spec = importlib.util.spec_from_file_location("fem_reference", path)
        self.fem = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.fem)

    def request(self, source_job, label, resolution):
        reference = self.references[label]
        return {
            "source_job": source_job,
            "region_name": "solid",
            "spec": {
                "schema_version": 1,
                "reference": {**reference["reference"], "resolution": resolution},
                "material_provenance": reference["material_provenance"],
                "boundary_provenance": reference["boundary_provenance"],
            },
        }

    def formulation(self, plan):
        return "imported_" + plan["imported_fem"]["reference"]["mode"]

    def verify(self, plan, data, receipt):
        checks = super().verify(plan, data, receipt)
        mesh = json.loads((data / "mesh.json").read_text())
        raw = (data / "reference.dat").read_bytes()
        assert checksum(data / "reference.dat") == receipt["native_field_sha256"]
        geometry, spec = plan["cad_source"]["geometry"], plan["imported_fem"]
        origin = [geometry["bounds_m"][i] for i in (0, 2, 4)]
        assert (
            receipt["world_origin_m"] == origin
            and receipt["material_provenance"] == spec["material_provenance"]
            and receipt["boundary_provenance"] == spec["boundary_provenance"]
        )
        fields = self.fem.read_dat(raw.decode())
        rechecked = self.fem.verify(
            spec["reference"],
            {int(k): v for k, v in mesh["nodes"].items()},
            {int(k): v for k, v in mesh["elements"].items()},
            fields,
            origin=origin,
        )
        assert rechecked == receipt["numerical_verification"] and all(
            v["passed"] for v in rechecked.values()
        )
        decoded = json.loads((data / "imported-fields.json").read_text())
        assert (
            decoded["coordinate_unit"] == "m"
            and decoded["world_origin_m"] == origin
            and decoded["static"]
        )
        for values in decoded["fields"].values():
            assert all(snapshot["physical_time_s"] is None for snapshot in values)
        return {**checks, "fields": rechecked}


if __name__ == "__main__":
    args = arguments()
    run_gate(args, ImportedFemRecipe(args))
