"""Numerical filtering is deliberately separate from this EGL render stage."""

import ctypes as c
import json
import os
import sys
from pathlib import Path


def egl_device(render_node):
    egl = c.CDLL("@libegl@")
    egl.eglGetProcAddress.argtypes = [c.c_char_p]
    egl.eglGetProcAddress.restype = c.c_void_p
    query = c.CFUNCTYPE(c.c_uint, c.c_int, c.POINTER(c.c_void_p), c.POINTER(c.c_int))(
        egl.eglGetProcAddress(b"eglQueryDevicesEXT")
    )
    name = c.CFUNCTYPE(c.c_char_p, c.c_void_p, c.c_int)(
        egl.eglGetProcAddress(b"eglQueryDeviceStringEXT")
    )
    devices = (c.c_void_p * 32)()
    count = c.c_int()
    if not query(32, devices, c.byref(count)):
        raise RuntimeError("EGL device enumeration failed")
    matches = []
    for i in range(count.value):
        node = name(devices[i], 0x3377)  # EGL_DRM_RENDER_NODE_FILE_EXT
        if node and os.path.realpath(node.decode()) == os.path.realpath(render_node):
            matches.append(i)
    if len(matches) != 1:
        raise RuntimeError("ambiguous/stale EGL device-to-DRM identity")
    return matches[0]


def main():
    if sys.argv[1] != "render":
        raise ValueError("render operation required")
    plan = json.loads(Path(sys.argv[2]).read_text())
    stage = next(s for s in plan["stages"] if s["operation"] == "render")
    pci = stage["selection"]["pci"]
    node = f"/dev/dri/by-path/pci-{pci}-render"
    index = egl_device(node)
    os.environ["VTK_EGL_DEVICE_INDEX"] = str(index)
    os.environ["VTK_DEFAULT_OPENGL_WINDOW"] = "vtkEGLRenderWindow"
    from paraview import simple as pv

    fields = sorted(Path("/work/tmp/vtkData").glob("*.pvd"))
    if len(fields) != 1:
        raise ValueError("one authoritative OpenLB time collection required")
    source = pv.OpenDataFile(str(fields[0]))
    view = pv.CreateView("RenderView")
    view.ViewSize = [
        plan["case"]["presentation"]["width"],
        plan["case"]["presentation"]["height"],
    ]
    view.CameraPosition = plan["case"]["presentation"]["camera"]
    display = pv.Show(source, view)
    pv.ColorBy(display, ("POINTS", "physVelocity"))
    lookup = pv.GetColorTransferFunction("physVelocity")
    lookup.RescaleTransferFunction(*plan["case"]["presentation"]["range"])
    display.SetScalarBarVisibility(view, True)
    scalar_bar = pv.GetScalarBar(lookup, view)
    scalar_bar.Title = "Velocity (m/s)"
    scalar_bar.ComponentTitle = "Magnitude"
    label = pv.Text()
    pv.Show(label, view)
    retained = plan["observation"]["retained_times_s"]
    native_receipt = json.loads(Path("/work/openlb-receipt.json").read_text())
    time_mapping = {t["requested_s"]: t for t in native_receipt["retained_times"]}
    available = list(getattr(source, "TimestepValues", []))
    if any(
        t not in time_mapping or time_mapping[t]["step"] not in available
        for t in retained
    ):
        raise ValueError(
            "requested physical times were not retained; solving is not a render operation"
        )
    for i, time_s in enumerate(retained):
        label.Text = f"Synthetic channel | physical time {time_mapping[time_s]['observed_s']:.9g} s"
        view.ViewTime = time_mapping[time_s]["step"]
        source.UpdatePipeline(view.ViewTime)
        pv.Render(view)
        window = view.GetClientSideObject().GetRenderWindow()
        capabilities = window.ReportCapabilities()
        if (
            window.GetClassName() != "vtkEGLRenderWindow"
            or window.GetDeviceIndex() != index
        ):
            raise RuntimeError(
                "observed EGL context device differs from selected device"
            )
        if any(
            x in capabilities.lower()
            for x in ["llvmpipe", "softpipe", "swrast", "software rasterizer"]
        ):
            raise RuntimeError("software graphics fallback rejected")
        pv.SaveScreenshot(f"/work/frame{i:04d}.png", view)
    receipt = {
        "adapter": "ParaView",
        "backend": "egl",
        "pci": pci,
        "egl_device_index": index,
        "context": capabilities,
        "executed": True,
        "software_fallback": False,
        "physical_times_s": retained,
        "numerical_filter": False,
        "time_mapping": list(time_mapping.values()),
        "fixed_range": plan["case"]["presentation"]["range"],
        "units": "m/s",
        "physical_time_labels": True,
        "physical_validation": "unqualified",
    }
    Path("/work/render-receipt.json.partial").write_text(json.dumps(receipt, indent=2))
    Path("/work/render-receipt.json.partial").replace("/work/render-receipt.json")


if __name__ == "__main__":
    main()
