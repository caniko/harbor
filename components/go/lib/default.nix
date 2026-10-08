{harbor-meta}: rec {
  timezone = harbor-meta.lib.timezone;
  mkGoToolchain = {
    pkgs,
    go ? pkgs.buildPackages.go_1_27,
    gopls ? null,
    golangciLint ? null,
  }: let
    buildGoModule = pkgs.buildGoModule.override {inherit go;};
  in {
    inherit go buildGoModule;
    gopls =
      if gopls != null
      then gopls
      else pkgs.gopls.override {buildGoLatestModule = buildGoModule;};
    golangciLint =
      if golangciLint != null
      then golangciLint
      else import ../nix/golangci-lint.nix {inherit pkgs buildGoModule;};
  };

  mkGoDevShellFragment = {
    pkgs,
    toolchain ? mkGoToolchain {inherit pkgs;},
    cgo ? true,
  }: {
    packages =
      [toolchain.go toolchain.gopls toolchain.golangciLint]
      ++ pkgs.lib.optionals cgo [pkgs.stdenv.cc pkgs.pkg-config];
    env = {
      GOTOOLCHAIN = "local";
      CGO_ENABLED =
        if cgo
        then "1"
        else "0";
    };
    shellHook = "";
  };

  mkGoDevShell = {
    pkgs,
    timeZone ? "UTC",
    toolchain ? mkGoToolchain {inherit pkgs;},
    cgo ? true,
    packages ? [],
    env ? {},
    extraShellHook ? "",
    mkShellArgs ? {},
  }:
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs timeZone packages env extraShellHook mkShellArgs;
      fragments = [(mkGoDevShellFragment {inherit pkgs toolchain cgo;})];
    };

  mkGoPackage = {
    pkgs,
    toolchain ? mkGoToolchain {inherit pkgs;},
    ...
  } @ args:
    toolchain.buildGoModule (
      builtins.removeAttrs args ["pkgs" "toolchain"]
      // {env = {GOTOOLCHAIN = "local";} // (args.env or {});}
    );
}
