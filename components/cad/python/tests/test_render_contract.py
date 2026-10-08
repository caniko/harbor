import ctypes as c
import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest


def bridge():
    path = Path(__file__).parents[2] / "adapters/paraview_bridge.py"
    spec = importlib.util.spec_from_file_location("render_bridge", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_velocity_collection_does_not_confuse_material_output_with_scientific_time(
    tmp_path,
):
    module = bridge()
    fields = tmp_path / "tmp/vtkData"
    fields.mkdir(parents=True)
    (fields / "geometry.pvd").write_text("material-only time collection")
    with pytest.raises(ValueError):
        module.velocity_collection(tmp_path)
    channel = fields / "channel.pvd"
    channel.write_text("authoritative velocity/pressure time collection")
    assert module.velocity_collection(tmp_path) == channel
    channel.unlink()
    channel.symlink_to(fields / "geometry.pvd")
    with pytest.raises(ValueError):
        module.velocity_collection(tmp_path)


@pytest.mark.parametrize(
    "display,node,accepted",
    [
        (100, b"/dev/dri/renderD128", True),
        (100, b"/dev/dri/renderD129", False),
        (100, None, False),
        (None, b"/dev/dri/renderD128", False),
    ],
)
def test_actual_egl_display_must_resolve_to_selected_render_node(
    monkeypatch, display, node, accepted
):
    module = bridge()
    # Simulate only the external EGL boundary; the adapter's rejection policy
    # executes normally. Actual context creation is a separate native gate.
    egl = SimpleNamespace(eglGetCurrentDisplay=lambda: display)
    monkeypatch.setattr(module, "egl_library", lambda: egl)

    def query(_display, _attribute, out):
        c.cast(out, c.POINTER(c.c_ssize_t))[0] = 42
        return 1

    functions = {
        b"eglQueryDisplayAttribEXT": query,
        b"eglQueryDeviceStringEXT": lambda *_: node,
    }
    monkeypatch.setattr(module, "egl_proc", lambda _egl, name, *_: functions[name])
    if accepted:
        assert module.egl_context_device("/dev/dri/renderD128") == "/dev/dri/renderD128"
    else:
        with pytest.raises(RuntimeError):
            module.egl_context_device("/dev/dri/renderD128")
