"""Sandboxed thin native launch with independently reconstructed cooling originals."""

import hashlib
import os
import stat
import subprocess
import sys
from pathlib import Path

from retained_cooling_reference import common, normalize, read_regular, verify

POLICY = "harbor-cad-retained-cooling-cpu-v1"


def validate_envelope(envelope, original):
    if (
        set(envelope)
        != {
            "schema_version",
            "native_request",
            "original_sha256",
            "original_bytes",
            "maximum_relative_conservation_error",
        }
        or type(envelope["schema_version"]) is not int
        or envelope["schema_version"] != 1
        or type(envelope["original_bytes"]) is not int
        or envelope["original_bytes"] != len(original)
        or envelope["original_sha256"] != hashlib.sha256(original).hexdigest()
        or not 0
        < common.number(envelope["maximum_relative_conservation_error"])
        <= 1e-10
    ):
        raise ValueError(
            "strict source-bound original byte identity and unchanged conservative native cooling approval required"
        )
    return normalize(envelope["native_request"], original)


def fresh_output(root):
    for path in root.iterdir():
        info = path.lstat()
        if (
            path.name != "retained-cooling.log"
            or not stat.S_ISREG(info.st_mode)
            or info.st_nlink != 1
            or info.st_size
        ):
            raise ValueError(
                "fresh native cooling stage with only its empty owned regular capture required"
            )


def main():
    if (
        len(sys.argv) != 3
        or sys.argv[1] != "reference"
        or sys.argv[2] != "/inputs/request.json"
    ):
        raise ValueError(
            "usage: harbor-cad-retained-cooling reference /inputs/request.json"
        )
    raw_request = read_regular(Path(sys.argv[2]), 65536)
    envelope = common.strict_json(raw_request)
    original_path = Path("/inputs/wetting-original.csv")
    original = read_regular(original_path, 16 * 1024**2)
    validate_envelope(envelope, original)
    sandbox = common.cpu_sandbox(
        "HARBOR_CAD_RETAINED_COOLING_POLICY",
        POLICY,
        "/retained-cooling-runtime-closure.txt",
        sys.argv[2],
    )
    if sandbox is None or os.statvfs(original_path).f_flag & os.ST_RDONLY == 0:
        raise ValueError(
            "operation-only native cooling sandbox and read-only authoritative source required"
        )
    sandbox["checks"]["original_source_readonly"] = True
    root = Path.cwd()
    fresh_output(root)
    # The generated descriptor is private sandbox scratch, never a replacement
    # of the read-only approval or original source. Its exact value is retained
    # in the native receipt and checked independently after execution.
    generated = "/tmp/retained-cooling-native-request.json"
    common.atomic_json(generated, envelope["native_request"])
    with (root / "process.log").open("xb") as log:
        result = subprocess.run(
            ["@native@", "reference", generated],
            stdout=log,
            stderr=subprocess.STDOUT,
            timeout=900,
            env={"HOME": "/home/worker", "LC_ALL": "C"},
            check=False,
        )
    if result.returncode:
        raise ValueError(
            "native retained cooling failed; every closed/partial original remains preserved"
        )
    raw_native = read_regular(root / "retained-cooling-receipt.json", 256 * 1024)
    receipt = common.strict_json(raw_native)
    independent = verify(
        envelope["native_request"],
        original,
        receipt,
        root,
        envelope["maximum_relative_conservation_error"],
    )
    native_path = root / "native-retained-cooling-receipt.json"
    if native_path.exists() or native_path.is_symlink():
        raise ValueError("new immutable original native receipt required")
    (root / "retained-cooling-receipt.json").rename(native_path)
    receipt.update(
        request_sha256=hashlib.sha256(raw_request).hexdigest(),
        original_source_sha256=hashlib.sha256(original).hexdigest(),
        native_receipt_sha256=hashlib.sha256(raw_native).hexdigest(),
        native_driver_sha256="@driver_sha256@",
        independent_verification=independent,
        sandbox=sandbox,
        convergence="separate uniform Stefan analytic and complete-history spatial/temporal original-parent qualifications required",
        field_units="position:m,water_fraction:1,specific_enthalpy:J/kg,temperature:K,liquid_fraction:1",
        field_association="native_original_parent_congruent_control",
        boundary_exchange_units="J",
        physical_validation="unqualified",
    )
    common.atomic_json("retained-cooling-receipt.json", receipt)


if __name__ == "__main__":
    main()
