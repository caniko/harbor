# mkGradlePackage :: {
#   pkgs,
#   pname,
#   version,
#   src,
#   depsJson,
#   artifactPath,
#   projectDir ?, artifactName ?, gradle ?, jdk ?,
#   gradleBuildTask ?, gradleCheckTask ?, gradleFlags ?, doCheck ?,
#   nativeBuildInputs ?, preBuild ?, postBuild ?, ...
# } -> derivation
#
# Build a Gradle artifact with nixpkgs' reproducible MITM dependency cache.
# This helper deliberately owns only the generic build mechanics; packaging
# metadata, signing, and publication policy stay with the consuming project.
{
  pkgs,
  pname,
  version,
  src,
  depsJson,
  artifactPath,
  projectDir ? ".",
  artifactName ? pkgs.lib.last (pkgs.lib.splitString "/" artifactPath),
  gradle ? pkgs.gradle_9,
  jdk ? pkgs.jdk21,
  gradleBuildTask ? "assemble",
  gradleCheckTask ? "test",
  gradleUpdateTask ? "nixDownloadDeps",
  gradleFlags ? [],
  doCheck ? true,
  nativeBuildInputs ? [],
  preBuild ? "",
  postBuild ? "",
  ...
}: let
  inherit (pkgs) lib;

  pathSegments = lib.splitString "/";
  isRelativePath = path:
    builtins.isString path
    && path != ""
    && !(lib.hasPrefix "/" path)
    && lib.all (segment: segment != "" && segment != "..") (pathSegments path);
  gradleFlagsText = lib.escapeShellArgs gradleFlags;
in
  assert builtins.isString pname
  && pname != ""
  || throw "mkGradlePackage: pname must be a non-empty string";
  assert builtins.isString version
  && version != ""
  || throw "mkGradlePackage: version must be a non-empty string";
  assert isRelativePath projectDir
  || throw "mkGradlePackage: projectDir must be a relative path without '..' segments";
  assert isRelativePath artifactPath
  || throw "mkGradlePackage: artifactPath must be a relative path without '..' segments";
  assert builtins.isString artifactName
  && artifactName != ""
  || throw "mkGradlePackage: artifactName must be a non-empty string";
  assert builtins.pathExists depsJson
  || throw "mkGradlePackage: depsJson does not exist: ${toString depsJson}";
    pkgs.stdenv.mkDerivation (finalAttrs: {
      inherit pname version src;

      nativeBuildInputs = [gradle jdk] ++ nativeBuildInputs;

      # finalPackage is intentional: the update script must use precisely the
      # same project arguments as the derivation that consumes its cache.
      mitmCache = gradle.fetchDeps {
        pkg = finalAttrs.finalPackage;
        data = depsJson;
      };

      __darwinAllowLocalNetworking = true;
      inherit gradleBuildTask gradleCheckTask gradleUpdateTask doCheck;
      inherit gradleFlags;

      buildPhase = ''
        runHook preBuild
        cd ${lib.escapeShellArg projectDir}
        gradle ${gradleFlagsText} ${gradleBuildTask}
        cd - >/dev/null
        runHook postBuild
      '';

      inherit preBuild postBuild;

      checkPhase = lib.optionalString doCheck ''
        cd ${lib.escapeShellArg projectDir}
        gradle ${gradleFlagsText} ${gradleCheckTask}
        cd - >/dev/null
      '';

      installPhase = ''
        artifact=${lib.escapeShellArg projectDir}/${lib.escapeShellArg artifactPath}
        test -f "$artifact" || {
          echo "mkGradlePackage: expected Gradle artifact is missing: $artifact" >&2
          exit 1
        }
        mkdir -p "$out"
        cp "$artifact" "$out/${lib.escapeShellArg artifactName}"
      '';

      passthru.rsHarbor = {
        helper = "mkGradlePackage";
        inherit artifactPath artifactName projectDir gradleBuildTask gradleCheckTask;
      };
    })
