"""Admit bind-mount fixtures on ephemeral Ubuntu runners without disabling AppArmor."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

RESTRICTION = Path("/proc/sys/kernel/apparmor_restrict_unprivileged_userns")
PROFILE = """abi <abi/4.0>,
include <tunables/global>
profile harbor-ci-unshare /nix/store/*-util-linux-*/bin/unshare flags=(unconfined) {
  userns,
}
"""


def probe():
    """Exercise the same UID mapping and mount namespace as the DB alias fixture."""
    unshare = shutil.which("unshare")
    if not unshare:
        raise RuntimeError("unshare is required for the bind-mount fixtures")
    if os.geteuid() == 0:
        raise RuntimeError(
            "namespace fixture admission must run as an unprivileged user"
        )
    with tempfile.TemporaryDirectory(prefix="harbor-ci-userns-") as directory:
        root = Path(directory)
        (root / "source").mkdir()
        (root / "target").mkdir()
        (root / "source" / "marker").write_text("bind-mount-fixture\n")
        command = [
            unshare,
            "--user",
            "--map-root-user",
            "--mount",
            "--fork",
            "--kill-child",
            "sh",
            "-ec",
            'mount --make-rprivate /; mount --bind "$1/source" "$1/target"; '
            'test "$(cat "$1/target/marker")" = bind-mount-fixture; umount "$1/target"',
            "harbor-ci-userns",
            str(root),
        ]
        result = subprocess.run(command, capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise RuntimeError(
                f"UID mapping / bind-mount probe failed ({result.returncode}): {result.stderr.strip()}"
            )


def load_profile():
    parser = next(
        (
            path
            for path in ("/sbin/apparmor_parser", "/usr/sbin/apparmor_parser")
            if Path(path).is_file()
        ),
        None,
    )
    if not parser:
        raise RuntimeError("the hosted Ubuntu runner must provide apparmor_parser")
    unshare = shutil.which("unshare")
    if not unshare or not Path(unshare).resolve().match(
        "/nix/store/*-util-linux-*/bin/unshare"
    ):
        raise RuntimeError("AppArmor admission only covers Nix util-linux unshare")
    # Keep the profile only in this ephemeral kernel; no persistent host config.
    subprocess.run(
        ["sudo", "-n", parser, "--replace", "--skip-cache"],
        input=PROFILE,
        text=True,
        check=True,
        timeout=30,
    )


def prepare(check_only=False):
    try:
        probe()
        return {
            "status": "user-and-mount-namespace-probe-passed",
            "profileLoaded": False,
        }
    except RuntimeError as failure:
        if (
            check_only
            or not RESTRICTION.is_file()
            or RESTRICTION.read_text().strip() != "1"
        ):
            raise
        if (
            os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"
        ):
            raise RuntimeError(
                "AppArmor profile loading is restricted to github-hosted runners"
            ) from failure
        print(str(failure), file=sys.stderr)
        load_profile()
    if RESTRICTION.read_text().strip() != "1":
        raise RuntimeError("global restriction changed during profile admission")
    probe()
    return {
        "status": "user-and-mount-namespace-probe-passed",
        "profileLoaded": True,
        "globalRestriction": 1,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Probe capability without loading any host policy",
    )
    args = parser.parse_args()
    if sys.platform != "linux":
        print(json.dumps({"status": "portable-workspace-lane", "linuxFixtures": False}))
        return
    print(json.dumps(prepare(check_only=args.check), sort_keys=True))


if __name__ == "__main__":
    main()
