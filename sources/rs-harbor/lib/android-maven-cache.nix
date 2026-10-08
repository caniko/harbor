# findLocalMavenCache :: {
#   sha256Path, hostPath,
#   name?, recursive?,
# } -> path or null
#
# Return a store path for a host-local Maven cache tarball when both the
# committed hash file and the tarball exist. This is for Android APK builders
# that keep large generated Maven cache archives outside git while pinning the
# content hash in source. `recursive = false` uses the flat-file hash produced
# by `sha256sum` or `nix hash file`; set it to true only when the hash file
# contains a recursive/NAR path hash. The result can be passed directly as
# `mavenCacheTar` to `mkAndroidApk`.
#
# Example:
#
#   mavenCacheTar = rs-harbor.lib.findLocalMavenCache {
#     sha256Path = ./nix/android/gradle-cache-sha256;
#     hostPath = "/abs/project/nix/android/gradle-cache.tar";
#     name = "my-game-gradle-cache.tar";
#   };
{
  sha256Path,
  hostPath,
  name ? "gradle-cache.tar",
  recursive ? false,
  lib ? {
    assertMsg = assertion: message:
      if assertion
      then true
      else builtins.throw message;
    hasPrefix = prefix: value:
      builtins.substring 0 (builtins.stringLength prefix) value == prefix;
  },
}:
assert lib.assertMsg (builtins.isPath sha256Path || builtins.isString sha256Path)
"rs-harbor: findLocalMavenCache `sha256Path` must be a path or string";
assert lib.assertMsg (builtins.isPath hostPath || builtins.isString hostPath)
"rs-harbor: findLocalMavenCache `hostPath` must be a path or string";
assert lib.assertMsg (builtins.isString name && name != "")
"rs-harbor: findLocalMavenCache `name` must be a non-empty string";
assert lib.assertMsg (builtins.isBool recursive)
"rs-harbor: findLocalMavenCache `recursive` must be a boolean"; let
  hostPathString = toString hostPath;

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
    then throw "rs-harbor: findLocalMavenCache `${toString sha256Path}` is empty or contains no hash"
    else builtins.head hashMatch;
  validHash =
    hash
    == null
    || builtins.match "[0-9a-fA-F]{64}" hash != null
    || builtins.match "[0-9abcdfghijklmnpqrsvwxyz]{52}" hash != null
    || builtins.match "sha256-[A-Za-z0-9+/]{43}=" hash != null;
  normalizedHash =
    if hash == null
    then null
    else if !validHash
    then throw "rs-harbor: findLocalMavenCache `${toString sha256Path}` does not contain a valid SHA-256 hash"
    else
      builtins.convertHash {
        inherit hash;
        hashAlgo = "sha256";
        toHashFormat = "sri";
      };
in
  assert hash == null || validHash || throw "rs-harbor: findLocalMavenCache `${toString sha256Path}` does not contain a valid SHA-256 hash";
    if hash == null || !(builtins.pathExists hostPath)
    then null
    else
      assert lib.assertMsg (builtins.isPath hostPath || lib.hasPrefix "/" hostPathString)
      "rs-harbor: findLocalMavenCache `hostPath` must be absolute when passed as a string";
        builtins.path {
          path = hostPath;
          inherit name recursive;
          sha256 = normalizedHash;
        }
