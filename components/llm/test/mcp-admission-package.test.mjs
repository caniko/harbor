import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

test("packed MCP exports work without OpenCode or installed dependencies", async (t) => {
  const root = await mkdtemp(path.join(tmpdir(), "harbor-mcp-package-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const repo = fileURLToPath(new URL("..", import.meta.url));
  const [packed] = JSON.parse(execFileSync("npm", ["pack", "--ignore-scripts", "--json", "--pack-destination", root],
    { cwd: repo, encoding: "utf8" }));
  const packageRoot = path.join(root, "node_modules", "harbor-llm");
  await mkdir(packageRoot, { recursive: true });
  execFileSync("tar", ["-xzf", path.join(root, packed.filename), "--strip-components=1", "-C", packageRoot]);
  const result = execFileSync(process.execPath, ["--input-type=module", "-e", `
    import assert from "node:assert/strict";
    import { bindMcpServersToRun, requireMcpRunBinding } from "harbor-llm/mcp-admission";
    import { AuthenticatedChannel } from "harbor-llm/execution-channel";
    import { frame, FrameDecoder } from "harbor-llm/execution-framing";
    import { runJson } from "harbor-llm/transport";
    import vectors from "harbor-llm/contracts/mcp-admission-conformance.v1.json" with { type: "json" };
    const [server] = bindMcpServersToRun({servers: [vectors.server], policy: vectors.policy,
      runId: "packed-run", executionHostId: "host-a"});
    assert.equal(requireMcpRunBinding(server.runBinding, server.runBinding).runId, "packed-run");
    const options = { key: Buffer.alloc(32, 1), nonce: Buffer.alloc(32, 2), binding: Buffer.alloc(32, 3) };
    const controller = new AuthenticatedChannel({...options, role: "controller"});
    const bridge = new AuthenticatedChannel({...options, role: "bridge"});
    const message = { schemaVersion: 1, requestId: "standalone", body: { kind: "error", code: "effect-unknown" } };
    assert.deepEqual(bridge.open(controller.seal(message)), message);
    const decoder = new FrameDecoder();
    assert.deepEqual(decoder.push(frame(message)), [message]);
    decoder.end(); controller.close(); bridge.close();
    assert.deepEqual(await runJson(process.execPath, ["-e", "process.stdout.write('{}')"], {
      limits: { timeoutMs: 5000, inputBytes: 1024, outputBytes: 1024, stderrBytes: 1024 },
    }), {});
    console.log("isolated MCP package accepted");
  `], { cwd: root, encoding: "utf8" });
  assert.match(result, /isolated MCP package accepted/);
  for (const file of ["src/mcp-admission.d.mts", "src/mcp-admission-schema.d.mts",
    "src/mcp-admission.LICENSE", "contracts/mcp-run-binding.v1.schema.json"]) {
    assert.ok(packed.files.some((entry) => entry.path === file), `${file} must survive packing`);
  }
});
