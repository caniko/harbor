{
  harbor,
  opencodeLsp,
  pkgs,
  toolchain,
  cross,
  cargoConfig,
  rsHarborCli,
}: let
  docsPackages = with pkgs; [
    mdbook
  ];
  rsHarborShellTools = harbor.mkProjectCliShellTools {
    inherit pkgs;
    package = rsHarborCli;
    commandName = "rs-harbor";
    hint = "rs-harbor dev shell - run `rs-harbor --help`";
  };
  docsShellHook = ''
    echo "Documentation: mdbook serve docs"
  '';
in
  (harbor.mkDevShells {
    inherit pkgs cross cargoConfig;
    inherit (toolchain) craneLib;
    packages = docsPackages ++ rsHarborShellTools.packages;
    extraShellHook = docsShellHook + rsHarborShellTools.shellHook;
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
