"""Local compatibility regressions, executed against the patched source."""

import ast
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock


def source_function(path, name, namespace):
    tree = ast.parse(Path(path).read_text())
    function = next(
        n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef) and n.name == name
    )
    exec(compile(ast.Module(body=[function], type_ignores=[]), path, "exec"), namespace)  # noqa: S102 - isolated patched-source fixture
    return namespace[name]


def test_zero_area_never_calls_native_capture():
    capture = Mock(side_effect=AssertionError("native capture must not run"))
    area = SimpleNamespace(type="VIEW_3D", width=0, height=400)
    bpy = SimpleNamespace(
        context=SimpleNamespace(screen=SimpleNamespace(areas=[area])),
        ops=SimpleNamespace(screen=SimpleNamespace(screenshot_area=capture)),
    )
    screenshot = source_function(
        "blender_extension/bridge/scene.py", "get_viewport_screenshot", {"bpy": bpy}
    )
    for width, height in [(0, 400), (400, 0), (-1, 400)]:
        area.width, area.height = width, height
        result = screenshot(None, filepath="unused.png")
        assert "no drawable area" in result["error"]
    capture.assert_not_called()


def test_tuple_only_vector_is_serialized():
    class Vector:
        def to_tuple(self):
            return (4.0, 5.0, 6.0)

    unrelated = type("UnrelatedBlenderType", (), {})
    namespace = {
        "bpy": SimpleNamespace(
            types=SimpleNamespace(
                Node=unrelated, NodeSocket=unrelated, ID=unrelated, bpy_struct=unrelated
            )
        )
    }
    convert = source_function(
        "blender_extension/nodes/serialization.py", "_gn_json_value", namespace
    )
    assert convert(Vector()) == [4.0, 5.0, 6.0]
