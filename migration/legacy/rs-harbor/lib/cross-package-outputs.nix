# mkCrossPackageOutputs :: {
#   buildSystem, hostSystem, ...mkCrossPackages args
# } -> {
#   packages = flat mkCrossPackages output;
#   crossPackages.<build-system>.<host-system> = packages;
# }
#
# The flat `packages` member preserves the existing helper contract. The
# namespaced member gives NixOS consumers a stable way to select an explicit
# cross package without guessing a project's output naming convention.
{mkCrossPackages}: args @ {
  buildSystem,
  hostSystem,
  ...
}: let
  packages = mkCrossPackages (builtins.removeAttrs args ["buildSystem" "hostSystem"]);
in {
  inherit packages;
  crossPackages = {
    ${buildSystem} = {
      ${hostSystem} = packages;
    };
  };
}
