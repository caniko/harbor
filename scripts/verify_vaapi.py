"""Opt-in selected-device VAAPI adapter probe; never qualifies CUDA or B1.

Uses three synthetic PNGs, the packaged media adapter and a bounded owned user
service. Native fields, render identity and physical-time labels are separate
gates. Requires an existing user manager and operator-provided driver access.
"""

import argparse
import hashlib
import json
import os
import re
import stat
import struct
import subprocess
import uuid
import zlib
from pathlib import Path


def packaged(path):
    if ".." in path.parts or not path.is_relative_to("/nix/store"):
        raise ValueError("immutable packaged path required")
    path = path.resolve(strict=True)
    if not path.is_relative_to("/nix/store") or not path.is_file():
        raise ValueError("packaged regular file required")
    return path


def png(path, frame):
    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data))
        )

    pixels = b"".join(
        b"\0" + bytes((frame * 80, row * 4, 128)) * 64 for row in range(64)
    )
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", 64, 64, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(pixels))
        + chunk(b"IEND", b"")
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--media", type=Path, required=True)
    parser.add_argument("--pci", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]", args.pci):
        raise ValueError("canonical PCI identity required")
    media = packaged(args.media)
    runtime = packaged(args.runtime)
    bwrap = packaged(Path(json.loads(runtime.read_text())["bwrap"]))
    alias = Path(f"/dev/dri/by-path/pci-{args.pci}-render")
    node = alias.resolve(strict=True)
    device = node.stat()
    if not stat.S_ISCHR(device.st_mode):
        raise ValueError("DRM character device required")
    physical = Path(
        f"/sys/dev/char/{os.major(device.st_rdev)}:{os.minor(device.st_rdev)}/device"
    ).resolve(strict=True)
    if physical.name != args.pci:
        raise ValueError("DRM node / PCI identity mismatch")
    root = args.output.resolve()
    root.mkdir(parents=True, mode=0o700, exist_ok=False)
    work = root / "work"
    work.mkdir(mode=0o700)
    for frame in range(3):
        png(work / f"frame{frame:04d}.png", frame)
    plan = root / "plan.json"
    plan.write_text(
        json.dumps(
            {
                "case": {"presentation": {"width": 64, "height": 64}},
                "stages": [
                    {
                        "operation": "video",
                        "selection": {
                            "role": "media",
                            "backend": "vaapi",
                            "pci": args.pci,
                        },
                    }
                ],
                "observation": {"retained_times_s": [0, 10, 20]},
            }
        )
    )
    unit = f"harbor-cad-vaapi-qualify-{uuid.uuid4()}.service"
    command = [
        "systemd-run",
        "--user",
        "--wait",
        "--pipe",
        "--collect",
        f"--unit={unit}",
        "--property=MemoryMax=536870912",
        "--property=CPUQuota=100%",
        "--property=TasksMax=128",
        "--property=KillMode=control-group",
        "--property=NoNewPrivileges=yes",
        "--property=RuntimeMaxSec=60",
        str(bwrap),
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--ro-bind",
        "/nix/store",
        "/nix/store",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/home",
        "--dir",
        "/home/worker",
        "--setenv",
        "HOME",
        "/home/worker",
        "--setenv",
        "PATH",
        "/nonexistent",
        "--bind",
        str(work),
        "/work",
        "--chdir",
        "/work",
        "--dev-bind",
        str(node),
        str(node),
        "--dir",
        "/dev/dri/by-path",
        "--symlink",
        str(node),
        str(alias),
        "--ro-bind",
        "/run/opengl-driver",
        "/run/opengl-driver",
        "--ro-bind",
        str(plan),
        "/plan.json",
        "--",
        str(media),
        "video",
        "/plan.json",
    ]
    environment = {
        key: os.environ[key]
        for key in ("HOME", "PATH", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS")
        if key in os.environ
    }
    receipt = None
    try:
        with (root / "process.log").open("w") as log:
            process = subprocess.run(
                command,
                env=environment,
                stdout=log,
                stderr=subprocess.STDOUT,
                timeout=75,
                check=False,
            )
        if process.returncode == 0:
            receipt = json.loads((work / "video-receipt.json").read_text())
            assert receipt["pci"] == args.pci and receipt["backend"] == "vaapi"
            assert receipt["executed"] and not receipt["software_fallback"]
            assert (
                receipt["frames"] == 3 and receipt["metadata"]["codec_name"] == "h264"
            )
        report = {
            "scope": "selected-device sandboxed encoding and CPU decode of three synthetic PNGs; not worker B1, render or time-label evidence",
            "unit": unit,
            "pci": args.pci,
            "node": str(node),
            "media": str(media),
            "runtime": str(runtime),
            "media_sha256": hashlib.sha256(media.read_bytes()).hexdigest(),
            "exit_code": process.returncode,
            "qualified_adapter_probe": process.returncode == 0,
            "receipt": receipt,
            "physical_validation": "unqualified",
        }
        (root / "verification.json").write_text(json.dumps(report, indent=2))
        print(json.dumps(report, indent=2))
        if process.returncode != 0:
            raise RuntimeError(
                f"VAAPI probe failed; retained log {root / 'process.log'}"
            )
    finally:
        subprocess.run(
            ["systemctl", "--user", "stop", unit],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=15,
            check=False,
        )


if __name__ == "__main__":
    main()
