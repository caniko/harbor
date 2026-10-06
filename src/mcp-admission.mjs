// SPDX-License-Identifier: MIT — see mcp-admission.LICENSE.
import { MCP_ADMISSION_IDENTIFIER_PATTERN, MCP_ADMISSION_LIMITS } from "./mcp-admission-schema.mjs";
export { MCP_ADMISSION_LIMITS, mcpAdmissionPolicySchema, mcpRunBindingSchema } from "./mcp-admission-schema.mjs";

const remedies = {
  invalid_identity: "Select an explicit run and execution identity.",
  invalid_policy: "Provide a bounded version-1 operator-owned endpoint and host policy.",
  too_many_servers: "Reduce the admitted server set to the per-run limit.",
  duplicate_server: "Resolve each connection exactly once.",
  server_not_authorized: "Ask the operator to authorize the exact resolved connection.",
  endpoint_mismatch: "Resolve a fresh connection matching the operator-approved endpoint.",
  execution_host_not_authorized: "Ask the operator to authorize the selected execution host.",
  unsafe_endpoint: "Use an exact HTTPS or loopback HTTP endpoint without credentials, query or fragment.",
  binding_mismatch: "Resolve fresh admission for this run, execution host and credential recipient.",
};

export class McpAdmissionError extends Error {
  code = "runtime_mcp_admission_blocked";
  constructor(reason) {
    super(`Runtime MCP admission blocked. ${remedies[reason]}`);
    this.name = "McpAdmissionError";
    this.reason = reason;
  }
}

const identifierPattern = new RegExp(MCP_ADMISSION_IDENTIFIER_PATTERN);
export function isMcpAdmissionIdentifier(value) {
  return typeof value === "string" && identifierPattern.test(value);
}

function record(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && [Object.prototype, null].includes(Object.getPrototypeOf(value));
}

function keysAre(value, required, optional = []) {
  return required.every((key) => Object.hasOwn(value, key))
    && Object.keys(value).every((key) => required.includes(key) || optional.includes(key));
}

function endpointString(value) {
  return typeof value === "string" && value.length > 0 && value.length <= MCP_ADMISSION_LIMITS.endpointCharacters;
}

/** Transport syntax only; no DNS resolution, grants or physical-host attestation. */
export function requireMcpCredentialEndpoint(value) {
  if (!endpointString(value) || /[^\x21-\x7e]/.test(value)
    || !/^https?:\/\//i.test(value) || /[\\?#]/.test(value)) throw new McpAdmissionError("unsafe_endpoint");
  let url;
  try { url = new URL(value); } catch { throw new McpAdmissionError("unsafe_endpoint"); }
  const authority = value.slice(value.indexOf("//") + 2).split("/")[0];
  if (!url.hostname || authority.includes("@") || url.username || url.password
    || (url.protocol !== "https:" && !(url.protocol === "http:"
      && ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname)))) throw new McpAdmissionError("unsafe_endpoint");
  return value;
}

/** Parse trusted operator metadata without interpreting a consumer's domain grants. */
export function parseMcpAdmissionPolicy(value) {
  if (!record(value) || !keysAre(value, ["version", "servers"]) || value.version !== 1 || !record(value.servers)) {
    throw new McpAdmissionError("invalid_policy");
  }
  try {
    if (new TextEncoder().encode(JSON.stringify(value)).length > MCP_ADMISSION_LIMITS.policyBytes) throw new Error();
  } catch { throw new McpAdmissionError("invalid_policy"); }
  const entries = Object.entries(value.servers).map(([id, rule]) => {
    if (!isMcpAdmissionIdentifier(id) || !record(rule) || !keysAre(rule, ["url", "gatewayUrl", "serverHostId", "executionHostIds"])
      || !endpointString(rule.url) || !endpointString(rule.gatewayUrl) || !isMcpAdmissionIdentifier(rule.serverHostId)
      || !Array.isArray(rule.executionHostIds) || rule.executionHostIds.length < 1
      || rule.executionHostIds.length > MCP_ADMISSION_LIMITS.executionHostsPerServer) throw new McpAdmissionError("invalid_policy");
    const executionHostIds = [];
    for (let index = 0; index < rule.executionHostIds.length; index++) {
      if (!Object.hasOwn(rule.executionHostIds, index)) throw new McpAdmissionError("invalid_policy");
      const hostId = rule.executionHostIds[index];
      if (!isMcpAdmissionIdentifier(hostId)) throw new McpAdmissionError("invalid_policy");
      executionHostIds.push(hostId);
    }
    return [id, { url: rule.url, gatewayUrl: rule.gatewayUrl, serverHostId: rule.serverHostId, executionHostIds }];
  });
  return { version: 1, servers: Object.fromEntries(entries) };
}

