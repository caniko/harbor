import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { OpenCodeV2 } from "../src/execution/opencode-v2.mjs";

const limits = { timeoutMs: 5000, inputBytes: 4096, outputBytes: 4096, totalResponseBytes: 65536, requests: 32, pages: 4, messages: 8 };
const session = { id: "ses_fixture", location: { directory: "/sealed" }, time: { updated: 1 } };
const prompt = { id: "inb_fixture", text: "synthetic fixture", delivery: "queue", resume: false };
const receipt = { id: prompt.id, sessionID: session.id, type: "user", payload: { text: prompt.text }, delivery: "queue" };
async function server(t, respond) {
  const requests = [];
  const http = createServer(async (req, res) => {
    let raw = "";
    for await (const bytes of req) raw += bytes;
    requests.push({ method: req.method, url: req.url, authorization: req.headers.authorization, body: raw ? JSON.parse(raw) : undefined });
    try { await respond(req, res, requests.at(-1), requests.length); }
    catch { res.destroy(); }
  });
  await new Promise(resolve => http.listen(0, "127.0.0.1", resolve));
  t.after(() => { http.closeAllConnections(); http.close(); });
  const adapter = new OpenCodeV2({ url: `http://127.0.0.1:${http.address().port}`, authorization: "Basic fixture-secret",
    server: { pid: 123, version: "2.0.22" }, sessionID: session.id, directory: "/sealed", limits });
  return { adapter, requests, url: `http://127.0.0.1:${http.address().port}` };
}
function json(res, data) { res.setHeader("content-type", "application/json"); res.end(JSON.stringify(data)); }
function respond(req, res) {
  if (req.url === "/api/info") return json(res, { version: "2.0.22", pid: 123 });
  if (req.method === "POST" && req.url === "/api/session") return json(res, { data: session });
  if (req.method === "POST" && req.url.endsWith("/prompt")) return json(res, { data: receipt });
  if (req.method === "POST" && req.url.endsWith("/interrupt?resume=false")) return json(res, { interrupted: true });
  if (req.url === "/api/session/ses_fixture") return json(res, { data: session });
  if (req.url.endsWith("/inbox")) return json(res, { data: [] });
  if (req.url.includes("/message?")) return json(res, { data: [{ id: "msg_fixture", type: "user", text: prompt.text }], cursor: {} });
  res.statusCode = 404; json(res, {});
}

test("real published client creates and submits once with explicit authority and resume disabled", async t => {
  const { adapter, requests } = await server(t, respond);
  assert.equal(requests.length, 0);
  await adapter.probe();
  assert.equal((await adapter.start({ model: { providerID: "synthetic", id: "fixture" }, permissions: [] })).id, session.id);
  assert.equal((await adapter.submit(prompt)).id, prompt.id);
  await assert.rejects(adapter.submit(prompt), { code: "effect-unknown" });
  assert.equal(requests.filter(r => r.method === "POST" && r.url.endsWith("/prompt")).length, 1);
  assert.deepEqual(requests.at(-1).body, prompt);
  assert.ok(requests.every(r => r.authorization === "Basic fixture-secret"));
});

test("reopen and reconciliation issue only GETs and retain explicit observation gaps", async t => {
  const { adapter, requests } = await server(t, respond);
  await adapter.probe();
  const before = requests.length;
  const snapshot = await adapter.reconcile();
  assert.equal(snapshot.gap, true);
  assert.equal(snapshot.messages[0].id, "msg_fixture");
  assert.equal(snapshot.complete, false);
  assert.ok(requests.slice(before).every(r => r.method === "GET"));
  await assert.rejects(adapter.submit(prompt), { code: "effect-unknown" });
});

test("lost and malformed mutation replies remain UNKNOWN with exactly one POST", async t => {
  for (const fault of ["lost", "wrong-binding"]) {
    const { adapter, requests } = await server(t, (req, res) => {
      if (req.url.endsWith("/prompt")) {
        if (fault === "lost") return res.destroy();
        return json(res, { data: { ...receipt, sessionID: "ses_other" } });
      }
      respond(req, res);
    });
    await adapter.probe(); await adapter.start({ model: { providerID: "synthetic", id: "fixture" }, permissions: [] });
    await assert.rejects(adapter.submit(prompt), { code: "effect-unknown" });
    await assert.rejects(adapter.submit(prompt), { code: "effect-unknown" });
    assert.equal(requests.filter(r => r.url.endsWith("/prompt")).length, 1);
  }
});

