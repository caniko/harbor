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
        harborRoot = self;
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
  bootstrap = system:
    import ./bootstrap.nix {
      inherit self inputs;
      pkgs = pkgsFor system {overlays = [inputs.rust-overlay.overlays.default];};
    };
  templatePlinth =
    inputs.plinth-source
    // {
      packages = nixlib.genAttrs systems (system: {plinth-project = (bootstrap system).plinthProject;});
    };
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
  companions = system: let
    pkgs = pkgsFor system {};
  in
    {
      open-pencil-cli = pkgs.callPackage ./packages/open-pencil-cli {};
    }
    // nixlib.optionalAttrs (system == "x86_64-linux") {
      open-pencil-desktop = pkgs.callPackage ./packages/open-pencil-desktop {};
    }
    // nixlib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux (import ./packages/blender-mcp {inherit pkgs;});
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
    tooling = import ./tooling/selection.nix {lib = nixlib;};
  };
  # Transitional output adapters, not independent input/source owners.
  legacy = aliases;
  sccache = components.rust.sccache;
  nixosModules = collectModules "nixosModules" // {default = import ./modules/nixos.nix {inherit self;};};
  homeManagerModules =
    collectModules "homeManagerModules"
    // {
      default = import ./modules/home.nix {inherit self;};
      fleetix = import ./modules/fleetix.nix {inherit self;};
    };
  treefmtModules = collectModules "treefmtModules";
  templates = nixlib.foldl' (result: name: result // nixlib.mapAttrs' (key: value: nixlib.nameValuePair "${name}-${key}" value) (components.${name}.templates or {})) {} componentNames;
  packages = nixlib.genAttrs systems (system:
    collect "packages" system
    // companions system
    // nixlib.optionalAttrs (builtins.hasAttr system components.rust.packages) {
      default = components.rust.packages.${system}.harbor-rs;
    });
  checks = nixlib.genAttrs systems (system: let
    pkgs = pkgsFor system {};
    templateContract = import ./template-contract.nix {
      inherit self inputs pkgs;
      plinth = templatePlinth;
    };
    bootstrapInputs = import ./bootstrap-contract.nix {inherit self inputs pkgs;};
    moduleContracts = import ./tooling/tests/modules.nix {inherit self inputs pkgs;};
    desktopChecks = nixlib.optionalAttrs (system == "x86_64-linux") (import ./tooling/tests/open-pencil.nix {
      inherit pkgs;
      openPencilCli = self.packages.${system}.open-pencil-cli;
      openPencilDesktop = self.packages.${system}.open-pencil-desktop;
    });
    blenderCheck = nixlib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      blender-mcp-runtime = import ./tooling/tests/blender.nix {inherit self inputs pkgs;};
    };
    # Core's retained component outputs are Linux-scoped; its shared shell
    # library also serves the portable Rust and Python Darwin consumers.
    portableCoreChecks = nixlib.optionalAttrs (!pkgs.stdenv.hostPlatform.isLinux) {
      scoped-runtime = import ../components/core/checks/scoped-runtime.nix {
        inherit pkgs;
        devShell = self.lib.core.devShell;
      };
    };
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
          // nixlib.optionalAttrs (name == "core") ({
              source-materialization = sourceMaterialization;
              template-contract = templateContract;
              bootstrap-inputs = bootstrapInputs;
              module-contracts = moduleContracts;
            }
            // portableCoreChecks)
          // nixlib.optionalAttrs (name == "cad" && system == "x86_64-linux") {
            dependency-links = desktopChecks.open-pencil-dependency-links;
          }))))
    componentNames);
  in
    collect "checks" system
    // nixlib.mapAttrs' (key: value: nixlib.nameValuePair "core-${key}" value) portableCoreChecks
    // groups
    // desktopChecks
    // blenderCheck
    // {
      template-contract = templateContract;
      bootstrap-inputs = bootstrapInputs;
      harbor-modules = moduleContracts;
      formatting = formatters.${system}.config.build.check self;
    });
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
    # Native workspace tests consume the DB check's existing immutable fixtures.
    # Machine-ID substitution belongs only to sandboxed testing-feature checks;
    # a native shell must retain the installed machine's custody identity.
    databaseTestEnvironment = nixlib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux (let
      databaseTest = self.checks.${system}.database-cargo-test;
    in {
      inherit (databaseTest) HARBOR_DB_TEST_POSTGRES HARBOR_DB_TEST_POSTGRES_17 HARBOR_DB_TEST_CONFIG_FIXTURE;
      HARBOR_DB_TEST_ROOT_UID = "0";
    });
    shell = pkgs.mkShell (databaseTestEnvironment
      // {
        packages = [toolchain.rustToolchain pkgs.pkg-config pkgs.openssl pkgs.git pkgs.python3 pkgs.uv pkgs.jq formatters.${system}.config.build.wrapper] ++ nixlib.optionals pkgs.stdenv.hostPlatform.isLinux [pkgs.util-linux];
        shellHook = nixlib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          # nix-direnv may cache exports after Nix removes its shell TMPDIR.
          export HARBOR_DB_TEST_TMPDIR="''${XDG_RUNTIME_DIR:-/tmp}"
        '';
      });
  in
    collect "devShells" system
    // {
      default = shell;
    }
    // {
      ci = pkgs.mkShell (databaseTestEnvironment
        // {
          inputsFrom = [shell];
          packages = [(bootstrap system).simit];
        });
    });
}
