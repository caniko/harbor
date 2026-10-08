"""Fixed controlled FCStd fixtures for opt-in importer/OCC correspondence tests.

No user-provided native documents or script paths are loaded by this generator.
"""

import hashlib
import json
from pathlib import Path

import FreeCAD as App
import Part


def main():
    if tuple(int(v) for v in App.Version()[:3]) != (1, 1, 4):
        raise ValueError("exact security-patched FreeCAD fixture ABI required")
    root = Path("/work")
    if list(root.iterdir()):
        raise ValueError("new empty controlled fixture output required")
    records = []
    for label, position in [
        ("origin", (0.0, 0.0, 0.0)),
        ("translated", (100.0, -20.0, 300.0)),
    ]:
        doc = App.newDocument(label)
        try:
            solid = doc.addObject("Part::Box", "solid")
            solid.Length, solid.Width, solid.Height = 20.0, 10.0, 10.0
            solid.Placement = App.Placement(App.Vector(*position), App.Rotation())
            doc.recompute()
            if not solid.Shape.isValid() or len(solid.Shape.Solids) != 1:
                raise ValueError("controlled closed box required")
            source = root / f"{label}.FCStd"
            doc.saveAs(str(source))
            bounds = solid.Shape.BoundBox
            records.append(
                {
                    "label": label,
                    "source": source.name,
                    "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                    "synthetic": True,
                    "region_name": "solid",
                    "size_m": [0.02, 0.01, 0.01],
                    "bounds_m": [
                        bounds.XMin * 0.001,
                        bounds.XMax * 0.001,
                        bounds.YMin * 0.001,
                        bounds.YMax * 0.001,
                        bounds.ZMin * 0.001,
                        bounds.ZMax * 0.001,
                    ],
                    "volume_m3": solid.Shape.Volume * 1e-9,
                    "source_transform": list(solid.Placement.toMatrix().A),
                    "source_unit": "mm",
                    "scale_to_m": 0.001,
                    "placement_translation_unit": "mm",
                }
            )
        finally:
            App.closeDocument(doc.Name)
    negatives = {}
    for label, shape in [
        (
            "two-solids",
            Part.makeCompound(
                [
                    Part.makeBox(20.0, 10.0, 10.0),
                    Part.makeBox(20.0, 10.0, 10.0, App.Vector(30.0, 0.0, 0.0)),
                ]
            ),
        ),
        ("curved", Part.makeCylinder(5.0, 10.0)),
    ]:
        path = root / f"{label}.brep"
        shape.exportBrep(str(path))
        negatives[label] = {
            "path": path.name,
            "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        }
    (root / "fixture-manifest.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "freecad_version": "1.1.4",
                "fixtures": records,
                "negative_breps": negatives,
                "synthetic": True,
            },
            allow_nan=False,
            indent=2,
        )
    )


main()
