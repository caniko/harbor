{harbor-meta}: rec {
  mkNtt = {
    pkgs,
    pins,
    anchorRustToolchain,
  }:
    if pkgs.stdenv.hostPlatform.system != "x86_64-linux"
    then throw "harbor-ntt: only x86_64-linux is supported"
    else let
      sources = import ./sources.nix {inherit pkgs pins;};
      cli = import ./cli.nix {inherit pkgs pins sources;};
      evm = import ./evm.nix {inherit pkgs pins sources;};
      solana = import ./solana.nix {
        inherit pkgs pins sources anchorRustToolchain;
      };
      pinCheck = import ./pin-check.nix {
        inherit pkgs pins sources;
      };
      fragment = {
        packages = [
          cli.package
          cli.bun
          evm.foundry
          evm.solc
          solana.anchor
          solana.node
          solana.solana
          anchorRustToolchain
        ];
        env = {};
        shellHook = ''
          export CARGO_NET_OFFLINE=true
          export FOUNDRY_OFFLINE=true
          export FOUNDRY_SOLC=${evm.solc}/bin/solc
        '';
      };
    in {
      inherit sources cli evm solana pinCheck fragment;
    };

  mkNttDevShell = {
    pkgs,
    pins,
    anchorRustToolchain,
    extraPackages ? [],
    extraEnv ? {},
    extraShellHook ? "",
  }: let
    ntt = mkNtt {inherit pkgs pins anchorRustToolchain;};
  in
    harbor-meta.lib.devShell.mkShell {
      inherit pkgs;
      fragments = [ntt.fragment];
      packages = extraPackages;
      env = extraEnv;
      inherit extraShellHook;
    };
}
