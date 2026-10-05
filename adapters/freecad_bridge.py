"""Trusted fixed FreeCAD script, launched only inside the import sandbox.

Native FreeCAD Python ABI, never imported by MCP. The patched 1.1.4 importer
is mandatory even with safe-mode and macro preferences disabled.
"""

import json
import os
from pathlib import Path

import FreeCAD as App
import MeshPart


def atomic_json(path, value):
    partial = Path(str(path) + ".partial")
    partial.write_text(json.dumps(value, allow_nan=False, indent=2))
    partial.replace(path)


def main():
    op = os.environ["HARBOR_CAD_OPERATION"]
    plan = json.loads(Path(os.environ["HARBOR_CAD_PLAN"]).read_text())
    case = plan["case"]
    version = tuple(int(v) for v in App.Version()[:3])
    if version < (1, 1, 4):
        raise RuntimeError("FreeCAD >= 1.1.4 security fixes required")
    App.ParamGet("User parameter:BaseApp/Preferences/Macro").SetBool("AutoRun", False)
    if op == "cad_fixture":
        if not case["geometry"]["synthetic"]:
            raise ValueError("fixture must be explicitly synthetic")
        if case["length"]["unit"] != "m" or case["channel_height"]["unit"] != "m":
            raise ValueError("synthetic fixture requires SI length inputs")
        doc = App.newDocument("HarborSyntheticChannel")
        fluid = doc.addObject("Part::Box", "fluid")
        fluid.Label = "synthetic fluid channel"
        fluid.Length = case["length"]["value"] * 1000
        fluid.Width = case["channel_height"]["value"] * 1000
        fluid.Height = case["channel_height"]["value"] * 1000
        doc.recompute()
        doc.saveAs("/work/source.FCStd")
    elif op == "cad_inspect":
        # No inspection outside sandbox precedes this open.
        doc = App.openDocument("/input.FCStd")
        doc.recompute()
    else:
        raise ValueError("allowlisted CAD operation required")
    regions = []
    required = set(case["regions"])
    if op == "cad_fixture":
        # This procedural channel's walls are implicit fluid-boundary surfaces.
        required.discard("wall")
    for obj in doc.Objects:
        if obj.Name not in required:
            continue
        if not hasattr(obj, "Shape") or obj.Shape.isNull() or not obj.Shape.isValid():
            raise ValueError("named region lacks valid geometry")
        if len(obj.Shape.Solids) != 1:
            raise ValueError("region must identify one unambiguous solid")
        tolerance = case["geometry_tolerance"]
        if tolerance["unit"] != "m":
            raise ValueError("explicit SI geometry tolerance required")
        mesh = MeshPart.meshFromShape(
            Shape=obj.Shape,
            LinearDeflection=tolerance["value"] * 1000,
            AngularDeflection=0.1,
            Relative=False,
        )
        mesh.write(f"/work/{obj.Name}.stl")
        bounds = obj.Shape.BoundBox
        regions.append(
            {
                "name": obj.Name,
                "label": obj.Label,
                "volume_m3": obj.Shape.Volume * 1e-9,
                "bounds_m": [
                    bounds.XMin / 1000,
                    bounds.XMax / 1000,
                    bounds.YMin / 1000,
                    bounds.YMax / 1000,
                    bounds.ZMin / 1000,
                    bounds.ZMax / 1000,
                ],
                "transform": list(obj.Placement.toMatrix().A),
                "triangles": mesh.CountFacets,
                "source_unit": "mm",
                "stl_scale_to_m": 0.001,
            }
        )
    if {r["name"] for r in regions} != required:
        raise ValueError(
            "missing or ambiguous named regions; face numbering is not accepted"
        )
    atomic_json(
        "/work/regions.json",
        {
            "synthetic": case["geometry"]["synthetic"],
            "regions": regions,
            "gap_healing": False,
            "geometry_tolerance": case["geometry_tolerance"],
        },
    )
    atomic_json(
        f"/work/{op}-receipt.json",
        {
            "adapter": "FreeCAD",
            "version": App.Version(),
            "executed": True,
            "backend": "cpu",
            "software_fallback": False,
            "security_minimum": "1.1.4",
            "sandbox_required": True,
        },
    )
    App.closeDocument(doc.Name)


try:
    main()
except Exception as exc:
    atomic_json("/work/import-error.json", {"error": str(exc)})
    raise
