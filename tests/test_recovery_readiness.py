"""Recovery acceptance must bind real records, backup bytes and cluster identity."""

import hashlib
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from harbor_db import postgres, recovery
from harbor_db.durable import lock, write_json


class RecoveryReadinessTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.backup = self.root / "backup"
        self.base = self.backup / "base" / "base-1"
        self.base.mkdir(parents=True)
        (self.backup / "locks").mkdir()
        (self.backup / "locks/mutate").touch()
        (self.backup / "evidence").mkdir()
        (self.backup / "LAST_SUCCESS").write_text("base-1\n")
        (self.base / "PG_VERSION").write_text("18\n")
        (self.base / "backup_manifest").write_text('{"WAL-Ranges": []}\n')
        write_json(self.backup / "base/base-1.meta.json", {
            "backup_id": "base-1", "system_identifier": "12345", "pg_major": 18,
            "epoch_id": "epoch-1", "backup_stop_lsn": "0/100", "post_backup_lsn": "0/200",
        })
        self.config = {
            "data_dir": str(self.root / "primary"), "major": "18", "package": "/postgres18",
            "recovery": {
                "system_identifier": "12345", "backup_root": str(self.backup),
                "snapshot_file": str(self.backup / "evidence/records.json"),
                "receipt_file": str(self.backup / "evidence/recovery.json"),
                "off_host_receipt_file": None, "source_hostname": "primary-host",
                "max_age_seconds": 3600, "verify_timeout_seconds": 900,
                "record_checks": [{"name": "reviews", "database": "app", "sql": "SELECT records"}],
            },
        }
        self.now = 10000
        os.utime(self.base / "backup_manifest", (self.now, self.now))
        self.live = patch.object(postgres, "inspect_live", return_value={"system_identifier": "12345"})
        self.live.start()
        self.addCleanup(self.live.stop)
        self.probe = patch.object(postgres, "inspect_cluster", return_value="12345")
        self.probe.start()
        self.addCleanup(self.probe.stop)
        self.verify = patch.object(recovery, "verify_backup")
        self.verify.start()
        self.addCleanup(self.verify.stop)
        self.query = patch.object(recovery, "query", return_value="3:record-digest\n")
        self.query.start()
        self.addCleanup(self.query.stop)
        self.restored = self.root / "restored"
        self.restored.mkdir()
        self.observed = {
            "data_dir": str(self.restored), "major": "18", "system_identifier": "12345",
            "read_only": "on", "in_recovery": False, "replay_lsn": "0/200",
        }

    def snapshot(self):
        recovery.snapshot(self.config, "/run/postgresql", 5432, now=self.now)

    def certify(self, **kwargs):
        with patch.object(recovery, "inspect_restored", return_value=self.observed):
            return recovery.certify(self.config, str(self.restored), "/restore/socket", 55432,
                                    now=self.now, hostname="primary-host", **kwargs)

    def test_missing_evidence_is_not_recovery_readiness(self):
        with self.assertRaisesRegex(ValueError, "snapshot"):
            recovery.check(self.config, now=self.now)
        self.assertFalse(Path(self.config["recovery"]["receipt_file"]).exists())

    def test_real_matching_checks_publish_a_bound_receipt(self):
        self.snapshot()
        result = self.certify()
        self.assertEqual(result["status"], "ready")
        self.assertEqual(result["manifest_sha256"], hashlib.sha256((self.base / "backup_manifest").read_bytes()).hexdigest())
        self.assertEqual(recovery.check(self.config, now=self.now)["backup_id"], "base-1")

    def test_record_loss_refuses_publication(self):
        self.snapshot()
        with patch.object(recovery, "query", return_value="2:missing-review\n"), self.assertRaisesRegex(ValueError, "records differ"):
            self.certify()
        self.assertFalse(Path(self.config["recovery"]["receipt_file"]).exists())

    def test_query_contract_change_invalidates_old_acceptance(self):
        self.snapshot()
        self.certify()
        self.config["recovery"]["record_checks"][0]["sql"] = "SELECT different_records"
        with self.assertRaisesRegex(ValueError, "contract"):
            recovery.check(self.config, now=self.now)

    def test_manifest_change_and_new_backup_invalidate_receipt(self):
        self.snapshot()
        self.certify()
        (self.base / "backup_manifest").write_text("{}\n")
        os.utime(self.base / "backup_manifest", (self.now, self.now))
        with self.assertRaisesRegex(ValueError, "backup"):
            recovery.check(self.config, now=self.now)
        (self.backup / "LAST_SUCCESS").write_text("../primary\n")
        with self.assertRaisesRegex(ValueError, "backup identifier"):
            recovery.check(self.config, now=self.now)

    def test_stale_and_future_evidence_is_rejected(self):
        self.snapshot()
        self.certify()
        for now in (self.now + 3601, self.now - 1):
            with self.subTest(now=now), self.assertRaisesRegex(ValueError, "timestamp"):
                recovery.check(self.config, now=now)

    def test_primary_writable_or_incomplete_restore_cannot_be_certified(self):
        self.snapshot()
        for changes in ({"data_dir": self.config["data_dir"]}, {"read_only": "off"},
                        {"in_recovery": True}, {"replay_lsn": "0/100"},
                        {"system_identifier": "99999"}):
            with self.subTest(changes=changes):
                observed = {**self.observed, **changes}
                with patch.object(recovery, "inspect_restored", return_value=observed), self.assertRaises(ValueError):
                    recovery.certify(self.config, str(self.restored), "/restore/socket", 55432,
                                     now=self.now, hostname="primary-host")
        self.assertFalse(Path(self.config["recovery"]["receipt_file"]).exists())

    def test_off_host_acceptance_must_be_independent_and_same_backup(self):
        self.snapshot()
        self.certify()
        off_host = self.backup / "evidence/off-host.json"
        self.config["recovery"]["off_host_receipt_file"] = str(off_host)
        with self.assertRaisesRegex(ValueError, "off-host"):
            recovery.check(self.config, now=self.now)
        local = json.loads(Path(self.config["recovery"]["receipt_file"]).read_text())
        write_json(off_host, local)
        with self.assertRaisesRegex(ValueError, "independent"):
            recovery.check(self.config, now=self.now)
        # Exercise the same certifier with a real executor-host identity; do not
        # make an unrelated copy of the local receipt establish remote recovery.
        self.config["recovery"]["receipt_file"] = str(off_host)
        with patch.object(recovery, "inspect_restored", return_value=self.observed):
            recovery.certify(self.config, str(self.restored), "/restore/socket", 55432,
                             now=self.now, hostname="recovery-host")
        self.config["recovery"]["receipt_file"] = str(self.backup / "evidence/recovery.json")
        self.assertEqual(recovery.check(self.config, now=self.now)["off_host"], "recovery-host")

    def test_readiness_never_creates_a_missing_backup_lock(self):
        (self.backup / "locks/mutate").unlink()
        with self.assertRaises(OSError):
            recovery.check(self.config, now=self.now)
        self.assertFalse((self.backup / "locks/mutate").exists())

    def test_concurrent_backup_and_evidence_publication_are_rejected(self):
        self.snapshot()
        self.certify()
        for path in (self.backup / "locks/mutate", self.backup / "evidence/recovery.lock"):
            with self.subTest(path=path), lock(path):
                with self.assertRaises(BlockingIOError):
                    recovery.check(self.config, now=self.now)
                with self.assertRaises(BlockingIOError):
                    self.snapshot()
        self.assertEqual(recovery.check(self.config, now=self.now)["status"], "ready")

    def test_new_snapshot_invalidates_prior_certification(self):
        self.snapshot()
        self.certify()
        with patch.object(recovery, "query", return_value="3:changed-review\n"):
            self.snapshot()
        with self.assertRaisesRegex(ValueError, "record-level recovery"):
            recovery.check(self.config, now=self.now)

    def test_redirected_evidence_is_rejected(self):
        self.snapshot()
        path = Path(self.config["recovery"]["snapshot_file"])
        path.rename(path.with_suffix(".saved"))
        path.symlink_to(path.with_suffix(".saved"))
        with self.assertRaisesRegex(ValueError, "redirected"):
            self.certify()


if __name__ == "__main__":
    unittest.main()
