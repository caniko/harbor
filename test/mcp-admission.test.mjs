import assert from "node:assert/strict";
import { test } from "node:test";
import conformance from "harbor-llm/contracts/mcp-admission-conformance.v1.json" with { type: "json" };
import policySchema from "harbor-llm/contracts/mcp-admission-policy.v1.schema.json" with { type: "json" };
import bindingSchema from "harbor-llm/contracts/mcp-run-binding.v1.schema.json" with { type: "json" };
import {
  bindMcpServersToRun, McpAdmissionError, requireMcpRunBinding,
  mcpAdmissionPolicySchema, mcpRunBindingSchema, MCP_ADMISSION_LIMITS,
} from "harbor-llm/mcp-admission";

const server = {
  connectionId: "fixture-reader", name: "Fixture reader display name",
  url: "https://tools.example/mcp", token: "reader-secret", custom: "retained",
};
const policy = { version: 1, servers: { [server.connectionId]: {
  url: server.url, gatewayUrl: "https://worker.example/profile/main/",
  serverHostId: "worker", executionHostIds: ["worker", "remote"],
} } };
const input = { servers: [server], runId: "run-a", executionHostId: "worker", policy };
const blocked = (reason) => (error) => {
  assert.ok(error instanceof McpAdmissionError);
  assert.equal(error.code, "runtime_mcp_admission_blocked");
  assert.equal(error.reason, reason);
  return true;
};

test("portable schemas match the runtime's contract and limits", () => {
  assert.deepEqual(mcpAdmissionPolicySchema, policySchema);
  assert.deepEqual(mcpRunBindingSchema, bindingSchema);
  assert.equal(policySchema.properties.servers.additionalProperties.properties.executionHostIds.maxItems,
    MCP_ADMISSION_LIMITS.executionHostsPerServer);
  assert.equal(bindingSchema.properties.gatewayUrl.maxLength, MCP_ADMISSION_LIMITS.endpointCharacters);
});

for (const vector of conformance.cases) test(`conforms: ${vector.name}`, () => {
  const fixtureServer = { ...conformance.server,
    ...("connectionId" in vector ? { connectionId: vector.connectionId } : {}),
    ...("url" in vector ? { url: vector.url } : {}),
  };
  const fixturePolicy = { ...conformance.policy, servers: { fixture: {
    ...conformance.policy.servers.fixture,
    ...("gatewayUrl" in vector ? { gatewayUrl: vector.gatewayUrl } : {}),
  } } };
  const invoke = () => bindMcpServersToRun({ servers: [fixtureServer], runId: vector.runId,
    executionHostId: vector.executionHostId, policy: fixturePolicy });
  if ("expectedReason" in vector) assert.throws(invoke, blocked(vector.expectedReason));
  else assert.equal(invoke()[0].runBinding.authorizedCrossHost, vector.expectedCrossHost);
});

test("preserves display names, extensions and exact recipients without provider aliases", () => {
  const [bound] = bindMcpServersToRun(input);
  assert.deepEqual(bound, { ...server, runBinding: {
    runId: "run-a", executionHostId: "worker", serverHostId: "worker",
    gatewayUrl: policy.servers[server.connectionId].gatewayUrl, authorizedCrossHost: false,
  } });
  assert.equal(Object.hasOwn(server, "runBinding"), false);
  assert.equal(Object.isFrozen(bound.runBinding), true);
  const gatewayUrl = "http://127.0.0.1:9119/chat";
  assert.equal(bindMcpServersToRun({ ...input, policy: { ...policy, servers: {
    [server.connectionId]: { ...policy.servers[server.connectionId], gatewayUrl },
  } } })[0].runBinding.gatewayUrl, gatewayUrl);
});

test("cross-host delivery requires the exact host in trusted policy", () => {
  assert.equal(bindMcpServersToRun({ ...input, executionHostId: "remote" })[0].runBinding.authorizedCrossHost, true);
  assert.throws(() => bindMcpServersToRun({ ...input, executionHostId: "other" }), blocked("execution_host_not_authorized"));
});

for (const override of [
  { policy: null }, { policy: {} }, { runId: "bad run" }, { executionHostId: undefined },
  { servers: [{ ...server, url: `${server.url}/` }] },
  { servers: [{ ...server, connectionId: "other" }] },
  { servers: [{ ...server, connectionId: "constructor" }] },
  { servers: [server, server] }, { servers: Array(9).fill(server) },
]) test(`fails closed: ${JSON.stringify(override)}`, () => {
  assert.throws(() => bindMcpServersToRun({ ...input, ...override }), McpAdmissionError);
});

for (const gatewayUrl of [
  "http://worker.example", "https://user:secret@worker.example", "https://worker.example?token=x",
  "https://worker.example#fragment", "file:///worker", " https://worker.example", "https://worker.example/\n",
]) test(`unsafe recipient: ${JSON.stringify(gatewayUrl)}`, () => {
  assert.throws(() => bindMcpServersToRun({ ...input, policy: { ...policy, servers: {
    [server.connectionId]: { ...policy.servers[server.connectionId], gatewayUrl },
  } } }), blocked("unsafe_endpoint"));
});

