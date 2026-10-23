import assert from "node:assert/strict";
import { test } from "node:test";
import { greet } from "./greet.ts";

test("greet", () => {
  assert.equal(greet("x"), "hello, x");
});
