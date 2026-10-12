import { afterEach, expect, test } from "bun:test";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readlinkSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const roots: string[] = [];
afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true });
});

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "open-pencil-links-"));
  roots.push(root);
  const store = join(root, "node_modules/.bun");
  const manifest = (name: string, value: object) => {
    const path = join(store, name);
    mkdirSync(path, { recursive: true });
    writeFileSync(join(path, "package.json"), JSON.stringify(value));
  };
  manifest("react-is@19.2.4/node_modules/react-is", {
    name: "react-is",
    version: "19.2.4",
  });
  mkdirSync(join(store, "node_modules"));
  manifest("eslint@10.4.0/node_modules/eslint", {
    name: "eslint",
    version: "10.4.0",
    bin: { eslint: "bin/eslint.js" },
  });
  const eslint = join(store, "eslint@10.4.0/node_modules/eslint/bin/eslint.js");
  mkdirSync(join(eslint, ".."));
  writeFileSync(eslint, "#!/usr/bin/env node\n");
  chmodSync(eslint, 0o755);
  const eslintPeer = join(
    store,
    "@eslint-community+eslint-utils@4.9.1+fixture/node_modules",
  );
  mkdirSync(eslintPeer, { recursive: true });
  symlinkSync(
    "../../eslint@10.4.0/node_modules/eslint",
    join(eslintPeer, "eslint"),
  );
  manifest("browserslist@4.28.8+fixture/node_modules/browserslist", {
    name: "browserslist",
    version: "4.28.8",
    bin: { browserslist: "cli.js" },
  });
  const executable = join(
    store,
    "browserslist@4.28.8+fixture/node_modules/browserslist/cli.js",
  );
  writeFileSync(
    executable,
    "#!/usr/bin/env node\nconsole.log('browserslist');\n",
  );
  chmodSync(executable, 0o755);
  const peer = join(store, "update-browserslist-db@1.3.1+fixture/node_modules");
  mkdirSync(join(peer, ".bin"), { recursive: true });
  symlinkSync(
    "../../browserslist@4.28.8+fixture/node_modules/browserslist",
    join(peer, "browserslist"),
  );
  writeFileSync(
    join(root, "bun.lock"),
    JSON.stringify({
      packages: {
        "react-is": ["react-is@19.2.4"],
        browserslist: [
          "browserslist@4.28.8",
          "",
          { bin: { browserslist: "cli.js" } },
        ],
      },
    }),
  );
  const run = () =>
    Bun.spawnSync({
      cmd: [process.execPath, join(import.meta.dir, "normalize-links.ts")],
      cwd: root,
    });
  return { peer, executable, run };
}

test("restores the missing frozen Browserslist peer bin without changing package bytes", () => {
  const { peer, executable, run } = fixture();
  const before = readFileSync(executable);
  expect(run().exitCode).toBe(0);
  expect(existsSync(join(peer, ".bin/browserslist"))).toBe(true);
  expect(readlinkSync(join(peer, ".bin/browserslist"))).toBe(
    "../browserslist/cli.js",
  );
  expect(readFileSync(executable)).toEqual(before);
  expect(run().exitCode).toBe(0);
  expect(readlinkSync(join(peer, ".bin/browserslist"))).toBe(
    "../browserslist/cli.js",
  );
});

test("refuses to replace an unrelated regular file with the peer bin", () => {
  const { peer, run } = fixture();
  const bin = join(peer, ".bin/browserslist");
  writeFileSync(bin, "operator file");
  expect(run().exitCode).not.toBe(0);
  expect(readFileSync(bin, "utf8")).toBe("operator file");
});

test("rejects a resolved Browserslist version outside the frozen lock", () => {
  const { peer, run } = fixture();
  writeFileSync(
    join(peer, "browserslist/package.json"),
    JSON.stringify({
      name: "browserslist",
      version: "0.0.0",
      bin: { browserslist: "cli.js" },
    }),
  );
  expect(run().exitCode).not.toBe(0);
  expect(existsSync(join(peer, ".bin/browserslist"))).toBe(false);
});
