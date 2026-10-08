"""Fixed edit allowlist/refusals; mocks establish no native CAD execution."""

import copy
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest


def module():
    path = Path(__file__).parents[2] / "adapters/cad_variant.py"
    source = importlib.util.spec_from_file_location("cad_variant", path)
    result = importlib.util.module_from_spec(source)
    source.loader.exec_module(result)
    return result


class Document:
    def __init__(self):
        self.recomputes = 0
        self.object = SimpleNamespace(
            Name="solid",
            TypeId="Part::Box",
            ExpressionEngine=[],
            getParentGeoFeatureGroup=lambda: None,
            Length=20.0,
            Width=10.0,
            Height=10.0,
            Placement=SimpleNamespace(
                toMatrix=lambda: SimpleNamespace(
                    A=[
                        1.0,
                        0.0,
                        0.0,
                        100.0,
                        0.0,
                        1.0,
                        0.0,
                        -20.0,
                        0.0,
                        0.0,
                        1.0,
                        300.0,
                        0.0,
                        0.0,
                        0.0,
                        1.0,
                    ]
                )
            ),
        )
        self.Objects = [self.object]
        self.geometry()

    def geometry(self):
        obj = self.object
        obj.Shape = SimpleNamespace(
            Solids=[1],
            isNull=lambda: False,
            isValid=lambda: True,
            Volume=obj.Length * obj.Width * obj.Height,
            BoundBox=SimpleNamespace(
                XMin=100.0,
                XMax=100.0 + obj.Length,
                YMin=-20.0,
                YMax=-20.0 + obj.Width,
                ZMin=300.0,
                ZMax=300.0 + obj.Height,
            ),
        )

    def recompute(self):
        self.recomputes += 1
        self.geometry()


def specification():
    return {
        "request": {
            "region_name": "solid",
            "dimensions": [
                {"value": 30.0, "unit": "mm"},
                {"value": 20.0, "unit": "mm"},
                {"value": 10.0, "unit": "mm"},
            ],
        },
        "source": {
            "geometry": {
                "source_transform": [
                    1.0,
                    0.0,
                    0.0,
                    100.0,
                    0.0,
                    1.0,
                    0.0,
                    -20.0,
                    0.0,
                    0.0,
                    1.0,
                    300.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ],
                "bounds_m": [0.1, 0.12, -0.02, -0.01, 0.3, 0.31],
                "volume_m3": 2e-6,
            }
        },
    }


def test_only_fixed_dimensions_change_with_original_placement_and_recompute():
    native = module()
    doc = Document()
    spec = specification()
    original = copy.deepcopy(spec)
    report = native.apply(doc, spec, 1e-6)
    assert doc.recomputes == 1 and spec == original
    assert [doc.object.Length, doc.object.Width, doc.object.Height] == [
        30.0,
        20.0,
        10.0,
    ]
    assert report["before"]["bounds_m"] == [0.1, 0.12, -0.02, -0.01, 0.3, 0.31]
    assert report["after"]["bounds_m"] == [0.1, 0.13, -0.02, 0.0, 0.3, 0.31]
    assert report["before"]["transform"] == report["after"]["transform"]
    assert report["after"]["volume_m3"] == pytest.approx(6e-6)
    assert not report["gap_healing"] and not report["expressions"]


@pytest.mark.parametrize(
    "drift",
    [
        "type",
        "name",
        "expressions",
        "assembly",
        "extra",
        "shape",
        "bounds",
        "unit",
        "nonfinite",
    ],
)
def test_foreign_objects_expressions_and_original_context_refuse_before_recompute(
    drift,
):
    native, doc, spec = module(), Document(), specification()
    if drift == "type":
        doc.object.TypeId = "App::FeaturePython"
    elif drift == "name":
        doc.object.Name = "other"
    elif drift == "expressions":
        doc.object.ExpressionEngine = [("Length", "Spreadsheet.A1")]
    elif drift == "assembly":
        doc.object.getParentGeoFeatureGroup = lambda: object()
    elif drift == "extra":
        doc.Objects.append(object())
    elif drift == "shape":
        doc.object.Shape.Solids = [1, 2]
    elif drift == "bounds":
        spec["source"]["geometry"]["bounds_m"][0] = 0.2
    elif drift == "unit":
        spec["request"]["dimensions"][0]["unit"] = "kg"
    else:
        doc.object.Shape.Volume = float("nan")
    with pytest.raises(ValueError):
        native.apply(doc, spec, 1e-6)
    assert doc.recomputes == 0
    assert [doc.object.Length, doc.object.Width, doc.object.Height] == [
        20.0,
        10.0,
        10.0,
    ]
