{
  url,
  sha256Path,
  name ? "gradle-cache.tar",
  pkgs,
  lib ? pkgs.lib,
}:
assert lib.assertMsg (builtins.isString url && url != "")
"rs-harbor: fetchMavenCache `url` must be a non-empty string";
assert lib.assertMsg (builtins.isPath sha256Path || builtins.isString sha256Path)
"rs-harbor: fetchMavenCache `sha256Path` must be a path or string";
assert lib.assertMsg (builtins.isString name && name != "")
"rs-harbor: fetchMavenCache `name` must be a non-empty string"; let
  rawHash =
    if builtins.pathExists sha256Path
    then builtins.readFile sha256Path
    else null;
  hashMatch =
    if rawHash == null
    then null
    else builtins.match "[[:space:]]*([^\n\r[:space:]]+).*" rawHash;
  hash =
    if rawHash == null
    then null
    else if hashMatch == null
    then throw "rs-harbor: fetchMavenCache `${toString sha256Path}` is empty or contains no hash"
    else builtins.head hashMatch;
  validHash =
    hash
    == null
    || builtins.match "[0-9a-fA-F]{64}" hash != null
    || builtins.match "[0-9abcdfghijklmnpqrsvwxyz]{52}" hash != null
    || builtins.match "sha256-[A-Za-z0-9+/]{43}=" hash != null;
  sriHash =
    if hash == null
    then null
    else if !validHash
    then throw "rs-harbor: fetchMavenCache `${toString sha256Path}` does not contain a valid SHA-256 hash"
    else
      builtins.convertHash {
        inherit hash;
        hashAlgo = "sha256";
        toHashFormat = "sri";
      };
in
  if hash == null
  then null
  else
    pkgs.fetchurl {
      inherit url name;
      hash = sriHash;
    }
