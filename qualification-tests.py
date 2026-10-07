"""Hosted regressions for provider evidence and runner readback boundaries."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


evidence = load("qualification_evidence", "qualification-evidence.py")
readiness = load("runner_readiness", "runner-readiness.py")


class ArtifactIdentityTests(unittest.TestCase):
    def setUp(self):
        # The producer checks out the raw head, while the provider may record a
        # synthetic merge SHA. Those two identities must remain distinct.
        self.source = {"head": "a" * 40, "workflow_run_head_sha": "b" * 40, "run_id": 123}
        self.artifact = {
            "id": 456, "name": "harbor-proof-0", "expired": False,
            "created_at": "2026-10-07T00:00:00Z", "expires_at": "2026-11-07T00:00:00Z",
            "digest": "sha256:" + "c" * 64,
            "workflow_run": {"id": 123, "head_sha": "b" * 40},
        }

    def audit(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "retention.json"
            with patch.object(evidence, "identity", return_value=self.source), patch.object(
                evidence, "api", return_value={"artifacts": [self.artifact]}
            ):
                evidence.artifacts("harbor-proof-", 1, destination)
            return json.loads(destination.read_text())

    def test_accepts_provider_merge_sha_separate_from_checked_head(self):
        receipt = self.audit()
        self.assertEqual(receipt["head"], "a" * 40)
        self.assertEqual(receipt["workflow_run_head_sha"], "b" * 40)
        self.assertEqual(receipt["artifacts"][0]["id"], 456)

    def test_rejects_artifact_from_another_run_even_with_matching_sha(self):
        self.artifact["workflow_run"]["id"] = 789
        with self.assertRaisesRegex(RuntimeError, "workflow run mismatch"):
            self.audit()

    def test_rejects_artifact_sha_not_bound_to_provider_run(self):
        self.artifact["workflow_run"]["head_sha"] = self.source["head"]
        with self.assertRaisesRegex(RuntimeError, "workflow source mismatch"):
            self.audit()


class RunnerResponseTests(unittest.TestCase):
    def setUp(self):
        # Match the documented hosted-runner schema, without the nonexistent
        # machine_size field that previously made every successful lookup fail.
        self.runner = {"id": 5, "name": "hosted-64gb", "status": "Ready", "maximum_runners": 1,
                       "machine_size_details": {"memory_gb": 64}, "runner_group_id": 7}
        self.repo = {"owner": {"type": "Organization"}, "id": 42, "private": True}
        self.group = {"id": 7, "visibility": "selected"}

    def response(self, path):
        if path == "repos/example/harbor-db":
            return self.repo
        if path.startswith("orgs/example/actions/hosted-runners?"):
            return {"runners": [self.runner]}
        if path == "orgs/example/actions/runner-groups/7":
            return self.group
        if path.startswith("orgs/example/actions/runner-groups/7/repositories?"):
            return {"repositories": [{"id": 42}]}
        raise AssertionError(f"Unexpected provider lookup: {path}")

    def configured(self):
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/harbor-db", "GITHUB_RUN_ID": "123"}), patch.object(
            readiness, "api", side_effect=self.response
        ):
            return readiness.configured("hosted-64gb", 64)

    def test_accepts_documented_machine_size_details_and_selected_access(self):
        self.assertEqual(self.configured()["runner"]["id"], 5)

    def test_rejects_memory_below_unchanged_floor(self):
        self.runner["machine_size_details"]["memory_gb"] = 32
        with self.assertRaisesRegex(RuntimeError, "below the qualification floor"):
            self.configured()

    def test_rejects_unprovisioned_runner(self):
        self.runner["status"] = "Provisioning"
        with self.assertRaisesRegex(RuntimeError, "not Ready"):
            self.configured()


if __name__ == "__main__":
    unittest.main(verbosity=2)
