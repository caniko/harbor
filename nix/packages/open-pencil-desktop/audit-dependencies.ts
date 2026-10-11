/** Report logical dependency payloads separately from filesystem link metadata. */
import { lstatSync, readlinkSync, readdirSync } from "node:fs";
import { join } from "node:path";

const modules: string[] = [];
function locate(path: string) {
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const child = join(path, entry.name);
    if (entry.name === "node_modules") modules.push(child);
    else locate(child);
  }
}
locate(".");

type Group = {
  hash: InstanceType<typeof Bun.CryptoHasher>;
  files: number;
  directories: number;
  symlinks: number;
  linkedFiles: number;
  bytes: number;
};
const groups = new Map<string, Group>();
function scope(path: string) {
  const packageScope = /(?:^|\/)node_modules\/\.bun\/[^/]+/.exec(path);
  if (packageScope)
    return path.slice(0, packageScope.index + packageScope[0].length);
  const moduleScope = path.match(/^(.*?node_modules)(?:\/|$)/);
  if (!moduleScope)
    throw new Error(`dependency path is outside node_modules: ${path}`);
  return moduleScope[1];
}

async function visit(path: string) {
  const metadata = lstatSync(path);
  const name = scope(path);
  let group = groups.get(name);
  if (!group) {
    group = {
      hash: new Bun.CryptoHasher("sha256"),
      files: 0,
      directories: 0,
      symlinks: 0,
      linkedFiles: 0,
      bytes: 0,
    };
    groups.set(name, group);
  }
  const mode = metadata.mode & 0o777;
  if (metadata.isSymbolicLink()) {
    group.symlinks++;
    group.hash.update(
      JSON.stringify([path, "symlink", mode, readlinkSync(path)]) + "\n",
    );
  } else if (metadata.isDirectory()) {
    group.directories++;
    group.hash.update(JSON.stringify([path, "directory", mode]) + "\n");
    for (const child of readdirSync(path).sort())
      await visit(join(path, child));
  } else if (metadata.isFile()) {
    group.files++;
    group.bytes += metadata.size;
    if (metadata.nlink > 1) group.linkedFiles++;
    const hash = new Bun.CryptoHasher("sha256");
    for await (const chunk of Bun.file(path).stream()) hash.update(chunk);
    group.hash.update(
      JSON.stringify([path, "file", mode, hash.digest("hex")]) + "\n",
    );
  } else {
    throw new Error(`unexpected dependency filesystem entry: ${path}`);
  }
}
for (const path of modules.sort()) await visit(path);
for (const [scope, group] of [...groups].sort(([a], [b]) =>
  a < b ? -1 : a > b ? 1 : 0,
)) {
  const { hash, ...counts } = group;
  console.log(
    "DEPENDENCY_AUDIT " +
      JSON.stringify({ scope, sha256: hash.digest("hex"), ...counts }),
  );
}
