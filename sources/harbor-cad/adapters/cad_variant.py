"""Fixed native Part::Box allowlist; caller code and property names are never inputs."""

import math


def lengths(request):
    values = []
    for quantity in request["dimensions"]:
        factor = {"m": 1.0, "mm": 0.001, "nm": 1e-9}.get(quantity["unit"])
        if factor is None:
            raise ValueError("explicit SI-capable variant lengths required")
        value = quantity["value"] * factor
        if (
            isinstance(quantity["value"], bool)
            or not math.isfinite(value)
            or not 1e-5 <= value <= 10.0
        ):
            raise ValueError("bounded native box dimensions required")
        values.append(value)
    if len(values) != 3 or max(values) / min(values) > 1000:
        raise ValueError("three resolved bounded box dimensions required")
    return values


def geometry(obj):
    bounds = obj.Shape.BoundBox
    values = {
        "bounds_m": [
            value * 0.001
            for value in (
                bounds.XMin,
                bounds.XMax,
                bounds.YMin,
                bounds.YMax,
                bounds.ZMin,
                bounds.ZMax,
            )
        ],
        "volume_m3": obj.Shape.Volume * 1e-9,
        "transform": list(obj.Placement.toMatrix().A),
        "dimensions_m": [
            float(obj.Length) * 0.001,
            float(obj.Width) * 0.001,
            float(obj.Height) * 0.001,
        ],
    }
    if any(
        not math.isfinite(value)
        for sequence in values.values()
        for value in (sequence if isinstance(sequence, list) else [sequence])
    ):
        raise ValueError("finite native CAD geometry required")
    return values


def apply(doc, spec, tolerance_m):
    """Validate the complete original context before editing only fixed dimensions."""
    name = spec["request"]["region_name"]
    if len(doc.Objects) != 1:
        raise ValueError("single-object native box document required")
    obj = doc.Objects[0]
    if (
        obj.Name != name
        or obj.TypeId != "Part::Box"
        or obj.getParentGeoFeatureGroup() is not None
        or obj.ExpressionEngine
    ):
        raise ValueError(
            "one named top-level native Part::Box without expressions required"
        )
    if obj.Shape.isNull() or not obj.Shape.isValid() or len(obj.Shape.Solids) != 1:
        raise ValueError("one valid native solid required before recompute")
    original = geometry(obj)
    source = spec["source"]["geometry"]
    if (
        original["transform"] != source["source_transform"]
        or any(
            abs(a - b) > tolerance_m
            for a, b in zip(original["bounds_m"], source["bounds_m"], strict=True)
        )
        or abs(original["volume_m3"] - source["volume_m3"])
        > 1e-10 * source["volume_m3"]
    ):
        raise ValueError("native original box differs from registered CAD geometry")
    if any(
        abs(
            original["dimensions_m"][axis]
            - (source["bounds_m"][2 * axis + 1] - source["bounds_m"][2 * axis])
        )
        > tolerance_m
        for axis in range(3)
    ):
        raise ValueError("axis-aligned primitive dimension context required")
    expected = lengths(spec["request"])
    obj.Length, obj.Width, obj.Height = (value * 1000.0 for value in expected)
    doc.recompute()
    if (
        obj.ExpressionEngine
        or obj.Shape.isNull()
        or not obj.Shape.isValid()
        or len(obj.Shape.Solids) != 1
    ):
        raise ValueError(
            "recomputed native variant must remain one valid primitive solid"
        )
    changed = geometry(obj)
    bounds = list(source["bounds_m"])
    for axis in range(3):
        bounds[2 * axis + 1] = bounds[2 * axis] + expected[axis]
    if (
        changed["transform"] != original["transform"]
        or any(
            abs(a - b) > tolerance_m
            for a, b in zip(changed["bounds_m"], bounds, strict=True)
        )
        or any(
            abs(a - b) > tolerance_m
            for a, b in zip(changed["dimensions_m"], expected, strict=True)
        )
        or abs(changed["volume_m3"] - math.prod(expected)) > 1e-10 * math.prod(expected)
    ):
        raise ValueError(
            "recomputed native box differs from approved dimensions or placement"
        )
    return {
        "object_type": "Part::Box",
        "before": original,
        "after": changed,
        "gap_healing": False,
        "expressions": False,
    }