test("redirects never forward server credentials and readiness identity drift fails before POST", async t => {
  const { adapter, requests } = await server(t, (req, res) => { res.writeHead(302, { location: "http://127.0.0.1:1/stolen" }); res.end(); });
  await assert.rejects(adapter.probe(), { code: "transport-failed" });
  await assert.rejects(adapter.start({}), { code: "custody-lost" });
  assert.equal(requests.length, 1);
  const other = await server(t, (req, res) => json(res, { pid: 124, version: "2.0.22" }));
  await assert.rejects(other.adapter.probe(), { code: "custody-lost" });
  await assert.rejects(other.adapter.start({}), { code: "custody-lost" });
  assert.ok(other.requests.every(r => r.method === "GET"));
});

test("cursor loops, changed snapshots and output overflow never yield complete reconciliation", async t => {
  for (const fault of ["cursor-loop", "drift", "overflow"]) {
    let reads = 0;
    const { adapter } = await server(t, (req, res) => {
      if (req.url.includes("/message?")) {
        if (fault === "overflow") return json(res, { padding: "x".repeat(5000) });
        return json(res, { data: [{ id: `msg_${fault === "drift" ? ++reads : "fixture"}`, type: "user" }], cursor: fault === "cursor-loop" ? { next: "same" } : {} });
      }
      respond(req, res);
    });
    await adapter.probe();
    await assert.rejects(adapter.reconcile());
  }
});

test("all transcript pages are read twice and kept with stable unique identities", async t => {
  const { adapter, requests } = await server(t, (req, res) => {
    if (req.url.includes("/message?")) {
      const second = req.url.includes("cursor=after-one");
      // The selected engine rejects order when a cursor already carries it.
      if (second && new URL(req.url, "http://fixture").searchParams.has("order")) {
        res.statusCode = 400;
        return json(res, { message: "Cursor cannot be combined with order" });
      }
      return json(res, { data: [{ id: second ? "msg_two" : "msg_one", type: "user" }], cursor: second ? {} : { next: "after-one" } });
    }
    respond(req, res);
  });
  await adapter.probe();
  const snapshot = await adapter.reconcile();
  assert.deepEqual(snapshot.messages.map(message => message.id), ["msg_one", "msg_two"]);
  assert.equal(requests.filter(r => r.url.includes("cursor=after-one")).length, 2);
});

test("invalid inputs have no effect and HTTP error details never enter diagnostics", async t => {
  const { adapter, requests } = await server(t, (req, res) => {
    if (req.url.endsWith("/prompt")) { res.statusCode = 400; return json(res, { message: "prompt-and-credential-secret" }); }
    respond(req, res);
  });
  await adapter.probe();
  await assert.rejects(adapter.start(null), { code: "invalid-frame" });
  await adapter.start({ model: { providerID: "synthetic", id: "fixture" }, permissions: [] });
  const before = requests.length;
  await assert.rejects(adapter.submit(null), { code: "invalid-frame" });
  await assert.rejects(adapter.submit({ ...prompt, resume: true }), { code: "invalid-frame" });
  assert.equal(requests.length, before);
  await assert.rejects(adapter.submit(prompt), error => error.code === "effect-unknown" && !JSON.stringify(error).includes("secret") && !String(error).includes("secret"));
  assert.equal(JSON.stringify(adapter), "{}");
});

test("abort terminates stalled read-only HTTP without resubmission", { timeout: 5000 }, async t => {
  let reached;
  const connected = new Promise(resolve => { reached = resolve; });
  const { url, requests } = await server(t, () => { reached(); });
  const signal = new AbortController();
  const adapter = new OpenCodeV2({ url, authorization: "Basic fixture-secret", server: { pid: 123, version: "2.0.22" },
    sessionID: session.id, directory: "/sealed", limits, signal: signal.signal });
  const pending = adapter.probe();
  await connected;
  signal.abort();
  await assert.rejects(pending, { code: "transport-failed" });
  assert.equal(requests.length, 1);
  adapter.close();
  await assert.rejects(adapter.probe(), { code: "transport-failed" });
});

test("aggregate HTTP request and byte budgets stop observation without mutation", async t => {
  for (const budget of [{ requests: 2 }, { totalResponseBytes: 40 }]) {
    const { url, requests } = await server(t, respond);
    const adapter = new OpenCodeV2({ url, authorization: "Basic fixture-secret", server: { pid: 123, version: "2.0.22" },
      sessionID: session.id, directory: "/sealed", limits: { ...limits, ...budget } });
    await adapter.probe();
    await assert.rejects(adapter.reconcile(), { code: "budget-exhausted" });
    assert.equal(requests.length, 2);
    assert.ok(requests.every(request => request.method === "GET"));
  }
});
