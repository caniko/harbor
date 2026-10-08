# mkAppImage :: { nix-appimage, system, program, pname?, squashfsArgs? }
#            -> derivation
#
# Wrap a Nix-built executable as a self-contained AppImage.
# Requires the user to provide nix-appimage (github:ralismark/nix-appimage)
# as a flake input — rs-harbor does not pull it in by default.
#
# Example:
#   packages.appimage = rs-harbor.lib.mkAppImage {
#     inherit system nix-appimage;
#     program = "${myPackage}/bin/my-app";
#   };
{packageTests}: {
  nix-appimage,
  system,
  program,
  pname ? null,
  version ? "0.1.0",
  squashfsArgs ? [],
}: let
  packageName =
    if pname != null
    then pname
    else builtins.baseNameOf program;
  output = nix-appimage.lib.${system}.mkAppImage (
    {inherit program squashfsArgs;}
    // (
      if pname != null
      then {inherit pname;}
      else {}
    )
  );
in
  assert builtins.isString system;
  assert builtins.match ".*-linux" system
  != null
  || throw "mkAppImage: only Linux systems are supported (got ${system})";
  assert builtins.isString program;
  assert nix-appimage ? lib
  || throw "mkAppImage: nix-appimage input must expose lib (is it github:ralismark/nix-appimage?)";
    output
    // {
      artifactBuilder = packageTests.mkArtifactBuilder {
        kind = "appimage-builder";
        inherit packageName version;
        output = toString output;
        buildCommand = "nix build .#${packageName}-appimage";
        metadata = {
          inherit program squashfsArgs system;
          helper = "mkAppImage";
        };
      };
    }