/** Bind resolved servers using trusted policy; no mutation, I/O or retained state. */
export function bindMcpServersToRun(input) {
  if (!input.servers.length) return [];
  const { executionHostId, runId } = input;
  if (!isMcpAdmissionIdentifier(executionHostId) || !isMcpAdmissionIdentifier(runId)) throw new McpAdmissionError("invalid_identity");
  if (input.servers.length > MCP_ADMISSION_LIMITS.serversPerRun) throw new McpAdmissionError("too_many_servers");
  const policy = parseMcpAdmissionPolicy(input.policy);
  const ids = new Set();
  return input.servers.map((server) => {
    if (!isMcpAdmissionIdentifier(server.connectionId)) throw new McpAdmissionError("invalid_identity");
    if (ids.has(server.connectionId)) throw new McpAdmissionError("duplicate_server");
    ids.add(server.connectionId);
    if (!Object.hasOwn(policy.servers, server.connectionId)) throw new McpAdmissionError("server_not_authorized");
    const rule = policy.servers[server.connectionId];
    if (rule.url !== server.url) throw new McpAdmissionError("endpoint_mismatch");
    if (!rule.executionHostIds.includes(executionHostId)) throw new McpAdmissionError("execution_host_not_authorized");
    requireMcpCredentialEndpoint(rule.url);
    requireMcpCredentialEndpoint(rule.gatewayUrl);
    return { ...server, runBinding: Object.freeze({ runId, executionHostId,
      serverHostId: rule.serverHostId, gatewayUrl: rule.gatewayUrl,
      authorizedCrossHost: rule.serverHostId !== executionHostId }) };
  });
}

/** Consumer recheck; raw URLs are validated before consumer-owned alias rules. */
export function requireMcpRunBinding(value, expected) {
  if (!record(value) || !keysAre(value, ["runId", "executionHostId", "serverHostId", "gatewayUrl"], ["authorizedCrossHost"])
    || !isMcpAdmissionIdentifier(value.runId) || !isMcpAdmissionIdentifier(value.executionHostId)
    || !isMcpAdmissionIdentifier(value.serverHostId)
    || ("authorizedCrossHost" in value && (!Object.hasOwn(value, "authorizedCrossHost") || typeof value.authorizedCrossHost !== "boolean"))
    || value.runId !== expected.runId || value.executionHostId !== expected.executionHostId
    || (value.serverHostId !== value.executionHostId && value.authorizedCrossHost !== true)) throw new McpAdmissionError("binding_mismatch");
  const approved = requireMcpCredentialEndpoint(value.gatewayUrl);
  const recipient = requireMcpCredentialEndpoint(expected.gatewayUrl);
  const normalize = expected.normalizeRecipient ?? ((url) => url);
  try {
    const normalizedApproved = normalize(approved);
    const normalizedRecipient = normalize(recipient);
    if (!normalizedApproved || normalizedApproved !== normalizedRecipient) throw new McpAdmissionError("binding_mismatch");
  } catch { throw new McpAdmissionError("binding_mismatch"); }
  return value;
}
