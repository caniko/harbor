"""Exercise the fixed importer script with only the FreeCAD boundary substituted."""

import json
import runpy
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest


def inspect(monkeypatch, tmp_path, regions, objects):
    plan = tmp_path / "plan.json"
    plan.write_text(
        json.dumps(
            {
                "case": {
                    "regions": regions,
                    "geometry": {"synthetic": False},
                    "geometry_tolerance": {"value": 1e-5, "unit": "m"},
                }
            }
        )
    )
    document = SimpleNamespace(Name="Imported", Objects=objects, recompute=lambda: None)
    app = SimpleNamespace(
        Version=lambda: ["1", "1", "4"],
        ParamGet=lambda _: SimpleNamespace(SetBool=lambda *_: None),
        openDocument=lambda _: document,
        closeDocument=lambda _: None,
    )
    mesh = SimpleNamespace(CountFacets=12, write=lambda _: None)
    monkeypatch.setitem(sys.modules, "FreeCAD", app)
    monkeypatch.setitem(
        sys.modules,
        "harbor_cad_import_policy",
        SimpleNamespace(
            verify_import_environment=lambda _: {"policy": "harbor-cad-importer-v1"}
        ),
    )
    monkeypatch.setitem(
        sys.modules, "MeshPart", SimpleNamespace(meshFromShape=lambda **_: mesh)
    )
    monkeypatch.setenv("HARBOR_CAD_OPERATION", "cad_inspect")
    monkeypatch.setenv("HARBOR_CAD_PLAN", str(plan))
    write = Path.write_text
    replace = Path.replace
    monkeypatch.setattr(
        Path,
        "write_text",
        lambda path, text: write(tmp_path / path.name, text),
    )
    monkeypatch.setattr(
        Path,
        "replace",
        lambda path, destination: replace(
            tmp_path / path.name, tmp_path / Path(destination).name
        ),
    )
    runpy.run_path(str(Path(__file__).parents[2] / "adapters/freecad_bridge.py"))


def solid(name):
    return SimpleNamespace(
        Name=name,
        Label=name,
        Shape=SimpleNamespace(
            isNull=lambda: False,
            isValid=lambda: True,
            Solids=[object()],
            Volume=1000,
            BoundBox=SimpleNamespace(XMin=0, XMax=10, YMin=0, YMax=10, ZMin=0, ZMax=10),
        ),
        Placement=SimpleNamespace(toMatrix=lambda: SimpleNamespace(A=[1] * 16)),
    )


@pytest.mark.parametrize(
    "regions,objects", [(["wall"], []), (["fluid", "wall"], [solid("fluid")])]
)
def test_inspection_rejects_every_missing_requested_region(
    monkeypatch, tmp_path, regions, objects
):
    with pytest.raises(ValueError, match="missing or ambiguous named regions"):
        inspect(monkeypatch, tmp_path, regions, objects)
    assert not (tmp_path / "cad_inspect-receipt.json").exists()


def test_inspection_keeps_an_explicit_wall_solid(monkeypatch, tmp_path):
    inspect(monkeypatch, tmp_path, ["wall"], [solid("wall")])
    report = json.loads((tmp_path / "regions.json").read_text())
    assert [region["name"] for region in report["regions"]] == ["wall"]
    assert report["regions"][0]["volume_m3"] == pytest.approx(1e-6, rel=1e-14, abs=0)
    assert json.loads((tmp_path / "cad_inspect-receipt.json").read_text())["executed"]
