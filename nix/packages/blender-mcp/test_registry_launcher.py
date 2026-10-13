"""Registry integration using the production hook and actual add-on writer."""

import argparse
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class RegistryLauncherTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="blender-registry-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        home = self.root / "home"
        home.mkdir()
        runtime = self.root / "run"
        runtime.mkdir(mode=0o700)
        self.env = {
            "PATH": os.environ["PATH"],
            "HOME": str(home),
            "XDG_RUNTIME_DIR": str(runtime),
            "XDG_CACHE_HOME": str(self.root / "cache"),
        }
        self.registry = runtime / "blender-mcp" / "instances"

    def launch(self, *, rejected=False, **values):
        result = subprocess.run(
            [
                "bash",
                "-u",
                "-c",
                # The acceptance fixture's workspace is outside the sandbox.
                # Refusals explicitly exit; do not enable errexit for that mkdir.
                (
                    'umask "${UMASK:-022}"; source "$HOOK" || exit 1; '
                    'printf "%s\\n" "$BLENDER_MCP_RUNTIME_DIR" "$BLENDER_MCP_TMPDIR" "$TMPDIR"'
                ),
            ],
            env={**self.env, **values, "HOOK": str(self.hook)},
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        if rejected:
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn("refusing", result.stderr)
            return result.stderr
        self.assertEqual(result.returncode, 0, result.stderr)
        registry, temporary, tmpdir = result.stdout.splitlines()
        self.assertEqual(temporary, tmpdir)
        return Path(registry), Path(temporary)

    def write(self, **values):
        result = subprocess.run(
            [
                sys.executable,
                "-B",
                str(Path(__file__).with_name("registry_writer.py")),
                str(self.extension),
                "--server",
                str(self.server),
            ],
            env={**self.env, **values},
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["registry"], report["server_registry"])
        self.assertEqual(report["modes"]["registry"], "700")
        self.assertEqual(report["modes"]["record"], "600")
        self.assertEqual(report["contents"]["schema"], "blender-mcp-instance/1")
        self.assertEqual(report["contents"]["blend_file"], "second.blend")
        self.assertEqual(report["temporaries"], [])
        return report

    def mode(self, path):
        return format(stat.S_IMODE(path.stat().st_mode), "03o")

    def test_production_launchers_share_one_hook(self):
        def hook(launcher):
            matches = re.findall(
                r"/nix/store/[a-z0-9]+-blender-mcp-env\.sh", launcher.read_text()
            )
            self.assertTrue(matches, launcher)
            self.assertEqual(len(set(matches)), 1)
            return Path(matches[0])

        self.assertEqual(hook(self.launcher), hook(self.desktop))

    def test_both_launcher_orders_preserve_real_addon_records(self):
        for first in (self.launcher, self.desktop):
            with self.subTest(first=first.name):
                runtime = self.root / first.name
                runtime.mkdir(mode=0o700)
                self.env["XDG_RUNTIME_DIR"] = str(runtime)
                # Each launcher sources this same hook before its executable.
                self.hook = Path(
                    re.search(
                        r"/nix/store/[a-z0-9]+-blender-mcp-env\.sh", first.read_text()
                    ).group()
                )
                registry, temporary = self.launch()
                self.assertEqual(registry, runtime / "blender-mcp" / "instances")
                self.assertEqual(temporary, registry / "tmp")
                report = self.write()
                self.assertEqual(report["registry"], str(registry))
                record = Path(report["record"])
                before = record.read_bytes()
                self.launch()
                self.assertEqual(record.read_bytes(), before)
                for path in (registry.parent, registry, temporary):
                    self.assertEqual(self.mode(path), "700")

    def test_private_modes_do_not_depend_on_umask(self):
        for mask in ("000", "022", "077"):
            with self.subTest(umask=mask):
                runtime = self.root / f"run-{mask}"
                registry, temporary = self.launch(
                    UMASK=mask, XDG_RUNTIME_DIR=str(runtime)
                )
                report = self.write(UMASK=mask, XDG_RUNTIME_DIR=str(runtime))
                self.assertEqual(report["registry"], str(registry))
                for path in (registry.parent, registry, temporary):
                    self.assertEqual(self.mode(path), "700")

    def test_xdg_state_and_home_fallbacks_use_native_resolvers(self):
        self.env.pop("XDG_RUNTIME_DIR")
        for values, expected in (
            ({"XDG_STATE_HOME": str(self.root / "state")}, self.root / "state"),
            ({}, self.root / "home" / ".local" / "state"),
        ):
            with self.subTest(values=values):
                registry, _ = self.launch(**values)
                self.assertEqual(registry, expected / "blender-mcp" / "instances")
                self.assertEqual(self.write(**values)["registry"], str(registry))
                self.assertEqual(self.mode(registry.parent), "700")

    def test_registry_and_temporary_overrides_are_preserved(self):
        registry = self.root / "override-registry"
        temporary = self.root / "screenshots"
        values = {
            "BLENDER_MCP_RUNTIME_DIR": str(registry),
            "BLENDER_MCP_TMPDIR": str(temporary),
        }
        self.assertEqual(self.launch(**values), (registry, temporary))
        self.assertEqual(self.write(**values)["registry"], str(registry))
        self.assertEqual(self.mode(temporary), "700")
        self.assertFalse(self.registry.exists())

    def test_addon_first_loose_native_parent_is_refused_without_repair(self):
        report = self.write()
        # Characterize the released add-on, rather than synthesizing its tree.
        self.assertEqual(report["modes"]["parent"], "755")
        record = Path(report["record"])
        before = record.read_bytes()
        self.assertIn("unsafe mode", self.launch(rejected=True))
        self.assertEqual(self.mode(self.registry.parent), "755")
        self.assertEqual(record.read_bytes(), before)

    def test_unsafe_registry_and_temporary_overrides_are_refused(self):
        for variable in ("BLENDER_MCP_RUNTIME_DIR", "BLENDER_MCP_TMPDIR"):
            for kind in ("symlink", "mode", "file", "relative"):
                with self.subTest(variable=variable, kind=kind):
                    path = self.root / f"{variable}-{kind}"
                    if kind == "symlink":
                        target = self.root / f"{variable}-target"
                        target.mkdir(mode=0o700)
                        path.symlink_to(target, target_is_directory=True)
                    elif kind == "mode":
                        path.mkdir(mode=0o755)
                        path.chmod(0o755)
                    elif kind == "file":
                        path.touch()
                    value = "relative/registry" if kind == "relative" else str(path)
                    self.launch(rejected=True, **{variable: value})


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("extension", "server", "launcher", "desktop"):
        parser.add_argument(f"--{name}", type=Path, required=True)
    args, remaining = parser.parse_known_args()
    for name, value in vars(args).items():
        setattr(RegistryLauncherTests, name, value)
    RegistryLauncherTests.hook = Path(
        re.search(
            r"/nix/store/[a-z0-9]+-blender-mcp-env\.sh", args.launcher.read_text()
        ).group()
    )
    unittest.main(argv=[sys.argv[0], *remaining])
