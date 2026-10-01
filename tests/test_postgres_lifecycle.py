"""Regression cases for cluster selection and interrupted publication."""

import json
import os
import select
import signal
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from harbor_db import postgres
from harbor_db.durable import lock, write_json


class ClusterLifecycleTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.data = self.root / "18"
        self.data.mkdir()
        (self.data / "PG_VERSION").write_text("18\n")
        self.state = self.root / "state"
        self.state.mkdir()
        self.config = {
            "resource": "test",
            "data_dir": str(self.data),
            "major": "18",
            "package": "/postgres18",
            "state_dir": str(self.state),
            "required_mounts": [],
        }
        self.probe = patch.object(postgres, "inspect_cluster", return_value="12345")
        self.probe.start()
        self.addCleanup(self.probe.stop)

    def adopt(self):
        postgres.adopt(self.config, "12345")

    def test_pg_version_alone_never_authorizes_start(self):
        with self.assertRaisesRegex(postgres.LifecycleError, "not adopted"):
            postgres.check(self.config)
        self.assertFalse((self.state / "identity.json").exists())

    def test_adoption_is_idempotent_and_rejects_replacement(self):
        self.adopt()
        self.adopt()
        postgres.check(self.config)
        with patch.object(postgres, "inspect_cluster", return_value="67890"):
            with self.assertRaisesRegex(postgres.LifecycleError, "identity mismatch"):
                postgres.check(self.config)
            with self.assertRaises(postgres.LifecycleError):
                postgres.adopt(self.config, "67890")

    def test_missing_data_does_not_initialize_replacement(self):
        self.adopt()
        (self.data / "PG_VERSION").unlink()
        self.probe.stop()
        with self.assertRaises(postgres.LifecycleError):
            postgres.check(self.config)
        self.assertFalse((self.data / "PG_VERSION").exists())

    def test_rollback_cannot_select_old_directory(self):
        self.adopt()
        rollback = dict(self.config, data_dir=str(self.root / "17"), major="17")
        with self.assertRaisesRegex(postgres.LifecycleError, "identity mismatch"):
            postgres.check(rollback)

    def test_interrupted_upgrade_blocks_even_adopted_cluster(self):
        self.adopt()
        (self.state / "upgrade.json").write_text(json.dumps({"phase": "building"}))
        with self.assertRaisesRegex(postgres.LifecycleError, "upgrade"):
            postgres.check(self.config)

    def test_missing_mount_fails_before_probe_or_adoption(self):
        self.config["required_mounts"] = [str(self.root / "missing-mount")]
        with self.assertRaisesRegex(postgres.LifecycleError, "mount"):
            self.adopt()
        self.assertFalse((self.state / "identity.json").exists())

    def test_adoption_requires_independently_supplied_identifier(self):
        with self.assertRaisesRegex(postgres.LifecycleError, "identifier"):
            postgres.adopt(self.config, "99999")
        self.assertFalse((self.state / "identity.json").exists())

    def test_writer_is_the_guarded_process_and_retains_lease_across_exec(self):
        self.adopt()
        package = self.root / "package"
        (package / "bin").mkdir(parents=True)
        executable = package / "bin/postgres"
        executable.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            "print(json.dumps({'pid': os.getpid(), 'args': sys.argv[1:]}), flush=True)\n"
            "sys.stdin.buffer.read(1)\n"
        )
        executable.chmod(0o700)
        config = dict(self.config, package=str(package))
        # The real exec/descriptor path is exercised in a separate process;
        # only the pg_controldata probe uses this test's synthetic cluster.
        command = (
            "from harbor_db import postgres; "
            "postgres.inspect_cluster = lambda *args: '12345'; "
            f"postgres.serve({config!r})"
        )
        process = subprocess.Popen(
            [sys.executable, "-B", "-c", command], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True,
            env={**os.environ, "PGDATA": str(self.root / "wrong-storage")},
        )
        try:
            self.assertTrue(select.select([process.stdout], [], [], 10)[0])
            ready = json.loads(process.stdout.readline())
            self.assertEqual(ready["pid"], process.pid)
            self.assertEqual(ready["args"], [
                "-D", str(self.data), "-c", f"data_directory={self.data}",
                "-c", "fsync=on", "-c", "full_page_writes=on",
                "-c", "synchronous_commit=on",
            ])
            with self.assertRaises(BlockingIOError):
                with lock(self.state / "lock"):
                    self.fail("offline operation acquired a live writer's lease")
            _, error = process.communicate(input=b"x", timeout=10)
            self.assertEqual(process.returncode, 0, error.decode())
            with lock(self.state / "lock"):
                pass
        finally:
            # Also cleans up a surviving child if the old supervisor path fails.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate(timeout=10)

    def ready_upgrade(self):
        self.data.rename(self.root / "17")
        self.old = self.root / "17"
        (self.old / "PG_VERSION").write_text("17\n")
        (self.old / "pg_tblspc").mkdir()
        self.config["upgrade"] = {
            "data_dir": str(self.old), "major": "17", "package": "/postgres17",
            "validate_command": ["/validate"],
        }
        source = dict(self.config, data_dir=str(self.old), major="17", package="/postgres17")
        postgres.adopt(source, "12345")
        stage = self.root / "18.harbor-staging"
        stage.mkdir()
        (stage / "PG_VERSION").write_text("18\n")
        journal = {
            "version": 1, "phase": "ready", "source": postgres.identity(source, "12345"),
            "source_control": "digest",
            "target": {k: self.config[k] for k in ("resource", "data_dir", "major", "package")},
            "staging": str(stage), "identity": postgres.identity(self.config, "12345"),
            "source_copy": str(self.root / "18.harbor-source"),
        }
        write_json(self.state / "upgrade.json", journal)
        return stage, journal

    def test_resume_after_rename_and_identity_write(self):
        stage, journal = self.ready_upgrade()
        stage.rename(self.data)
        write_json(self.state / "identity.json", journal["identity"])
        with patch.object(postgres, "require_stopped"), patch.object(postgres, "control_digest", return_value="digest"):
            postgres.upgrade(self.config)
        postgres.check(self.config)
        self.assertTrue(self.old.exists())
        self.assertFalse((self.state / "upgrade.json").exists())

    def test_changed_source_prevents_publication(self):
        stage, _ = self.ready_upgrade()
        with patch.object(postgres, "require_stopped"), patch.object(postgres, "control_digest", return_value="changed"):
            with self.assertRaisesRegex(postgres.LifecycleError, "source or contract changed"):
                postgres.upgrade(self.config)
        self.assertTrue(stage.exists())
        self.assertFalse(self.data.exists())

    def interrupt_authority_publication(self, filename):
        _, journal = self.ready_upgrade()
        real_write = postgres.write_json

        def interrupt(path, value):
            real_write(path, value)
            if path.name == filename:
                raise OSError("publication interrupted")

        with patch.object(postgres, "require_stopped"), patch.object(postgres, "write_json", side_effect=interrupt):
            with self.assertRaisesRegex(OSError, "publication interrupted"):
                postgres.publish(self.config, journal)
        self.assertTrue((self.state / "upgrade.json").exists())
        with self.assertRaisesRegex(postgres.LifecycleError, "upgrade"):
            postgres.check(self.config)
        with patch.object(postgres, "require_stopped"), patch.object(postgres, "control_digest", return_value="digest"):
            postgres.upgrade(self.config)
        postgres.check(self.config)
        self.assertEqual(json.loads((self.state / "previous-identity.json").read_text()), journal["source"])

    def test_interruption_after_previous_identity_publication_can_resume(self):
        self.interrupt_authority_publication("previous-identity.json")

    def test_interruption_after_current_identity_publication_can_resume(self):
        self.interrupt_authority_publication("identity.json")

    def test_resumed_publication_flush_failure_keeps_upgrade_barrier(self):
        stage, journal = self.ready_upgrade()
        stage.rename(self.data)
        real_sync = postgres.sync_directory

        def fail_destination_parent(path):
            if path == self.data.parent:
                raise OSError("destination publication is not durable")
            real_sync(path)

        with patch.object(postgres, "require_stopped"), patch.object(
            postgres, "sync_directory", side_effect=fail_destination_parent,
        ):
            with self.assertRaisesRegex(OSError, "publication is not durable"):
                postgres.publish(self.config, journal)
        self.assertTrue((self.state / "upgrade.json").exists())
        self.assertEqual(json.loads((self.state / "identity.json").read_text()), journal["source"])

    def test_interruption_after_initdb_cannot_authorize_target(self):
        stage, journal = self.ready_upgrade()
        journal["phase"] = "building"
        del journal["identity"]
        write_json(self.state / "upgrade.json", journal)
        with patch.object(postgres, "require_stopped"), patch.object(postgres, "control_digest", return_value="digest"):
            with self.assertRaisesRegex(postgres.LifecycleError, "incomplete upgrade"):
                postgres.upgrade(self.config)
        with self.assertRaises(postgres.LifecycleError):
            postgres.check(self.config)
        self.assertTrue((stage / "PG_VERSION").exists())
        self.assertFalse(self.data.exists())


if __name__ == "__main__":
    unittest.main()
