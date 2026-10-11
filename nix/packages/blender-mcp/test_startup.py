"""Bootstrap regression tests without a GUI or user preferences on disk."""

import importlib.util
import os
import sys
import unittest
from types import SimpleNamespace
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("bootstrap", sys.argv.pop(1))
bootstrap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bootstrap)


class StartupTests(unittest.TestCase):
    def run_startup(self, persistent, save_preferences):
        addon_preferences = SimpleNamespace(
            telemetry_consent=True, allow_ai_control=False
        )
        preferences = SimpleNamespace(
            use_preferences_save=save_preferences,
            addons={
                bootstrap._addon_module(): SimpleNamespace(
                    preferences=addon_preferences
                )
            },
        )
        server = SimpleNamespace(running=True, ensure_heartbeat_timer=lambda: None)
        bpy = SimpleNamespace(
            context=SimpleNamespace(preferences=preferences),
            types=SimpleNamespace(blendermcp_server=server),
        )
        with (
            patch.dict(sys.modules, {"bpy": bpy}),
            patch.dict(
                os.environ,
                {
                    "CANIX_BLENDER_MCP_PERSISTENT": persistent,
                    "CANIX_BLENDER_MCP_VERSION": "fixture",
                },
            ),
            patch.object(bootstrap, "_ensure_repo", return_value="/fixture"),
            patch.object(bootstrap, "_payload_version", return_value="fixture"),
        ):
            self.assertEqual(bootstrap.main(), 0)
        self.assertFalse(addon_preferences.allow_ai_control)
        self.assertFalse(addon_preferences.telemetry_consent)
        return preferences.use_preferences_save

    def test_everyday_preserves_both_user_preference_choices(self):
        for setting in (True, False):
            with self.subTest(setting=setting):
                self.assertEqual(self.run_startup("1", setting), setting)

    def test_opt_in_never_automatically_saves_preferences(self):
        self.assertFalse(self.run_startup("0", True))

    def test_repository_conflict_is_not_adopted(self):
        repo = SimpleNamespace(module=bootstrap.REPO_MODULE, directory="/unrelated")
        bpy = SimpleNamespace(
            context=SimpleNamespace(
                preferences=SimpleNamespace(
                    extensions=SimpleNamespace(repos=[repo]),
                )
            )
        )
        with (
            patch.dict(sys.modules, {"bpy": bpy}),
            patch.dict(os.environ, {"CANIX_BLENDER_MCP_REPO_DIR": "/managed"}),
            patch.object(os.path, "isfile", return_value=True),
            self.assertRaisesRegex(RuntimeError, "refusing to adopt or replace"),
        ):
            bootstrap._ensure_repo()
        self.assertEqual(repo.directory, "/unrelated")


if __name__ == "__main__":
    unittest.main()
