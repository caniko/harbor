{
  harbor,
  opencodeLsp,
  pkgs,
  toolchain,
  cross,
  cargoConfig,
  rsHarborCli,
  harborCi,
  treefmtWrapper,
}: let
  docsPackages = with pkgs; [
    mdbook
  ];
  rsHarborShellTools = harbor.mkProjectCliShellTools {
    inherit pkgs;
    package = rsHarborCli;
    commandName = "harbor-rs";
    hint = "harbor-rs dev shell - run `harbor-rs --help`";
  };
  harborCiShellTools = harbor.mkProjectCliShellTools {
    inherit pkgs;
    package = harborCi;
    commandName = "harbor-ci";
    hint = "harbor-ci is available; run `harbor-ci default`";
  };
  docsShellHook = ''
    echo "Documentation: mdbook serve docs"
  '';
  opencodeLspShell = opencodeLsp.mkShell {
    inherit pkgs;
    rustAnalyzer = toolchain.rustToolchain;
  };
in
  (harbor.mkDevShells {
    inherit pkgs cross cargoConfig;
    inherit (toolchain) craneLib;
    packages = [treefmtWrapper pkgs.jq] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.bubblewrap pkgs.dbus pkgs.sway] ++ docsPackages ++ harborCiShellTools.packages ++ rsHarborShellTools.packages;
    extraEnv = pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      HARBOR_TEST_BWRAP = pkgs.lib.getExe pkgs.bubblewrap;
      HARBOR_TEST_BASH = pkgs.lib.getExe pkgs.bash;
      HARBOR_TEST_PATH = pkgs.lib.makeBinPath [pkgs.bash pkgs.coreutils pkgs.gnugrep];
      HARBOR_TEST_RUST_PATH = pkgs.lib.makeBinPath [toolchain.rustToolchain pkgs.gcc pkgs.binutils];
      HARBOR_TEST_DBUS = "${pkgs.dbus}/bin/dbus-run-session";
      HARBOR_TEST_SWAY = pkgs.lib.getExe pkgs.sway;
    };
    extraShellHook = docsShellHook + rsHarborShellTools.shellHook + harborCiShellTools.shellHook + "\n" + opencodeLspShell.shellHook;
  })
  // {
    docs = harbor.mkDocsShell {
      inherit pkgs cross cargoConfig;
      inherit (toolchain) craneLib;
      packages = docsPackages;
      extraShellHook = docsShellHook;
    };
    opencode-lsp = opencodeLspShell;
  }
