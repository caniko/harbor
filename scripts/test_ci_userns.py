"""Runner admission must never turn a failed namespace fixture into a skip."""

import contextlib
import io
from pathlib import Path
import tomllib
import unittest
from unittest.mock import patch

import ci_userns


class AdmissionTests(unittest.TestCase):
    def test_check_mode_reports_the_namespace_failure_without_loading_policy(self):
        with (
            patch.object(
                ci_userns, "probe", side_effect=RuntimeError("uid_map denied")
            ),
            patch.object(ci_userns, "load_profile") as load,
            self.assertRaisesRegex(RuntimeError, "uid_map denied"),
        ):
            ci_userns.prepare(check_only=True)
        load.assert_not_called()

    def test_a_self_hosted_runner_cannot_change_host_policy(self):
        with (
            patch.object(
                ci_userns, "probe", side_effect=RuntimeError("uid_map denied")
            ),
            patch.object(ci_userns, "RESTRICTION") as restriction,
            patch.dict(
                ci_userns.os.environ,
                {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "self-hosted"},
            ),
            patch.object(ci_userns, "load_profile") as load,
            self.assertRaisesRegex(RuntimeError, "github-hosted"),
        ):
            restriction.is_file.return_value = True
            restriction.read_text.return_value = "1\n"
            ci_userns.prepare()
        load.assert_not_called()

    def test_a_failed_post_profile_probe_remains_a_failure(self):
        with (
            patch.object(
                ci_userns,
                "probe",
                side_effect=[
                    RuntimeError("uid_map denied"),
                    RuntimeError("bind denied"),
                ],
            ),
            patch.object(ci_userns, "RESTRICTION") as restriction,
            patch.dict(
                ci_userns.os.environ,
                {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"},
            ),
            patch.object(ci_userns, "load_profile"),
            self.assertRaisesRegex(RuntimeError, "bind denied"),
        ):
            restriction.is_file.return_value = True
            restriction.read_text.return_value = "1\n"
            with contextlib.redirect_stderr(io.StringIO()):
                ci_userns.prepare()

    def test_a_changed_global_restriction_is_rejected(self):
        with (
            patch.object(
                ci_userns, "probe", side_effect=[RuntimeError("uid_map denied"), None]
            ),
            patch.object(ci_userns, "RESTRICTION") as restriction,
            patch.dict(
                ci_userns.os.environ,
                {"GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted"},
            ),
            patch.object(ci_userns, "load_profile"),
            self.assertRaisesRegex(RuntimeError, "global restriction changed"),
        ):
            restriction.is_file.return_value = True
            restriction.read_text.side_effect = ["1\n", "0\n"]
            with contextlib.redirect_stderr(io.StringIO()):
                ci_userns.prepare()

    def test_policy_attaches_only_to_nix_util_linux_unshare(self):
        self.assertIn("/nix/store/*-util-linux-*/bin/unshare", ci_userns.PROFILE)
        self.assertNotIn("/nix/store/**", ci_userns.PROFILE)
        self.assertNotIn("sysctl", ci_userns.PROFILE)

    def test_qualification_lanes_preserve_the_global_namespace_restriction(self):
        manifest = Path(__file__).resolve().parents[1] / "simit.toml"
        configuration = tomllib.loads(manifest.read_text())
        for component in configuration["monorepo"]["components"]:
            for gate in component.get("checks", []):
                with self.subTest(component=component["id"], gate=gate["id"]):
                    self.assertNotIn(
                        "apparmor_restrict_unprivileged_userns=0", gate["run"]
                    )


if __name__ == "__main__":
    unittest.main()