test("rejects unknown policy fields and inherited decisions", () => {
  for (const invalid of [
    { ...policy, wildcard: true }, { ...policy, version: true },
    { ...policy, servers: { [server.connectionId]: { ...policy.servers[server.connectionId], allowAll: true } } },
    { ...policy, servers: Object.create(policy.servers) },
  ]) assert.throws(() => bindMcpServersToRun({ ...input, policy: invalid }), blocked("invalid_policy"));
});

test("rejects inherited execution-host array entries", () => {
  const executionHostIds = new Array(1);
  Object.setPrototypeOf(executionHostIds, Object.assign(Object.create(Array.prototype), { 0: "worker" }));
  assert.throws(() => bindMcpServersToRun({ ...input, policy: { ...policy, servers: {
    [server.connectionId]: { ...policy.servers[server.connectionId], executionHostIds },
  } } }), blocked("invalid_policy"));
});

test("copies host entries without caller-controlled iterators", () => {
  const executionHostIds = ["worker"];
  executionHostIds[Symbol.iterator] = function* () { yield "unapproved"; };
  const parsed = bindMcpServersToRun({ ...input, policy: { ...policy, servers: {
    [server.connectionId]: { ...policy.servers[server.connectionId], executionHostIds },
  } } });
  assert.equal(parsed[0].runBinding.executionHostId, "worker");
});

test("requires no policy for an empty delivery", () => {
  assert.deepEqual(bindMcpServersToRun({ servers: [], runId: "", policy: null }), []);
});

test("replaces stale metadata without mutating the input", () => {
  const stale = { ...server, runBinding: { runId: "old-run" } };
  assert.equal(bindMcpServersToRun({ ...input, servers: [stale], runId: "new-run" })[0].runBinding.runId, "new-run");
  assert.equal(stale.runBinding.runId, "old-run");
});

test("rechecks run, execution identity and recipient at the consumer boundary", () => {
  const [bound] = bindMcpServersToRun(input);
  const expected = { runId: "run-a", executionHostId: "worker", gatewayUrl: bound.runBinding.gatewayUrl };
  assert.equal(requireMcpRunBinding(bound.runBinding, expected), bound.runBinding);
  for (const override of [
    { runId: "run-b" }, { executionHostId: "remote" }, { gatewayUrl: "https://replacement.example/" },
  ]) assert.throws(() => requireMcpRunBinding(bound.runBinding, { ...expected, ...override }), blocked("binding_mismatch"));
  assert.throws(() => requireMcpRunBinding(undefined, expected), blocked("binding_mismatch"));
  assert.throws(() => requireMcpRunBinding({ ...bound.runBinding, serverHostId: "remote", authorizedCrossHost: false }, expected), blocked("binding_mismatch"));
});

test("requires an own cross-host approval under prototype pollution", () => {
  const [bound] = bindMcpServersToRun(input);
  const binding = { runId: "run-a", executionHostId: "worker", serverHostId: "remote", gatewayUrl: bound.runBinding.gatewayUrl };
  const previous = Object.getOwnPropertyDescriptor(Object.prototype, "authorizedCrossHost");
  try {
    Object.defineProperty(Object.prototype, "authorizedCrossHost", { value: true, configurable: true });
    assert.throws(() => requireMcpRunBinding(binding, binding), blocked("binding_mismatch"));
    assert.equal(requireMcpRunBinding({ ...binding, authorizedCrossHost: true }, binding).authorizedCrossHost, true);
  } finally {
    if (previous) Object.defineProperty(Object.prototype, "authorizedCrossHost", previous);
    else Reflect.deleteProperty(Object.prototype, "authorizedCrossHost");
  }
});

for (const throwOnCall of [1, 2]) test(`normalizer exception on call ${throwOnCall} stays content-free`, () => {
  const [bound] = bindMcpServersToRun(input);
  let calls = 0;
  assert.throws(() => requireMcpRunBinding(bound.runBinding, { ...bound.runBinding, normalizeRecipient: (url) => {
    if (++calls === throwOnCall) throw new Error(`private recipient: ${url}`);
    return url;
  } }), (error) => {
    blocked("binding_mismatch")(error);
    assert.equal(String(error).includes(bound.runBinding.gatewayUrl), false);
    assert.equal(String(error).includes("private recipient"), false);
    assert.equal(Object.hasOwn(error, "cause"), false);
    return true;
  });
});

test("generic errors omit endpoints and caller credentials", () => {
  assert.throws(() => bindMcpServersToRun({ ...input, servers: [{ ...server, url: "https://unapproved.example/secret" }] }), (error) => {
    blocked("endpoint_mismatch")(error);
    assert.equal(String(error).includes("reader-secret"), false);
    assert.equal(String(error).includes("unapproved.example"), false);
    return true;
  });
});
