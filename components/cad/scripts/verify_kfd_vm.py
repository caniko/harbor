"""Bounded raw KFD VM checks on the running kernel; no queues or numerical work."""

import argparse
import errno
import fcntl
import hashlib
import json
import os
import struct
import subprocess
import sys
from pathlib import Path


def child(gpu_id, render_node):
    # Linux UAPI: _IOR('K', 0x01, {u32,u32}) and
    # _IOW('K', 0x15, {drm_fd:u32,gpu_id:u32}). x86_64 ioctl layout.
    version_ioctl = 0x80084B01
    acquire_ioctl = 0x40084B15
    with open("/dev/kfd", "rb", buffering=0) as kfd:
        version = bytearray(8)
        fcntl.ioctl(kfd, version_ioctl, version, True)
        major, minor = struct.unpack("=II", version)
        if major != 1:
            raise ValueError("unsupported KFD UAPI major")
        records = []
        with open("/dev/null", "rb", buffering=0) as null:
            for label, drm_fd, selected in (
                ("invalid_fd", 0xFFFFFFFF, gpu_id),
                ("non_drm_fd", null.fileno(), gpu_id),
            ):
                try:
                    fcntl.ioctl(
                        kfd, acquire_ioctl, struct.pack("=II", drm_fd, selected)
                    )
                    result = 0
                except OSError as error:
                    result = error.errno
                if result != errno.EINVAL:
                    raise ValueError(
                        f"raw KFD {label} did not reject with EINVAL: {result}"
                    )
                records.append({"test": label, "errno": result})
        if Path(render_node).exists():
            with open(render_node, "r+b", buffering=0) as render:
                try:
                    fcntl.ioctl(
                        kfd, acquire_ioctl, struct.pack("=II", render.fileno(), 0)
                    )
                    result = 0
                except OSError as error:
                    result = error.errno
                if result != errno.EINVAL:
                    raise ValueError(
                        "unavailable KFD GPU ID did not reject with EINVAL"
                    )
                records.append({"test": "unavailable_gpu_id", "errno": result})
                fcntl.ioctl(
                    kfd, acquire_ioctl, struct.pack("=II", render.fileno(), gpu_id)
                )
                records.append({"test": "exact_drm_fd_and_gpu_id", "errno": 0})
        print(
            json.dumps(
                {
                    "uapi": {"major": major, "minor": minor},
                    "records": records,
                    "render_node_accessible": Path(render_node).exists(),
                }
            )
        )


def main():
    if len(sys.argv) == 4 and sys.argv[1] == "--child":
        child(int(sys.argv[2]), sys.argv[3])
        return
    parser = argparse.ArgumentParser(description=__doc__)
    # Aliases let the existing card-owning bounded HIP probe launch this fixture.
    parser.add_argument(
        "--bwrap", "--executable", dest="bwrap", type=Path, required=True
    )
    parser.add_argument(
        "--python", "--cpu-executable", dest="python", type=Path, required=True
    )
    parser.add_argument("--pci", required=True)
    parser.add_argument("--uuid", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if os.uname().machine != "x86_64":
        raise ValueError(
            "raw KFD fixture is qualified only for the x86_64 ioctl layout"
        )
    root = args.output.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    nodes = Path("/sys/devices/virtual/kfd/kfd/topology/nodes")
    gpu_nodes = [p for p in nodes.iterdir() if int((p / "gpu_id").read_text()) != 0]
    if len(gpu_nodes) != 1:
        raise ValueError(
            "single live KFD GPU required; another GPU remains unqualified"
        )
    properties = dict(
        line.split() for line in (gpu_nodes[0] / "properties").read_text().splitlines()
    )
    unique = int(properties["unique_id"])
    expected_uuid = "GPU-" + f"{unique:016x}".encode("ascii").hex()
    drm_node = Path(f"/dev/dri/renderD{properties['drm_render_minor']}")
    if (
        expected_uuid != args.uuid
        or (Path("/sys/class/drm") / drm_node.name / "device").resolve().name
        != args.pci
    ):
        raise ValueError("approved raw KFD PCI/UUID identity mismatch")
    # The guarded outer OpenLB HIP probe owns the card anchor. Each child starts
    # in a fresh process with no inherited DRM descriptors or HIP runtime.
    bwrap = args.bwrap.resolve(strict=True)
    python = args.python.resolve(strict=True)
    if not bwrap.is_relative_to("/nix/store") or python != Path(sys.executable).resolve(
        strict=True
    ):
        raise ValueError("exact immutable bubblewrap/Python required")
    reports = []
    for mounted in (False, True):
        command = [
            str(bwrap),
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--cap-drop",
            "ALL",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--tmpfs",
            "/tmp",
            "--ro-bind",
            "/nix/store",
            "/nix/store",
            "--ro-bind",
            str(Path(__file__).resolve()),
            "/probe.py",
            "--dev-bind",
            "/dev/kfd",
            "/dev/kfd",
        ]
        if mounted:
            command.extend(["--dev-bind", str(drm_node), str(drm_node)])
        command.extend(
            [
                sys.executable,
                "/probe.py",
                "--child",
                (gpu_nodes[0] / "gpu_id").read_text().strip(),
                str(drm_node),
            ]
        )
        result = subprocess.run(
            command, env={}, capture_output=True, text=True, check=False, timeout=30
        )
        record = {
            "command": command,
            "exit_code": result.returncode,
            "stderr": result.stderr,
            "stdout": result.stdout,
            "render_node_mounted": mounted,
        }
        (root / f"child-{int(mounted)}.json").write_text(json.dumps(record, indent=2))
        if result.returncode:
            raise RuntimeError("raw KFD child failed; exact output retained")
        observed = json.loads(result.stdout)
        if observed["render_node_accessible"] != mounted:
            raise ValueError("unexpected render-node access")
        record["observed"] = observed
        reports.append(record)
    report = {
        "scope": "running-kernel raw VM acquisition with exact selected DRM FD; no second-GPU exclusion claim",
        "kernel_release": os.uname().release,
        "kernel_source_matched": False,
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "pci": args.pci,
        "backend_uuid": args.uuid,
        "reports": reports,
    }
    (root / "verification.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
