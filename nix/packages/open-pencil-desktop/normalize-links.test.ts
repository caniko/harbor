import { afterEach, expect, test } from "bun:test";
import {
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
  const root = mkdtempSync(join(tmpdir(), "open-pencil-peer-bins-"));
  roots.push(root);
  const store = join(root, "node_modules/.bun");
  const packages = {
    "react-is": { version: "19.2.4", bin: {} },
    eslint: { version: "10.4.0", bin: { eslint: "bin/eslint.js" } },
    browserslist: { version: "4.28.8", bin: { browserslist: "cli.js" } },
    "update-browserslist-db": {
      version: "1.3.1",
      bin: { "update-browserslist-db": "cli.js" },
    },
  };
  const scopes = new Map<string, string>();
  for (const [name, { version, bin }] of Object.entries(packages)) {
    const suffix = name.includes("browserslist") ? "+fixture" : "";
    const scope = join(store, `${name}@${version}${suffix}/node_modules`);
    const directory = join(scope, name);
    scopes.set(name, scope);
    mkdirSync(directory, { recursive: true });
    writeFileSync(
      join(directory, "package.json"),
      JSON.stringify({
        name,
        version,
        bin:
          name === "update-browserslist-db"
            ? "cli.js"
            : name === "eslint"
              ? { eslint: "./bin/eslint.js" }
              : bin,
      }),
    );
    for (const executable of Object.values(bin)) {
      const path = join(directory, executable);
      mkdirSync(join(path, ".."), { recursive: true });
      writeFileSync(path, "#!/usr/bin/env node\n");
    }
  }
  const peers = [
    ["@eslint-community+eslint-utils@4.9.1+fixture", "eslint"],
    ["update-browserslist-db@1.3.1+fixture", "browserslist"],
    ["browserslist@4.28.8+fixture", "update-browserslist-db"],
  ];
  for (const [context, dependency] of peers) {
    const scope = join(store, context, "node_modules");
    mkdirSync(scope, { recursive: true });
    symlinkSync(
      scopes.get(dependency) + "/" + dependency,
      join(scope, dependency),
    );
  }
  mkdirSync(join(store, "node_modules"));
  writeFileSync(
    join(root, "bun.lock"),
    JSON.stringify({
      packages: Object.fromEntries(
        Object.entries(packages).map(([name, { version, bin }]) => [
          name,
          [`${name}@${version}`, "", { bin }],
        ]),
      ),
    }),
  );
  const scope = scopes.get("browserslist")!;
  const bin = join(scope, ".bin/update-browserslist-db");
  const executable = join(scope, "update-browserslist-db/cli.js");
  const run = () =>
    Bun.spawnSync({
      cmd: [process.execPath, join(import.meta.dir, "normalize-links.ts")],
      cwd: root,
    });
  return { bin, executable, run };
}

test("restores the reciprocal circular peer executable without changing package bytes", () => {
  const { bin, executable, run } = fixture();
  const before = readFileSync(executable);
  expect(run().exitCode).toBe(0);
  expect(readlinkSync(bin)).toBe("../update-browserslist-db/cli.js");
  expect(readFileSync(executable)).toEqual(before);
  expect(run().exitCode).toBe(0);
  expect(readlinkSync(bin)).toBe("../update-browserslist-db/cli.js");
});

test("refuses to overwrite a regular file at the reciprocal peer executable", () => {
  const { bin, run } = fixture();
  mkdirSync(join(bin, ".."), { recursive: true });
  writeFileSync(bin, "foreign file");
  expect(run().exitCode).not.toBe(0);
  expect(readFileSync(bin, "utf8")).toBe("foreign file");
});
