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
in
  (harbor.mkDevShells {
    inherit pkgs cross cargoConfig;
    inherit (toolchain) craneLib;
    packages = [treefmtWrapper] ++ docsPackages ++ harborCiShellTools.packages ++ rsHarborShellTools.packages;
    extraShellHook = docsShellHook + rsHarborShellTools.shellHook + harborCiShellTools.shellHook;
  })
  // {
    docs = harbor.mkDocsShell {
      inherit pkgs cross cargoConfig;
      inherit (toolchain) craneLib;
      packages = docsPackages;
      extraShellHook = docsShellHook;
    };
    opencode-lsp = opencodeLsp.mkShell {
      inherit pkgs;
      rustAnalyzer = toolchain.rustToolchain;
    };
  }
