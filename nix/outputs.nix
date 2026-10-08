inputs @ {
  self,
  nixpkgs,
  ...
}: let
  nixlib = nixpkgs.lib;
  componentNames = ["core" "rust" "python" "javascript" "go" "android" "tex" "database" "evm" "solana" "ntt" "llm" "cad" "projects" "macos-sdk"];
  aliases = {
    harbor-meta = components.core;
    meta-harbor = components.core;
    harbor-rs = components.rust;
    rs-harbor = components.rust;
    harbor-py = components.python;
    py-harbor = components.python;
    harbor-android = components.android;
  };
  # Import component output modules with one shared input graph. Compatibility
  # adapters bind a component-local self so internal output recursion remains
  # valid without evaluating component flakes or their former lockfiles.
  components = nixlib.genAttrs componentNames (name: let
    source = ../components + "/${name}";
    # Retain the shared flake source's context on the component path. Builders
    # depend on that source on fresh runners, and read-only flake evaluation
    # does not need to register a new store copy of every component.
    componentSource = self.outPath + "/components/${name}";
    componentInputs =
      inputs
      // aliases
      // {
        self = component;
        harborFormatting = system: formatters.${system}.config.build.check self;
      };
    component =
      outputs
      // {
        outPath = componentSource;
        inputs = componentInputs;
        sourceInfo = (self.sourceInfo or {}) // {outPath = componentSource;};
        rev = self.rev or null;
        lastModified = self.lastModified or 0;
        narHash = self.narHash or "";
        __toString = _: toString componentSource;
      };
    outputs = (import (source + "/flake.nix")).outputs componentInputs;
  in
    component);
  systems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  pkgsFor = system: args:
    import (
      if system == "x86_64-darwin"
      then inputs.nixpkgs-darwin
      else nixpkgs
    ) (args // {inherit system;});
  collect = output: system:
    nixlib.foldl' (result: name: result // nixlib.mapAttrs' (key: value: nixlib.nameValuePair "${name}-${key}" value) (components.${name}.${output}.${system} or {})) {} componentNames;
  collectModules = output:
    nixlib.foldl' (result: name: result // nixlib.mapAttrs' (key: value: nixlib.nameValuePair "${name}-${key}" value) (components.${name}.${output} or {})) {} componentNames;
  formatters = nixlib.genAttrs systems (system: let
    pkgs = pkgsFor system {
      overlays = [inputs.rust-overlay.overlays.default];
    };
    toolchain = self.lib.rust.mkToolchain {
      inherit pkgs;
      toolchainProfile = "nightly";
      cache.enable = false;
    };
  in
    inputs.treefmt-nix.lib.evalModule pkgs (import ./treefmt.nix {rustfmtPackage = toolchain.rustToolchain;}));
in {
  lib = {
    core = components.core.lib;
    rust = components.rust.lib;
    python = components.python.lib;
    javascript = components.javascript.lib;
    go = components.go.lib;
    android = components.android.lib;
    tex = components.tex.lib;
    docs = components.projects.lib;
    database = components.database.lib;
    cad = components.cad.lib;
    llm = components.llm.lib;
    blockchain = {
      evm = components.evm.lib;
      solana = components.solana.lib;
      ntt = components.ntt.lib;
    };
    pins.macosSdk = (import ../components/macos-sdk/flake.nix).outputs {};
  };
  # Transitional output adapters, not independent input/source owners.
  legacy = aliases;
  sccache = components.rust.sccache;
  nixosModules = collectModules "nixosModules";
  homeManagerModules = collectModules "homeManagerModules";
  treefmtModules = collectModules "treefmtModules";
  templates = nixlib.foldl' (result: name: result // nixlib.mapAttrs' (key: value: nixlib.nameValuePair "${name}-${key}" value) (components.${name}.templates or {})) {} componentNames;
  packages = nixlib.genAttrs systems (system:
    collect "packages" system
    // nixlib.optionalAttrs (builtins.hasAttr system components.rust.packages) {
      default = components.rust.packages.${system}.harbor-rs;
    });
  checks = nixlib.genAttrs systems (system: let
    pkgs = pkgsFor system {};
    sourceMaterialization = assert builtins.all (name: builtins.getContext (toString components.${name}) != {}) componentNames;
      pkgs.runCommand "harbor-component-source-materialization" {} ''
        set -euo pipefail
        ${nixlib.concatMapStringsSep "\n" (name: ''
            test -d ${components.${name}}
            test -f ${components.${name}}/flake.nix
          '')
          componentNames}
        touch "$out"
      '';
    groups = nixlib.listToAttrs (map (name:
      nixlib.nameValuePair "component-${name}" (pkgs.linkFarm "harbor-${name}-qualification" (nixlib.mapAttrsToList (check: path: {
          name = check;
          inherit path;
        }) ((components.${name}.checks.${system} or {})
          // nixlib.optionalAttrs (name == "core") {source-materialization = sourceMaterialization;}))))
    componentNames);
  in
    collect "checks" system // groups // {formatting = formatters.${system}.config.build.check self;});
  formatter = nixlib.genAttrs systems (system: formatters.${system}.config.build.wrapper);
  apps = nixlib.genAttrs systems (collect "apps");
  devShells = nixlib.genAttrs systems (system: let
    pkgs = pkgsFor system {
      overlays = [inputs.rust-overlay.overlays.default];
    };
    toolchain = self.lib.rust.mkToolchain {
      inherit pkgs;
      toolchainProfile = "nightly";
      cache.enable = false;
    };
    shell = pkgs.mkShell {
      packages = [toolchain.rustToolchain pkgs.pkg-config pkgs.openssl pkgs.git pkgs.python3 pkgs.uv pkgs.jq formatters.${system}.config.build.wrapper];
    };
  in
    collect "devShells" system
    // {
      default = shell;
    }
    // nixlib.optionalAttrs (builtins.hasAttr system inputs.simit.packages) {
      ci = pkgs.mkShell {
        inputsFrom = [shell];
        packages = [inputs.simit.packages.${system}.default];
      };
    });
}
