{
  description = "Polyglot project: stable Rust, Python (uv) and TypeScript (Node) on Harbor";

  inputs = {
    harbor.url = "git+https://github.com/caniko/harbor.git?ref=feat/harbor-monorepo-components&rev=a623c2db712a823f75b29523940fc7432dea49aa";
    nixpkgs.follows = "harbor/nixpkgs";
    treefmt-nix.follows = "harbor/treefmt-nix";
  };

  outputs = {
    harbor,
    treefmt-nix,
    ...
  }: let
    forSystem = f: harbor.lib.python.forAllSystems (system: f (harbor.lib.python.mkPkgs {inherit system;}));
    treefmt = pkgs:
      (treefmt-nix.lib.evalModule pkgs {
        imports = with harbor.treefmtModules; [core-nix core-toml python-python javascript-javascript rust-rust];
        projectRootFile = "flake.nix";
        settings.global.excludes = ["Cargo.lock" "uv.lock" "package-lock.json" "node_modules/**" ".vscode/**"];
      }).config.build.wrapper;
  in {
    formatter = forSystem treefmt;

    devShells = forSystem (pkgs: {
      default = harbor.lib.core.devShell.mkShell {
        inherit pkgs;
        shells = [
          # Pinned stable Rust, host stdlib only; rust-analyzer and clippy included.
          (harbor.lib.rust.mkToolchainShell {inherit pkgs;})
          # Syncs .venv from pyproject.toml (`--group dev`) on entry.
          (harbor.lib.python.mkUvDevShell {inherit pkgs;})
          # Node 22 LTS runs .ts files and `node --test` natively.
          (harbor.lib.javascript.mkNodeDevShell {
            inherit pkgs;
            nodejs = pkgs.nodejs_22;
          })
        ];
        packages = [(treefmt pkgs)];
      };
    });
  };
}
