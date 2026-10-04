"""Native timezone regression using an already-realized tzdata package; no builds."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tzdata", type=Path, required=True)
    args = parser.parse_args()
    source = Path(__file__).resolve().parents[1] / "lib/timezone.nix"
    coreutils = Path(shutil.which("head")).parent.parent
    expression = f"""
      let timezone = import {source};
          pkgs.buildPackages = {{
            tzdata = {json.dumps(str(args.tzdata))};
            coreutils = {json.dumps(str(coreutils))};
          }};
      in timezone.mkEnvironment {{inherit pkgs;}}
    """
    result = subprocess.run(
        ["nix-instantiate", "--eval", "--no-allow-import-from-derivation",
         "--strict", "--json", "--expr", expression],
        check=True, text=True, capture_output=True,
    )
    config = json.loads(result.stdout)
    # Check the shared validation script and libc's actual timestamp conversion.
    cases = [
        ("UTC", 946684800, "00:00:+0000"),
        ("Europe/Istanbul", 946684800, "02:00:+0200"),
        ("Europe/Istanbul", 1593561600, "03:00:+0300"),
        ("America/New_York", 946684800, "19:00:-0500"),
        ("America/New_York", 1593561600, "20:00:-0400"),
    ]
    for zone, epoch, expected in cases:
        env = dict(os.environ, **config["env"])
        env["TZ"] = zone
        subprocess.run(["bash", "-c", config["validationScript"]], env=env, check=True)
        observed = subprocess.check_output(
            ["date", "-d", f"@{epoch}", "+%H:%M:%z"], env=env, text=True,
        ).strip()
        assert observed == expected, (zone, epoch, observed, expected)
    for zone, directory in [
        ("Missing/Zone", config["env"]["TZDIR"]),
        ("leapseconds", config["env"]["TZDIR"]),
        ("UTC", "/missing/timezone-database"),
    ]:
        env = dict(os.environ, TZ=zone, TZDIR=directory)
        result = subprocess.run(
            ["bash", "-c", config["validationScript"]], env=env,
            text=True, capture_output=True,
        )
        assert result.returncode != 0, (zone, directory)
        assert "unknown timezone" in result.stderr, result.stderr
    print("8 timezone runtime cases passed (UTC, named zones, DST, missing/invalid data)")


if __name__ == "__main__":
    main()
