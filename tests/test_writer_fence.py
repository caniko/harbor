"""Writer admission policy and durable epoch regression cases."""

import copy
import tempfile
import unittest
from pathlib import Path

from harbor_db import writer_fence


class FencePolicyTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name)
        (root / "state").mkdir(mode=0o700)
        (root / "data").mkdir(mode=0o700)
        self.config = {
            "resource": "fixture", "major": "18", "package": "/postgres",
            "data_dir": str(root / "data"), "state_dir": str(root / "state"),
            "required_mounts": [],
            "writer_fence": {
                "system_identifier": "12345", "required_for_recovery": True,
                "normal_hba_file": "/nix/store/fixture-pg-hba.conf",
                "allowed_preload_libraries": ["pg_stat_statements", "vchord.so"],
                "replication_peers": [{"role": "replicator", "address": "127.0.0.1/32"}],
            },
        }

    def test_strict_hba_never_permits_an_application_database_role(self):
        lines = writer_fence.hba(self.config).decode().splitlines()
        self.assertEqual(lines, [
            "local all postgres peer", "local replication postgres peer",
            "host replication replicator 127.0.0.1/32 scram-sha-256",
            "local all all reject", "local replication all reject",
            "host all all 0.0.0.0/0 reject", "host all all ::/0 reject",
            "host replication all 0.0.0.0/0 reject", "host replication all ::/0 reject",
        ])

    def test_replication_policy_rejects_role_address_or_rule_injection(self):
        for key, value in [("role", "replicator\nlocal all all trust"),
                           ("role", "all"), ("role", "postgres"),
                           ("address", "localhost"),
                           ("address", "127.0.0.1/32 trust")]:
            with self.subTest(key=key, value=value):
                policy = copy.deepcopy(self.config)
                policy["writer_fence"]["replication_peers"][0][key] = value
                with self.assertRaises(ValueError):
                    writer_fence.hba(policy)

    def test_epoch_binding_rejects_a_replacement_cluster_or_changed_fence(self):
        original = writer_fence.binding(self.config)
        for change in ["data_dir", "resource"]:
            config = copy.deepcopy(self.config)
            config[change] += "-replacement"
            self.assertNotEqual(writer_fence.binding(config), original)
        config = copy.deepcopy(self.config)
        config["writer_fence"]["system_identifier"] = "67890"
        self.assertNotEqual(writer_fence.binding(config), original)
        config = copy.deepcopy(self.config)
        config["writer_fence"]["replication_peers"][0]["address"] = "::1/128"
        self.assertNotEqual(writer_fence.binding(config), original)

    def test_new_package_or_enrollment_retirement_does_not_discard_an_active_epoch(self):
        original = writer_fence.binding(self.config)
        config = copy.deepcopy(self.config)
        config["package"] = "/new-postgres"
        config["writer_fence"]["normal_hba_file"] = "/nix/store/new-pg-hba.conf"
        config["writer_fence"]["required_for_recovery"] = False
        self.assertEqual(writer_fence.binding(config), original)

    def test_redirected_state_is_not_a_fence_authority(self):
        alias = Path(self.temporary.name) / "alias"
        alias.symlink_to(self.config["state_dir"], target_is_directory=True)
        self.config["state_dir"] = str(alias)
        with self.assertRaisesRegex(ValueError, "canonical"):
            writer_fence.paths(self.config)


if __name__ == "__main__":
    unittest.main()
