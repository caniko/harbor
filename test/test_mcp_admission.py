"""Portable vectors plus Python boundary attacks; no transport or harness imports."""
import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
from harbor_llm.mcp_admission import (
    McpAdmissionError, bind_mcp_servers_to_run, require_mcp_run_binding,
)

CONTRACT = json.loads((Path(__file__).resolve().parents[1] / "contracts" /
                       "mcp-admission-conformance.v1.json").read_text())


class AdmissionTests(unittest.TestCase):
    def invoke(self, vector=None, **overrides):
        vector = vector or CONTRACT["cases"][0]
        server = {**CONTRACT["server"], **{k: vector[k] for k in ("connectionId", "url") if k in vector}}
        policy = copy.deepcopy(CONTRACT["policy"])
        if "gatewayUrl" in vector:
            policy["servers"]["fixture"]["gatewayUrl"] = vector["gatewayUrl"]
        return bind_mcp_servers_to_run(servers=[server], run_id=vector["runId"],
                                      execution_host_id=vector["executionHostId"], policy=policy, **overrides)

    def test_portable_vectors(self):
        for vector in CONTRACT["cases"]:
            with self.subTest(vector=vector["name"]):
                if "expectedReason" in vector:
                    with self.assertRaises(McpAdmissionError) as caught:
                        self.invoke(vector)
                    self.assertEqual(caught.exception.reason, vector["expectedReason"])
                    self.assertEqual(caught.exception.code, "runtime_mcp_admission_blocked")
                else:
                    self.assertEqual(self.invoke(vector)[0]["runBinding"]["authorizedCrossHost"],
                                     vector["expectedCrossHost"])

    def test_policy_array_and_boolean_types(self):
        for hosts in ("host-a", {"host-a": True}, ["host-a\n"], [], ["host-a"] * 65):
            policy = copy.deepcopy(CONTRACT["policy"])
            policy["servers"]["fixture"]["executionHostIds"] = hosts
            with self.subTest(hosts=hosts), self.assertRaises(McpAdmissionError) as caught:
                bind_mcp_servers_to_run(servers=[CONTRACT["server"]], run_id="run-a",
                                       execution_host_id="host-a", policy=policy)
            self.assertEqual(caught.exception.reason, "invalid_policy")
        policy = {**CONTRACT["policy"], "version": True}
        with self.assertRaises(McpAdmissionError):
            bind_mcp_servers_to_run(servers=[CONTRACT["server"]], run_id="run-a",
                                   execution_host_id="host-a", policy=policy)

    def test_binding_is_frozen_and_rechecked(self):
        binding = self.invoke()[0]["runBinding"]
        expected = dict(run_id="run-a", execution_host_id="host-a", gateway_url=binding["gatewayUrl"])
        self.assertIs(require_mcp_run_binding(binding, **expected), binding)
        with self.assertRaises(TypeError):
            binding["runId"] = "replacement"
        for changes in ({"run_id": "old"}, {"execution_host_id": "host-b"},
                        {"gateway_url": "https://replacement.example"}):
            with self.assertRaises(McpAdmissionError):
                require_mcp_run_binding(binding, **{**expected, **changes})
        for cross in (False, "true", 1, None):
            with self.assertRaises(McpAdmissionError):
                require_mcp_run_binding({**binding, "serverHostId": "host-b", "authorizedCrossHost": cross}, **expected)

    def test_normalizer_redacts_callback_error(self):
        binding = self.invoke()[0]["runBinding"]
        def fail(_):
            raise RuntimeError("private-url-and-token")
        with self.assertRaises(McpAdmissionError) as caught:
            require_mcp_run_binding(binding, run_id="run-a", execution_host_id="host-a",
                                    gateway_url=binding["gatewayUrl"], normalize_recipient=fail)
        self.assertNotIn("private", str(caught.exception))
        self.assertIsNone(caught.exception.__cause__)
        self.assertIsNone(caught.exception.__context__)

    def test_empty_delivery_and_stale_binding(self):
        self.assertEqual(bind_mcp_servers_to_run(servers=[], run_id="", policy=None), [])
        server = {**CONTRACT["server"], "runBinding": {"runId": "stale"}, "token": "private"}
        bound = bind_mcp_servers_to_run(servers=[server], run_id="fresh", execution_host_id="host-a",
                                        policy=CONTRACT["policy"])[0]
        self.assertEqual(bound["runBinding"]["runId"], "fresh")
        self.assertEqual(server["runBinding"]["runId"], "stale")


if __name__ == "__main__":
    unittest.main()
