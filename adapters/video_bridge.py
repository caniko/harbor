"""Fixed hardware encoding plus real decode, without any codec fallback."""

import json
import subprocess
import sys
from pathlib import Path


def main():
    if sys.argv[1] != "video":
        raise ValueError("video operation required")
    plan = json.loads(Path(sys.argv[2]).read_text())
    selection = next(
        s["selection"] for s in plan["stages"] if s["operation"] == "video"
    )
    if selection["backend"] != "vaapi":
        raise ValueError("only explicitly selected VAAPI adapter implemented")
    frames = sorted(Path("/work").glob("frame[0-9][0-9][0-9][0-9].png"))
    if not frames or len(frames) > 1024:
        raise ValueError("bounded retained frame sequence required")
    if any(p.name != f"frame{i:04d}.png" for i, p in enumerate(frames)):
        raise ValueError("no implicit missing-frame interpolation")
    node = f"/dev/dri/by-path/pci-{selection['pci']}-render"
    command = [
        "@ffmpeg@",
        "-nostdin",
        "-v",
        "verbose",
        "-init_hw_device",
        f"vaapi=media:{node}",
        "-filter_hw_device",
        "media",
        "-framerate",
        "24",
        "-i",
        "/work/frame%04d.png",
        "-vf",
        "format=nv12,hwupload",
        "-c:v",
        "h264_vaapi",
        "-frames:v",
        str(len(frames)),
        "-f",
        "mp4",
        "/work/video.partial",
    ]
    # Logs flow to the worker's bounded native log, never a full captured array.
    subprocess.run(command, check=True)
    subprocess.run(
        [
            "@ffmpeg@",
            "-nostdin",
            "-v",
            "error",
            "-i",
            "/work/video.partial",
            "-f",
            "null",
            "-",
        ],
        check=True,
    )
    metadata = json.loads(
        subprocess.check_output(
            [
                "@ffprobe@",
                "-v",
                "error",
                "-count_frames",
                "-show_frames",
                "-show_entries",
                "stream=codec_name,pix_fmt,nb_read_frames,r_frame_rate,width,height:frame=best_effort_timestamp_time",
                "-of",
                "json",
                "/work/video.partial",
            ]
        )
    )
    stream = metadata["streams"][0]
    if int(stream["nb_read_frames"]) != len(frames):
        raise RuntimeError("encoded/decoded frame count mismatch")
    presentation = plan["case"]["presentation"]
    if (
        stream["width"] != presentation["width"]
        or stream["height"] != presentation["height"]
    ):
        raise RuntimeError("encoded image dimensions differ from approved presentation")
    timestamps = [float(f["best_effort_timestamp_time"]) for f in metadata["frames"]]
    if len(timestamps) != len(frames) or any(
        abs(t - i / 24) > 1e-5 for i, t in enumerate(timestamps)
    ):
        raise RuntimeError(
            "decoded presentation timestamps differ from fixed 24 fps sequence"
        )
    Path("/work/video.partial").replace("/work/video.mp4")
    receipt = {
        "adapter": "FFmpeg",
        "backend": "vaapi",
        "encoder": "h264_vaapi",
        "pci": selection["pci"],
        "render_node": node,
        "executed": True,
        "software_fallback": False,
        "decode": "CPU verification",
        "frames": len(frames),
        "metadata": stream,
        "scientific_arrays": "native fields retained independently of lossy video",
        "physical_times_s": plan["observation"]["retained_times_s"],
        "presentation_fps": 24,
        "presentation_timestamps_s": timestamps,
        "physical_time_labels": "rendered into frames; presentation clock is independent",
    }
    Path("/work/video-receipt.json.partial").write_text(json.dumps(receipt, indent=2))
    Path("/work/video-receipt.json.partial").replace("/work/video-receipt.json")


if __name__ == "__main__":
    main()
