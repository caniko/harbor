import assert from "node:assert/strict";
import test from "node:test";

test("Node runtime is available", () => {
  assert.ok(process.versions.node);
});
