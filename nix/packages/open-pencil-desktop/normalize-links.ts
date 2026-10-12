/** Repair Bun 1.4.2's racy links in this frozen OpenPencil graph.
 * Package contents and dependency edges remain untouched. The lock's unaliased
 * react-is entry owns the shared name; npm aliases keep their own local links.
 * ESLint and Browserslist peers resolve correctly, but Bun can omit their bins.
 */
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  symlinkSync,
  unlinkSync,
} from "node:fs";
import { join, relative } from "node:path";

function link(path: string, target: string) {
  if (lstatSync(path, { throwIfNoEntry: false })?.isSymbolicLink()) {
    unlinkSync(path);
  } else if (existsSync(path)) {
    throw new Error(`refusing to replace a non-symlink: ${path}`);
  }
  symlinkSync(target, path);
}

const lock = Bun.JSONC.parse(readFileSync("bun.lock", "utf8"));
const store = "node_modules/.bun";
// An alias such as react-is-18 must not win the shared react-is name.
const identity: string = lock.packages["react-is"][0];
if (!identity.startsWith("react-is@")) {
  throw new Error("unexpected unaliased react-is lock entry");
}
const version = identity.slice("react-is@".length);
const target = join(store, `${identity}/node_modules/react-is`);
const manifest = JSON.parse(readFileSync(join(target, "package.json"), "utf8"));
if (manifest.name !== "react-is" || manifest.version !== version) {
  throw new Error("installed react-is does not match the frozen lock");
}
link(
  join(store, "node_modules/react-is"),
  relative(join(store, "node_modules"), target),
);

// The peer-context suffix is Bun-owned; require exactly one matching scope.
const scopes = readdirSync(store).filter((name) =>
  name.startsWith("@eslint-community+eslint-utils@"),
);
if (scopes.length !== 1) {
  throw new Error(`unexpected ESLint utility peer contexts: ${scopes}`);
}
const scope = join(store, scopes[0], "node_modules");
const eslint = JSON.parse(
  readFileSync(join(scope, "eslint/package.json"), "utf8"),
);
const executable = eslint.bin.eslint;
if (!lstatSync(join(scope, "eslint", executable)).isFile()) {
  throw new Error("missing resolved ESLint executable");
}
mkdirSync(join(scope, ".bin"), { recursive: true });
link(join(scope, ".bin/eslint"), `../eslint/${executable}`);

// The circular Browserslist/update-db peer graph has the same bin-link race.
const updateScopes = readdirSync(store).filter((name) =>
  name.startsWith("update-browserslist-db@"),
);
if (updateScopes.length !== 1) {
  throw new Error(
    `unexpected Browserslist update peer contexts: ${updateScopes}`,
  );
}
const updateScope = join(store, updateScopes[0], "node_modules");
const browserslist = JSON.parse(
  readFileSync(join(updateScope, "browserslist/package.json"), "utf8"),
);
if (
  browserslist.name !== "browserslist" ||
  `browserslist@${browserslist.version}` !== lock.packages.browserslist[0]
) {
  throw new Error("resolved Browserslist peer does not match the frozen lock");
}
const browserslistBin = browserslist.bin.browserslist;
if (
  browserslistBin !== lock.packages.browserslist[2].bin.browserslist ||
  !lstatSync(join(updateScope, "browserslist", browserslistBin)).isFile()
) {
  throw new Error("missing frozen Browserslist executable");
}
mkdirSync(join(updateScope, ".bin"), { recursive: true });
link(
  join(updateScope, ".bin/browserslist"),
  `../browserslist/${browserslistBin}`,
);
