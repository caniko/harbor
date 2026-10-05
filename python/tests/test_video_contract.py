import hashlib
import importlib.util
import json
from pathlib import Path

import pytest


def sequence(tmp_path, times=(0, 10, 20)):
    frames = []
    for index, time in enumerate(times):
        path = tmp_path / f"frame{index:04d}.png"
        payload = f"closed frame at {time} s".encode()
        path.write_bytes(payload)
        frames.append(
            {
                "path": path.name,
                "bytes": len(payload),
                "sha256": hashlib.sha256(payload).hexdigest(),
                "requested_s": time,
                "observed_s": time,
            }
        )
    manifest = {"schema_version": 1, "source": "synthetic_fixture", "frames": frames}
    (tmp_path / "frame-sequence.json").write_text(json.dumps(manifest))


def run_video(monkeypatch, tmp_path, times=(0, 10, 20)):
    path = Path(__file__).parents[2] / "adapters/video_bridge.py"
    spec = importlib.util.spec_from_file_location("video_bridge", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    plan = tmp_path / "plan.json"
    plan.write_text(
        json.dumps(
            {
                "case": {
                    "geometry": {"synthetic": True},
                    "presentation": {"width": 640, "height": 480},
                },
                "stages": [
                    {
                        "operation": "video",
                        "selection": {"backend": "vaapi", "pci": "0000:03:00.0"},
                    }
                ],
                "observation": {"retained_times_s": list(times)},
            }
        )
    )
    monkeypatch.setattr(module.sys, "argv", [str(path), "video", str(plan)])
    monkeypatch.setattr(
        module,
        "Path",
        lambda value: (
            tmp_path
            if value == "/work"
            else Path(str(value).replace("/work/", str(tmp_path) + "/"))
        ),
    )
    replace = Path.replace
    monkeypatch.setattr(
        Path,
        "replace",
        lambda path, target: replace(
            path, Path(str(target).replace("/work/", str(tmp_path) + "/"))
        ),
    )
    encoded = []

    def native(command, **_):
        if "h264_vaapi" in command:
            encoded.append(True)
            (tmp_path / "video.partial").write_bytes(b"encoded fixture")

    monkeypatch.setattr(module.subprocess, "run", native)

    def metadata(*_, **__):
        count = len(list(tmp_path.glob("frame*.png")))
        return json.dumps(
            {
                "streams": [
                    {
                        "codec_name": "h264",
                        "pix_fmt": "yuv420p",
                        "nb_read_frames": str(count),
                        "width": 640,
                        "height": 480,
                    }
                ],
                "frames": [
                    {"best_effort_timestamp_time": str(i / 24)} for i in range(count)
                ],
            }
        ).encode()

    monkeypatch.setattr(module.subprocess, "check_output", metadata)
    return module, encoded


@pytest.mark.parametrize(
    "damage",
    [
        "missing_tail",
        "extra",
        "changed",
        "wrong_time",
        "missing_manifest",
        "symlink",
        "nonfinite",
    ],
)
def test_video_rejects_incomplete_or_unbound_frames_before_encoding(
    monkeypatch, tmp_path, damage
):
    sequence(tmp_path)
    if damage == "missing_tail":
        (tmp_path / "frame0002.png").unlink()
    elif damage == "extra":
        (tmp_path / "frame0003.png").write_bytes(b"extra")
    elif damage == "changed":
        (tmp_path / "frame0001.png").write_bytes(b"altered")
    elif damage == "wrong_time":
        manifest = json.loads((tmp_path / "frame-sequence.json").read_text())
        manifest["frames"][1]["requested_s"] = 5
        (tmp_path / "frame-sequence.json").write_text(json.dumps(manifest))
    elif damage == "missing_manifest":
        (tmp_path / "frame-sequence.json").unlink()
    elif damage == "nonfinite":
        manifest = json.loads((tmp_path / "frame-sequence.json").read_text())
        manifest["frames"][1]["observed_s"] = float("nan")
        (tmp_path / "frame-sequence.json").write_text(json.dumps(manifest))
    else:
        (tmp_path / "frame0001.png").rename(tmp_path / "outside.png")
        (tmp_path / "frame0001.png").symlink_to(tmp_path / "outside.png")
    module, encoded = run_video(monkeypatch, tmp_path)
    with pytest.raises((ValueError, OSError)):
        module.main()
    assert not encoded
    assert not (tmp_path / "video-receipt.json").exists()


def test_video_preserves_selected_physical_times_and_independent_playback_clock(
    monkeypatch, tmp_path
):
    sequence(tmp_path, (0, 20))
    module, encoded = run_video(monkeypatch, tmp_path, (0, 20))
    module.main()
    receipt = json.loads((tmp_path / "video-receipt.json").read_text())
    assert encoded and receipt["frames"] == 2
    assert receipt["physical_times_s"] == [0, 20]
    assert receipt["presentation_timestamps_s"] == [0, 1 / 24]
    assert (
        receipt["frame_sequence_sha256"]
        == hashlib.sha256((tmp_path / "frame-sequence.json").read_bytes()).hexdigest()
    )


@pytest.mark.parametrize(
    "damage", [None, "missing_receipt", "changed_receipt", "changed_sequence"]
)
def test_native_frames_are_bound_to_completed_render(monkeypatch, tmp_path, damage):
    sequence(tmp_path)
    manifest_path = tmp_path / "frame-sequence.json"
    manifest = json.loads(manifest_path.read_text())
    manifest["source"] = "rendered_fields"
    manifest_path.write_text(json.dumps(manifest))
    receipt = {
        "adapter": "ParaView",
        "executed": True,
        "software_fallback": False,
        "frame_sequence_sha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
        "frames": manifest["frames"],
        "physical_times_s": [0, 10, 20],
    }
    if damage == "changed_receipt":
        receipt["frames"][1]["observed_s"] = 30
    if damage != "missing_receipt":
        (tmp_path / "render-receipt.json").write_text(json.dumps(receipt))
    if damage == "changed_sequence":
        manifest_path.write_text(json.dumps(manifest, indent=2))
    module, encoded = run_video(monkeypatch, tmp_path)
    if damage is None:
        module.main()
        assert encoded
    else:
        with pytest.raises((ValueError, OSError)):
            module.main()
        assert not encoded
