import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import conformance from "../contracts/mcp-admission-conformance.v1.json" with { type: "json" };
import { bindMcpServersToRun } from "../src/mcp-admission.mjs";

test("Python and ESM agree on portable vectors, endpoint parsing and malformed admission", () => {
  const inputs = conformance.cases.map((vector) => ({
    servers: [{ ...conformance.server, ...("url" in vector ? { url: vector.url } : {}),
      ...("connectionId" in vector ? { connectionId: vector.connectionId } : {}) }],
    runId: vector.runId, executionHostId: vector.executionHostId,
    policy: { ...conformance.policy, servers: { fixture: { ...conformance.policy.servers.fixture,
      ...("gatewayUrl" in vector ? { gatewayUrl: vector.gatewayUrl } : {}) } } },
  }));
  const baseline = inputs[0];
  for (const endpoint of ["https://worker.example:99999", "http://[0:0:0:0:0:0:0:1]:8000",
    "http://127.0.0.2", "https://worker.123", "https://127.01.0.1", "https://0x7f000001",
    "https://0xworker.example", "https://worker.example./mcp", "https://[::1]/mcp",
    "https://@worker.example", "https://worker.example:/mcp", "http://LOCALHOST:8000/mcp"]) {
    inputs.push({ ...baseline, policy: { version: 1, servers: { fixture: {
      ...baseline.policy.servers.fixture, gatewayUrl: endpoint } } } });
  }
  for (const executionHostIds of ["host-a", null, {}, [], ["host-a\n"], Array(65).fill("host-a")]) {
    inputs.push({ ...baseline, policy: { version: 1, servers: { fixture: {
      ...baseline.policy.servers.fixture, executionHostIds } } } });
  }
  for (const policy of [null, {}, { ...baseline.policy, version: true }, { ...baseline.policy, wildcard: true }]) {
    inputs.push({ ...baseline, policy });
  }
  inputs.push({ ...baseline, servers: [baseline.servers[0], baseline.servers[0]] });
  inputs.push({ ...baseline, servers: Array(9).fill(baseline.servers[0]) });
  const js = inputs.map((input) => {
    try { return { bindings: bindMcpServersToRun(input).map((s) => s.runBinding) }; }
    catch (error) { return { reason: error.reason, code: error.code }; }
  });
  const python = JSON.parse(execFileSync(process.env.HARBOR_TEST_PYTHON ?? "python3", ["-I", "-c", `
import json, sys
sys.path.insert(0, ${JSON.stringify(new URL("../python", import.meta.url).pathname)})
from harbor_llm.mcp_admission import bind_mcp_servers_to_run, McpAdmissionError
out = []
for row in json.load(sys.stdin):
    try:
        result = bind_mcp_servers_to_run(servers=row['servers'], run_id=row['runId'],
                                        execution_host_id=row['executionHostId'], policy=row['policy'])
        out.append({'bindings': [dict(s['runBinding']) for s in result]})
    except McpAdmissionError as error:
        out.append({'reason': error.reason, 'code': error.code})
print(json.dumps(out))
`], { input: JSON.stringify(inputs), encoding: "utf8" }));
  assert.deepEqual(python, js);
});
